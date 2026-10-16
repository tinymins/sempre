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
struct Memory {
    routes: Mutex<HashSet<Route>>,
    events: Mutex<Vec<String>>,
    fail: AtomicBool,
    fail_cleanup: AtomicBool,
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
    async fn policy(&self, _: &dyn command::Runner, add: bool) -> Result<(), TransparentError> {
        self.events
            .lock()
            .expect("events")
            .push(format!("policy:{add}"));
        Ok(())
    }
    async fn verify(
        &self,
        _: &dyn command::Runner,
        routes: &[Route],
        _: &[String],
    ) -> Result<(), TransparentError> {
        if routes
            .iter()
            .all(|r| self.routes.lock().expect("routes").contains(r))
        {
            Ok(())
        } else {
            Err(invalid("missing route"))
        }
    }
}

fn endpoints() -> (Endpoint, Endpoint) {
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

fn intent() -> Intent {
    Intent {
        direct: true,
        prefixes: vec!["223.5.5.0/24".into(), "114.114.114.0/24".into()],
        tun_addresses: vec!["172.19.0.1/30".into()],
        capture_prefixes: Vec::new(),
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
    ] {
        let address = address.parse::<std::net::IpAddr>().expect("IP");
        let route = routes
            .iter()
            .filter(|r| {
                r.prefix
                    .parse::<ipnet::IpNet>()
                    .expect("CIDR")
                    .contains(&address)
            })
            .max_by_key(|r| r.prefix.parse::<ipnet::IpNet>().expect("CIDR").prefix_len())
            .expect("route");
        assert_eq!(route.endpoint.interface, interface);
    }
    assert!(normalize(&[]).is_err());
    assert!(normalize(&["1.1.1.0/24".into()]).is_err());
    assert!(normalize(&["0.0.0.0/0".into()]).is_err());
    let v6 = Endpoint {
        interface: "en0".into(),
        index: 1,
        gateway: "fe80::1%en0".into(),
    };
    assert!(plan(&intent(), &[uplink, v6], &tun).is_err());
}

#[tokio::test]
async fn installation_updates_and_cleanup_preserve_order() {
    let directory = tempfile::tempdir().expect("directory");
    let controller = Controller::<Memory>::new(Some(directory.path().join("routes.json")));
    let (mut uplink, tun) = endpoints();
    let original = plan(&intent(), std::slice::from_ref(&uplink), &tun).expect("plan");
    controller
        .replace(&command::SystemRunner, original.clone(), &[])
        .await
        .expect("install");
    let events = controller.backend.events.lock().expect("events").clone();
    assert_eq!(
        &events[..4],
        &["true:false", "true:false", "true:true", "true:true"]
    );
    controller.backend.events.lock().expect("events").clear();
    let mut next = intent();
    next.prefixes.push("120.0.0.0/8".into());
    let updated = plan(&next, std::slice::from_ref(&uplink), &tun).expect("plan");
    controller
        .replace(&command::SystemRunner, updated, &[])
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
        .replace(&command::SystemRunner, changed.clone(), &[])
        .await
        .expect("switch");
    assert_eq!(
        *controller.backend.routes.lock().expect("routes"),
        changed.into_iter().collect()
    );
    next.direct = false;
    let proxy = plan(&next, &[endpoints().0], &tun).expect("proxy plan");
    assert!(proxy.iter().all(|route| route.fallback));
    controller
        .replace(&command::SystemRunner, proxy.clone(), &next.prefixes)
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
    let controller = Controller::<Memory>::new(Some(path.clone()));
    let (uplink, tun) = endpoints();
    let routes = plan(&intent(), &[uplink], &tun).expect("plan");
    controller.backend.fail.store(true, Ordering::SeqCst);
    assert!(
        controller
            .replace(&command::SystemRunner, routes.clone(), &[])
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

#[test]
fn ipv6_cn_and_private_capture_obey_longest_prefix() {
    let (v4, tun) = endpoints();
    let mut intent = intent();
    intent.prefixes.push("2400:3200::/32".into());
    intent.tun_addresses.push("fdfe:dcba:9876::1/126".into());
    intent.capture_prefixes.push("10.8.0.0/16".into());
    let v6 = Endpoint {
        gateway: "fe80::1%en0".into(),
        ..v4.clone()
    };
    let routes = plan(&intent, &[v4.clone(), v6.clone()], &tun).expect("dual stack");
    for (address, expected) in [
        ("2400:3200::1", "en0"),
        ("2606:4700:4700::1111", "utun9"),
        ("10.8.0.1", "utun9"),
    ] {
        let address = address.parse::<std::net::IpAddr>().expect("address");
        let selected = routes
            .iter()
            .filter_map(|r| {
                let net = r.prefix.parse::<ipnet::IpNet>().expect("prefix");
                net.contains(&address)
                    .then_some((net.prefix_len(), &r.endpoint.interface))
            })
            .max_by_key(|(length, _)| *length)
            .expect("route");
        assert_eq!(selected.1, expected);
    }
    intent.capture_prefixes.push("223.5.0.0/16".into());
    assert!(plan(&intent, &[v4, v6], &tun).is_err());
}

#[tokio::test]
async fn failed_cleanup_keeps_journal_for_next_recovery() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("routes.json");
    let controller = Controller::<Memory>::new(Some(path.clone()));
    let (uplink, tun) = endpoints();
    let desired = plan(&intent(), &[uplink], &tun).expect("plan");
    controller
        .replace(&command::SystemRunner, desired, &[])
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

#[tokio::test]
async fn interface_replacement_withdraws_fallback_before_core_rebind() {
    for (name, index) in [("en1", 3), ("en0", 3)] {
        let directory = tempfile::tempdir().expect("directory");
        let controller = std::sync::Arc::new(Controller::<Memory>::new(Some(
            directory.path().join("routes.json"),
        )));
        controller
            .apply(std::sync::Arc::new(command::SystemRunner), &intent())
            .await
            .expect("install");
        let (mut uplink, _) = endpoints();
        uplink.interface = name.into();
        uplink.index = index;
        *controller.backend.uplink.lock().expect("uplink") = Some(uplink);
        assert!(
            controller
                .apply(std::sync::Arc::new(command::SystemRunner), &intent())
                .await
                .is_err()
        );
        assert!(controller.backend.routes.lock().expect("routes").is_empty());
        assert!(!directory.path().join("routes.json").exists());
    }
}
