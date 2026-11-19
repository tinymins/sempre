use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use sempre_dns::DnsQueryEvent;

use crate::dns_query_segments::Segments;

const MAX_PENDING: usize = 2048;
const REPORT_INTERVAL: Duration = Duration::from_secs(30);

type Operation = Box<dyn FnOnce(&mut Writer) + Send>;

enum Message {
    Record(Arc<DnsQueryEvent>),
    Control(Operation),
    Stop,
}

struct Memory {
    events: VecDeque<Arc<DnsQueryEvent>>,
    limit: usize,
}

pub(super) struct QueryHistory {
    memory: Mutex<Memory>,
    sender: Sender<Message>,
    pending: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}

impl QueryHistory {
    pub(super) fn open(path: PathBuf, limit: usize, log: PathBuf) -> io::Result<Self> {
        let (_, events) = Segments::open(&path, limit)?;
        let (sender, receiver) = mpsc::channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let writer_pending = pending.clone();
        let writer_dropped = dropped.clone();
        let writer = Writer {
            path,
            initial_limit: limit,
            storage: None,
            log,
        };
        let worker = thread::Builder::new()
            .name("dns-query-log".into())
            .spawn(move || writer.run(&receiver, &writer_pending, &writer_dropped))?;
        Ok(Self {
            memory: Mutex::new(Memory { events, limit }),
            sender,
            pending,
            dropped,
            worker: Some(worker),
        })
    }

    pub(super) fn record(&self, event: DnsQueryEvent) {
        let event = Arc::new(event);
        let mut memory = self.memory.lock().expect("DNS query history lock");
        if memory.events.len() == memory.limit {
            memory.events.pop_front();
        }
        memory.events.push_back(event.clone());
        if self.pending.load(Ordering::Relaxed) < MAX_PENDING {
            self.pending.fetch_add(1, Ordering::Relaxed);
            if self.sender.send(Message::Record(event)).is_err() {
                self.pending.fetch_sub(1, Ordering::Relaxed);
            }
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn recent(&self) -> Vec<DnsQueryEvent> {
        self.memory
            .lock()
            .expect("DNS query history lock")
            .events
            .iter()
            .rev()
            .map(|event| (**event).clone())
            .collect()
    }

    // Mutating administration calls are serialized by DnsSettingsStore. Only
    // queue the fence under the history lock, never wait for disk while holding it.
    pub(super) fn resize_and_save(
        &self,
        limit: usize,
        save: impl FnOnce() -> io::Result<()> + Send + 'static,
    ) -> io::Result<()> {
        let (old_limit, result) = {
            let mut memory = self.memory.lock().expect("DNS query history lock");
            let old_limit = memory.limit;
            let result = self.control(move |writer| {
                let storage = writer.storage()?;
                if limit > old_limit {
                    storage.pin_retention()?;
                }
                save()?;
                // A resize takes effect immediately. Physical removal of whole
                // expired segments can be retried by the next append if necessary.
                storage.set_limit(limit);
                writer.prune_after_commit();
                Ok(())
            })?;
            memory.limit = limit.max(old_limit);
            (old_limit, result)
        };
        let result = result
            .recv()
            .unwrap_or_else(|error| Err(io::Error::other(error)));
        let mut memory = self.memory.lock().expect("DNS query history lock");
        memory.limit = if result.is_ok() { limit } else { old_limit };
        let remove = memory.events.len().saturating_sub(memory.limit);
        memory.events.drain(..remove);
        result
    }

    pub(super) fn clear(&self) -> io::Result<()> {
        let (mut previous, result) = {
            let mut memory = self.memory.lock().expect("DNS query history lock");
            let result = self.control(|writer| {
                writer.storage()?.clear()?;
                writer.prune_after_commit();
                Ok(())
            })?;
            (std::mem::take(&mut memory.events), result)
        };
        let result = result
            .recv()
            .unwrap_or_else(|error| Err(io::Error::other(error)));
        if result.is_err() {
            let mut memory = self.memory.lock().expect("DNS query history lock");
            previous.append(&mut memory.events);
            let remove = previous.len().saturating_sub(memory.limit);
            previous.drain(..remove);
            memory.events = previous;
        }
        result
    }

    fn control(
        &self,
        action: impl FnOnce(&mut Writer) -> io::Result<()> + Send + 'static,
    ) -> io::Result<mpsc::Receiver<io::Result<()>>> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Message::Control(Box::new(move |writer| {
                let _ = reply.send(action(writer));
            })))
            .map_err(|_| io::Error::other("DNS query writer stopped"))?;
        Ok(result)
    }
}

impl Drop for QueryHistory {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Writer {
    log: PathBuf,
    path: PathBuf,
    initial_limit: usize,
    storage: Option<(File, Segments)>,
}

impl Writer {
    fn run(
        mut self,
        receiver: &mpsc::Receiver<Message>,
        pending: &AtomicUsize,
        dropped: &AtomicUsize,
    ) {
        let mut failures = 0;
        let mut detail = String::new();
        let mut last_report = None;
        while let Ok(message) = receiver.recv() {
            let stopping = matches!(message, Message::Stop);
            match message {
                Message::Record(event) => {
                    if let Err(error) = self.storage().and_then(|s| s.append(&event)) {
                        failures += 1;
                        detail = error.to_string();
                    }
                    pending.fetch_sub(1, Ordering::Relaxed);
                }
                Message::Control(control) => control(&mut self),
                Message::Stop => {}
            }
            if stopping || last_report.is_none_or(|at: Instant| at.elapsed() >= REPORT_INTERVAL) {
                let lost = dropped.swap(0, Ordering::Relaxed);
                if failures > 0 || lost > 0 {
                    let message = format!(
                        "DNS query history: {failures} write failures, {lost} dropped from the full write queue. {detail}"
                    );
                    let _ = sempre_supervisor::append_log(&self.log, &message);
                    failures = 0;
                    detail.clear();
                    last_report = Some(Instant::now());
                }
            }
            if stopping {
                break;
            }
        }
    }

    // Logical retention is already durable. An unlink failure must not roll
    // back the in-memory window; the next write will retry the removal.
    fn prune_after_commit(&mut self) {
        if let Some((_, segments)) = &mut self.storage
            && let Err(error) = segments.prune()
        {
            let _ = sempre_supervisor::append_log(
                &self.log,
                &format!("DNS query history cleanup failed: {error}"),
            );
        }
    }

    // CLI commands can construct another Manager in the same directory.
    // Only a mutating store takes the lease and refreshes its on-disk cursor.
    fn storage(&mut self) -> io::Result<&mut Segments> {
        if self.storage.is_none() {
            let directory = self.path.with_extension("jsonl.d");
            fs::create_dir_all(&directory)?;
            let lease = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(directory.join("writer.lock"))?;
            lease.try_lock().map_err(io::Error::from)?;
            let (segments, _) = Segments::open(&self.path, self.initial_limit)?;
            self.storage = Some((lease, segments));
        }
        Ok(&mut self.storage.as_mut().expect("DNS query writer lease").1)
    }
}
