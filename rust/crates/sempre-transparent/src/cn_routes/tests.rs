use std::{
    collections::HashSet,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{
    Controller,
    backend::Backend,
    model::{Endpoint, Intent, Route, invalid, normalize, plan},
};
use crate::{TransparentError, command};

#[derive(Default)]
pub(super) struct Memory {
    pub(super) routes: Mutex<HashSet<Route>>,
    pub(super) events: Mutex<Vec<String>>,
    fail: AtomicBool,
    pub(super) fail_cleanup: AtomicBool,
    // Removal batches allowed before cleanup fails.
    pub(super) removal_batches: Mutex<Option<usize>>,
    paused: AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
    uplink: Mutex<Option<Endpoint>>,
}

impl Backend for Memory {
    async fn discover(
        &self,
        _: &dyn command::Runner,
        _: &Intent,
    ) -> Result<(Vec<Endpoint>, Endpoint), TransparentError> {
        let (uplink, tun) = endpoints();
        Ok((
            vec![
                self.uplink
                    .lock()
                    .expect("uplink")
                    .clone()
                    .unwrap_or(uplink),
            ],
            tun,
        ))
    }
    async fn observe(
        &self,
        _: &dyn command::Runner,
        candidates: &[Route],
    ) -> Result<Vec<Route>, TransparentError> {
        let installed = self.routes.lock().expect("routes");
        Ok(candidates
            .iter()
            .filter(|route| installed.contains(route))
            .cloned()
            .collect())
    }
    async fn preflight(
        &self,
        _: &dyn command::Runner,
        _: &[Route],
    ) -> Result<(), TransparentError> {
        Ok(())
    }
    async fn change(
        &self,
        _: &dyn command::Runner,
        routes: &[Route],
        add: bool,
    ) -> Result<(), TransparentError> {
        if add && self.paused.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        if !add && self.fail_cleanup.load(Ordering::SeqCst) {
            return Err(invalid("injected cleanup failure"));
        }
        if !add && let Some(left) = self.removal_batches.lock().expect("budget").as_mut() {
            if *left == 0 {
                return Err(invalid("injected interrupted cleanup"));
            }
            *left -= 1;
        }
        for route in routes {
            self.events
                .lock()
                .expect("events")
                .push(format!("{add}:{}", route.fallback));
            let mut installed = self.routes.lock().expect("routes");
            if add {
                installed.insert(route.clone());
            } else {
                installed.remove(route);
            }
            if add && self.fail.swap(false, Ordering::SeqCst) {
                return Err(invalid("injected partial batch failure"));
            }
        }
        Ok(())
    }
    async fn policy(
        &self,
        _: &dyn command::Runner,
        add: bool,
        _: bool,
    ) -> Result<(), TransparentError> {
        self.events
            .lock()
            .expect("events")
            .push(format!("policy:{add}"));
        Ok(())
    }
}

pub(super) fn endpoints() -> (Endpoint, Endpoint) {
    (
        Endpoint {
            interface: "en0".into(),
            index: 1,
            gateway: "192.168.1.1".into(),
        },
        Endpoint {
            interface: "utun9".into(),
            index: 2,
            gateway: String::new(),
        },
    )
}

pub(super) fn intent() -> Intent {
    Intent {
        direct: true,
        prefixes: vec!["223.5.5.0/24".into(), "114.114.114.0/24".into()],
        tun_addresses: vec!["172.19.0.1/30".into()],
        capture_prefixes: Vec::new(),
        intranet_exclusions: None,
        outbound_interfaces: [Some("en0".into()), None],
    }
}

#[test]
fn cn_wins_longest_prefix_and_literal_foreign_ip_uses_tun() {
    let (uplink, tun) = endpoints();
    let routes = plan(&intent(), std::slice::from_ref(&uplink), &tun).expect("plan");
    for (address, interface) in [
        ("223.5.5.5", "en0"),
        ("114.114.114.114", "en0"),
        ("1.1.1.1", "utun9"),
        ("10.23.0.200", "utun9"),
    ] {
        let address = address.parse::<std::net::IpAddr>().expect("IP");
        let route = routes
            .iter()
            .filter(|r| {
                !r.scoped
                    && r.prefix
                        .parse::<ipnet::IpNet>()
                        .expect("CIDR")
                        .contains(&address)
            })
            .max_by_key(|r| r.prefix.parse::<ipnet::IpNet>().expect("CIDR").prefix_len())
            .expect("route");
        assert_eq!(route.endpoint.interface, interface);
    }
    let scoped = routes
        .iter()
        .filter(|route| route.scoped)
        .collect::<Vec<_>>();
    assert_eq!(scoped.len(), usize::from(cfg!(target_os = "macos")));
    if let Some(route) = scoped.first() {
        assert_eq!(route.prefix, "0.0.0.0/0");
        assert_eq!(route.endpoint, uplink);
        assert!(!route.fallback);
        assert!(
            route
                .prefix
                .parse::<ipnet::IpNet>()
                .expect("default")
                .contains(&"10.23.0.200".parse::<std::net::IpAddr>().expect("IP"))
        );
        let mut journal = serde_json::to_value(route).expect("route journal");
        assert_eq!(
            serde_json::from_value::<Route>(journal.clone()).expect("round trip"),
            **route
        );
        journal.as_object_mut().expect("object").remove("scoped");
        assert!(serde_json::from_value::<Route>(journal).is_err());
    }
    for prefixes in [vec![], vec!["1.1.1.0/24".into()], vec!["0.0.0.0/0".into()]] {
        assert!(normalize(&prefixes).is_ok());
        let sparse = Intent {
            prefixes,
            ..intent()
        };
        assert!(plan(&sparse, std::slice::from_ref(&uplink), &tun).is_ok());
    }
    let v6 = Endpoint {
        interface: "en1".into(),
        index: 3,
        gateway: "fe80::1%en1".into(),
    };
    assert!(plan(&intent(), &[uplink, v6.clone()], &tun).is_ok());
    assert!(plan(&intent(), &[v6], &tun).is_ok());
    let overlapping = Intent {
        capture_prefixes: vec!["223.5.5.0/24".into()],
        ..intent()
    };
    assert!(plan(&overlapping, &[endpoints().0], &tun).is_ok());
}

#[tokio::test]
async fn installation_updates_and_cleanup_preserve_order() {
    let directory = tempfile::tempdir().expect("directory");
    let controller = Controller::<Memory>::new(Some(directory.path().join("routes.json")));
    let (mut uplink, tun) = endpoints();
    let original = plan(&intent(), std::slice::from_ref(&uplink), &tun).expect("plan");
    controller
        .reconcile(&command::SystemRunner, original.clone(), Some(false))
        .await
        .expect("install");
    let events = controller.backend.events.lock().expect("events").clone();
    let physical_count = original.iter().filter(|route| !route.fallback).count();
    assert!(
        events[..physical_count]
            .iter()
            .all(|event| event == "true:false")
    );
    assert_eq!(
        &events[physical_count..physical_count + 2],
        &["true:true", "true:true"]
    );
    controller.backend.events.lock().expect("events").clear();
    let mut next = intent();
    next.prefixes.push("120.0.0.0/8".into());
    let updated = plan(&next, std::slice::from_ref(&uplink), &tun).expect("plan");
    controller
        .reconcile(&command::SystemRunner, updated, Some(false))
        .await
        .expect("delta");
    assert_eq!(
        controller
            .backend
            .events
            .lock()
            .expect("events")
            .iter()
            .filter(|e| e.starts_with("true:"))
            .count(),
        1
    );
    uplink.gateway = "192.168.2.1".into();
    let changed = plan(&next, &[uplink], &tun).expect("new network");
    controller
        .reconcile(&command::SystemRunner, changed.clone(), Some(false))
        .await
        .expect("switch");
    assert_eq!(
        *controller.backend.routes.lock().expect("routes"),
        changed.into_iter().collect()
    );
    next.direct = false;
    let proxy = plan(&next, &[endpoints().0], &tun).expect("proxy plan");
    assert!(proxy.iter().all(|route| route.fallback || route.scoped));
    controller
        .reconcile(&command::SystemRunner, proxy.clone(), Some(false))
        .await
        .expect("proxy switch");
    assert_eq!(
        *controller.backend.routes.lock().expect("routes"),
        proxy.into_iter().collect()
    );
    controller.backend.events.lock().expect("events").clear();
    controller
        .cleanup(&command::SystemRunner)
        .await
        .expect("cleanup");
    let events = controller.backend.events.lock().expect("events");
    assert_eq!(&events[..3], &["policy:false", "false:true", "false:true"]);
    assert!(controller.backend.routes.lock().expect("routes").is_empty());
    assert!(!directory.path().join("routes.json").exists());
}

#[tokio::test]
async fn partial_batch_failure_and_stale_journal_are_recoverable() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("routes.json");
    let controller = std::sync::Arc::new(Controller::<Memory>::new(Some(path.clone())));
    let (uplink, tun) = endpoints();
    let routes = plan(&intent(), &[uplink], &tun).expect("plan");
    controller.backend.fail.store(true, Ordering::SeqCst);
    assert!(
        controller
            .apply(std::sync::Arc::new(command::SystemRunner), &intent())
            .await
            .is_err()
    );
    assert!(controller.backend.routes.lock().expect("routes").is_empty());
    controller
        .write(&super::Journal {
            routes: routes.clone(),
            policy: true,
        })
        .expect("write ahead");
    controller
        .backend
        .routes
        .lock()
        .expect("routes")
        .extend(routes);
    controller
        .cleanup(&command::SystemRunner)
        .await
        .expect("recover");
    assert!(controller.backend.routes.lock().expect("routes").is_empty());
    assert!(!path.exists());
}

#[tokio::test]
async fn failed_cleanup_keeps_journal_for_next_recovery() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("routes.json");
    let controller = Controller::<Memory>::new(Some(path.clone()));
    let (uplink, tun) = endpoints();
    let desired = plan(&intent(), &[uplink], &tun).expect("plan");
    controller
        .reconcile(&command::SystemRunner, desired, Some(false))
        .await
        .expect("install");
    controller
        .backend
        .fail_cleanup
        .store(true, Ordering::SeqCst);
    assert!(controller.cleanup(&command::SystemRunner).await.is_err());
    assert!(path.exists());
    controller
        .backend
        .fail_cleanup
        .store(false, Ordering::SeqCst);
    controller
        .cleanup(&command::SystemRunner)
        .await
        .expect("retry");
    assert!(!path.exists());
}

#[tokio::test]
async fn cancelled_apply_cannot_reinstall_routes_after_cleanup() {
    let directory = tempfile::tempdir().expect("directory");
    let controller = std::sync::Arc::new(Controller::<Memory>::new(Some(
        directory.path().join("routes.json"),
    )));
    controller.backend.paused.store(true, Ordering::SeqCst);
    let apply_controller = std::sync::Arc::clone(&controller);
    let caller = tokio::spawn(async move {
        apply_controller
            .apply(std::sync::Arc::new(command::SystemRunner), &intent())
            .await
    });
    controller.backend.entered.notified().await;
    caller.abort();
    let cleanup_controller = std::sync::Arc::clone(&controller);
    let cleanup =
        tokio::spawn(async move { cleanup_controller.cleanup(&command::SystemRunner).await });
    controller.backend.resume.notify_one();
    cleanup.await.expect("cleanup task").expect("cleanup");
    assert!(controller.backend.routes.lock().expect("routes").is_empty());
    assert!(!directory.path().join("routes.json").exists());
}
