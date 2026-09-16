use super::*;

#[test]
fn task_serializes_restarts_until_health_or_failure_and_keeps_config_private() {
    let tasks = RestartTasks::default();
    let first = tasks.begin(Vec::new()).unwrap();
    assert_eq!(
        tasks.begin(Vec::new()).unwrap_err().runtime_action_code(),
        Some("RUNTIME_RESTART_IN_PROGRESS")
    );
    tasks.healthy(); // A previous process becoming healthy must not finish preparation.
    assert!(tasks.running());
    tasks.prepared(CurrentConfig {
        hash: "hash".into(),
        content: "secret configuration".into(),
    });
    let snapshot = tasks.snapshot().unwrap();
    assert!(snapshot.config_available);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("secret configuration")
    );
    tasks.healthy();
    assert!(!tasks.running());
    let finished = tasks.snapshot().unwrap();
    assert_eq!(finished.state, "succeeded");
    assert!(finished.finished_at.is_some());
    tasks.runtime_log("stdout", "must not alter a finished task");
    assert_eq!(tasks.snapshot().unwrap().logs.len(), finished.logs.len());
    assert_ne!(tasks.begin(Vec::new()).unwrap().id, first.id);
}

#[test]
fn startup_failure_finishes_without_waiting_for_a_restored_core() {
    let tasks = RestartTasks::default();
    tasks.begin(Vec::new()).unwrap();
    tasks.prepared(CurrentConfig {
        hash: "hash".into(),
        content: "{}".into(),
    });
    tasks.failure("startup", "exit status 1");
    let task = tasks.snapshot().unwrap();
    assert_eq!(task.state, "failed");
    assert!(
        task.logs
            .iter()
            .any(|entry| entry.message.contains("exit status 1"))
    );
    assert!(!task.logs.iter().any(|entry| entry.stage == "rollback"));
    assert!(!task.logs.iter().any(|entry| entry.stage == "succeeded"));
}

#[test]
fn preparation_and_supervisor_failures_release_the_task() {
    let tasks = RestartTasks::default();
    tasks.begin(Vec::new()).unwrap();
    tasks.fail("validation failed\noriginal stderr");
    assert_eq!(tasks.snapshot().unwrap().state, "failed");
    tasks.begin(Vec::new()).unwrap();
    tasks.prepared(CurrentConfig {
        hash: "hash".into(),
        content: "{}".into(),
    });
    tasks.failure("startup", "exit status 1");
    assert_eq!(tasks.snapshot().unwrap().state, "failed");
}

#[test]
fn output_is_bounded_and_sequences_survive_eviction() {
    let tasks = RestartTasks::default();
    tasks.begin(Vec::new()).unwrap();
    for _ in 0..MAX_LOG_ENTRIES + 3 {
        tasks.log("stdout", "line");
    }
    let task = tasks.snapshot().unwrap();
    assert_eq!(task.logs.len(), MAX_LOG_ENTRIES);
    assert_eq!(task.omitted_logs, 4);
    assert_eq!(task.logs.first().unwrap().sequence, 4);
    assert_eq!(
        task.logs.last().unwrap().sequence,
        (MAX_LOG_ENTRIES + 3) as u64
    );
}
