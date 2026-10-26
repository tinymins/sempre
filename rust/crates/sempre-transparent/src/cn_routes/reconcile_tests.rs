use std::sync::Arc;

use super::{
    CnRouteProgress, Controller,
    model::{Intent, Route, plan},
    tests::{Memory, endpoints, intent},
};
use crate::command;

fn wide_intent() -> Intent {
    // Non-adjacent prefixes stay separate after aggregation and span several batches.
    let prefixes = (0..300)
        .map(|i| format!("{}.{}.0.0/24", 1 + i / 200, i % 200))
        .collect();
    Intent {
        prefixes,
        ..intent()
    }
}

fn removals(controller: &Controller<Memory>) -> usize {
    let events = controller.backend.events.lock().expect("events");
    events.iter().filter(|e| e.starts_with("false:")).count()
}

fn journal_len(controller: &Controller<Memory>) -> usize {
    controller.read().expect("journal").routes.len()
}

#[tokio::test]
async fn interrupted_cleanup_resumes_with_only_the_remaining_routes() {
    let directory = tempfile::tempdir().expect("directory");
    let controller = Arc::new(Controller::<Memory>::new(Some(
        directory.path().join("routes.json"),
    )));
    let runner: Arc<dyn command::Runner> = Arc::new(command::SystemRunner);
    controller
        .apply(Arc::clone(&runner), &wide_intent())
        .await
        .expect("install");
    let installed = journal_len(&controller);
    assert!(installed > 2 * 128);
    // Fallback batch and one direct batch complete before the interruption.
    *controller.backend.removal_batches.lock().expect("budget") = Some(2);
    assert!(controller.cleanup(runner.as_ref()).await.is_err());
    let first = removals(&controller);
    let remaining = journal_len(&controller);
    assert_eq!(remaining, installed - first);
    assert_eq!(
        remaining,
        controller.backend.routes.lock().expect("routes").len()
    );

    *controller.backend.removal_batches.lock().expect("budget") = None;
    controller.cleanup(runner.as_ref()).await.expect("resume");
    assert_eq!(removals(&controller) - first, remaining);
    assert!(controller.backend.routes.lock().expect("routes").is_empty());
    assert!(!directory.path().join("routes.json").exists());
}

#[tokio::test]
async fn restart_reconciles_against_installed_routes() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("routes.json");
    let runner: Arc<dyn command::Runner> = Arc::new(command::SystemRunner);
    let first = Arc::new(Controller::<Memory>::new(Some(path.clone())));
    first
        .apply(Arc::clone(&runner), &wide_intent())
        .await
        .expect("install");
    let installed = first.backend.routes.lock().expect("routes").clone();
    let total = installed.len();
    let mut progress = first.progress();
    assert_eq!(
        *progress.borrow_and_update(),
        CnRouteProgress { done: total, total }
    );

    // A service restart keeps the routes; the next apply changes nothing.
    let restarted = Arc::new(Controller::<Memory>::new(Some(path.clone())));
    restarted
        .backend
        .routes
        .lock()
        .expect("routes")
        .clone_from(&installed);
    let foreign = Route {
        prefix: "198.51.100.0/24".into(),
        endpoint: endpoints().0,
        fallback: false,
        scoped: false,
    };
    restarted
        .backend
        .routes
        .lock()
        .expect("routes")
        .insert(foreign.clone());
    restarted
        .apply(Arc::clone(&runner), &wide_intent())
        .await
        .expect("reconcile");
    assert!(
        restarted
            .backend
            .events
            .lock()
            .expect("events")
            .iter()
            .all(|event| event.starts_with("policy:"))
    );

    // A reboot clears the OS table but not the journal; every route is restored.
    let rebooted = Arc::new(Controller::<Memory>::new(Some(path.clone())));
    rebooted
        .apply(Arc::clone(&runner), &wide_intent())
        .await
        .expect("reinstall");
    assert_eq!(*rebooted.backend.routes.lock().expect("routes"), installed);

    // Cleanup removes only routes this service owns.
    restarted.cleanup(runner.as_ref()).await.expect("cleanup");
    assert_eq!(
        restarted
            .backend
            .routes
            .lock()
            .expect("routes")
            .iter()
            .collect::<Vec<_>>(),
        [&foreign]
    );
    let (uplink, tun) = endpoints();
    assert!(
        plan(&wide_intent(), &[uplink], &tun)
            .expect("plan")
            .iter()
            .all(|route| *route != foreign)
    );
}
