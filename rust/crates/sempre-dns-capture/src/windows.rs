mod cleanup;
mod owner;

use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Read as _, Write as _},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Semaphore,
    time::timeout,
};
use windivert::prelude::{NetworkLayer, WinDivert, WinDivertFlags, WinDivertPacket};

use crate::packet::{Error, Flow, Packet};
use windivert_sys::ChecksumFlags;

type Connections = Arc<Mutex<HashMap<Flow, Instant>>>;
const IDLE: Duration = Duration::from_mins(2);
const EXCHANGE: Duration = Duration::from_secs(30);
const MAX_CONNECTIONS: usize = 2048;

pub fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--cleanup") && args.next().is_none() {
        return cleanup::run();
    }
    let backend: SocketAddr = first.ok_or("missing DNS frontend address")?.parse()?;
    let parent: u32 = args.next().ok_or("missing daemon process ID")?.parse()?;
    if !backend.ip().is_loopback() || backend.port() == 53 || args.next().is_some() {
        return Err("DNS capture requires a loopback frontend on a non-53 port".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let _entered = runtime.enter();
    let tcp = listener(Ipv4Addr::UNSPECIFIED.into(), 0)?;
    let port = tcp.local_addr()?.port();
    let tcp6 = listener(Ipv6Addr::UNSPECIFIED.into(), port)?;
    let connections: Connections = Arc::default();
    for listener in [tcp, tcp6] {
        runtime.spawn(serve_tcp(listener, Arc::clone(&connections), backend));
    }
    let filter =
        format!("outbound and (udp.DstPort == 53 or tcp.DstPort == 53 or tcp.SrcPort == {port})");
    let divert = Arc::new(WinDivert::network(&filter, 30000, WinDivertFlags::new())?);
    // The daemon exclusively owns stdin. EOF also handles daemon crashes and upgrades.
    std::thread::spawn(|| {
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        std::process::exit(0);
    });
    println!("READY");
    std::io::stdout().flush()?;
    let capacity = Arc::new(Semaphore::new(256));
    let mut owners = owner::Owners::new(parent);
    let mut buffer = vec![0; 65575];
    loop {
        let mut packet = divert.recv(Some(&mut buffer))?.into_owned();
        let Ok(parsed) = Packet::parse(&packet.data) else {
            divert.send(&packet)?;
            continue;
        };
        if !parsed.tcp {
            if owners.bypass(&parsed) {
                divert.send(&packet)?;
                continue;
            }
            let Ok(permit) = Arc::clone(&capacity).try_acquire_owned() else {
                divert.send(&packet)?;
                continue;
            };
            let sender = Arc::clone(&divert);
            runtime.spawn(async move {
                let _permit = permit;
                match timeout(EXCHANGE, udp_exchange(&sender, packet, backend)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => eprintln!("DNS capture UDP exchange failed: {error}"),
                    Err(error) => eprintln!("DNS capture UDP timeout: {error}"),
                }
            });
            continue;
        }
        let response = parsed.source.port() == port;
        let flow = parsed.flow(response);
        let mut flows = connections.lock().expect("DNS connections");
        flows.retain(|_, activity| activity.elapsed() < IDLE);
        if !flows.contains_key(&flow)
            && (response || !parsed.syn || flows.len() >= MAX_CONNECTIONS || owners.bypass(&parsed))
        {
            drop(flows);
            divert.send(&packet)?;
            continue;
        }
        flows.insert(flow, Instant::now());
        drop(flows);
        let mut reflected = packet.data.to_vec();
        parsed.reflect(&mut reflected, port, response);
        packet.data = Cow::Owned(reflected);
        packet.address.set_outbound(packet.address.loopback());
        packet.recalculate_checksums(ChecksumFlags::default())?;
        divert.send(&packet)?;
    }
}

async fn udp_exchange(
    divert: &WinDivert<NetworkLayer>,
    mut packet: WinDivertPacket<'static, NetworkLayer>,
    backend: SocketAddr,
) -> Result<(), Error> {
    let parsed = Packet::parse(&packet.data)?;
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    socket.connect(backend).await?;
    socket.send(parsed.payload).await?;
    let mut answer = vec![0; 65535];
    let length = socket.recv(&mut answer).await?;
    packet.data = Cow::Owned(parsed.udp_response(&answer[..length])?);
    packet.address.set_outbound(packet.address.loopback());
    packet.recalculate_checksums(ChecksumFlags::default())?;
    divert.send(&packet)?;
    Ok(())
}

fn listener(ip: IpAddr, port: u16) -> std::io::Result<TcpListener> {
    let socket = Socket::new(
        if ip.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        },
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    if ip.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.bind(&SocketAddr::new(ip, port).into())?;
    socket.listen(128)?;
    socket.set_nonblocking(true)?;
    TcpListener::from_std(socket.into())
}

async fn serve_tcp(listener: TcpListener, connections: Connections, backend: SocketAddr) {
    let capacity = Arc::new(Semaphore::new(256));
    while let Ok((mut client, peer)) = listener.accept().await {
        let Ok(local) = client.local_addr() else {
            continue;
        };
        let flow = Flow {
            client: SocketAddr::new(local.ip(), peer.port()),
            server: peer.ip(),
        };
        if !connections
            .lock()
            .expect("DNS connections")
            .contains_key(&flow)
        {
            continue;
        }
        let Ok(permit) = Arc::clone(&capacity).try_acquire_owned() else {
            continue;
        };
        tokio::spawn(async move {
            let _permit = permit;
            let result = timeout(Duration::from_secs(30), async {
                let mut upstream = TcpStream::connect(backend).await?;
                copy_bidirectional(&mut client, &mut upstream).await
            })
            .await;
            if let Ok(Err(error)) = result {
                eprintln!("DNS capture TCP exchange failed: {error}");
            }
        });
    }
}
