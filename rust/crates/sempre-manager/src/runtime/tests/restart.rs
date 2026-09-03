use std::{sync::Arc, time::Duration};

use super::*;

#[tokio::test]
async fn restart_is_accepted_before_preparation_and_blocks_all_other_actions() {
    let (_root, manager) = fixture();
    let manager = Arc::new(manager);
    let task = manager.start_restart_task().unwrap();
    assert_eq!(task.state, "running");
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 0);
    assert!(!manager.runtime_status().unwrap().actions.restart.allowed);
    assert!(manager.start_restart_task().is_err());
    for action in [START, STOP, RESTART] {
        assert_eq!(
            manager
                .runtime_action(action)
                .await
                .unwrap_err()
                .runtime_action_code(),
            Some("RUNTIME_RESTART_IN_PROGRESS")
        );
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while !manager.restart_task().unwrap().config_available {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(manager.restart_tasks.running());
    assert!(
        manager
            .restart_task_config(&task.id)
            .unwrap()
            .content
            .contains("inbounds")
    );
    assert!(manager.restart_task_config("wrong-task").is_none());
    manager.restart_tasks.healthy();
    assert_eq!(manager.restart_task().unwrap().state, "succeeded");
}

#[tokio::test]
async fn transitioning_core_rejects_restart_even_with_uncompiled_changes() {
    let (_root, manager) = fixture();
    manager.runtime_action(START).await.unwrap();
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0].revision += 1;
            Ok(())
        })
        .unwrap();
    assert!(manager.runtime_status().unwrap().pending);
    assert_eq!(
        manager
            .runtime_action(RESTART)
            .await
            .unwrap_err()
            .runtime_action_code(),
        Some("RUNTIME_RESTART_IN_PROGRESS")
    );
}
