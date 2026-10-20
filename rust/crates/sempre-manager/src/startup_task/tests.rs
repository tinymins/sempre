use super::*;

#[test]
fn preparation_and_supervisor_failures_release_the_task() {
    let tasks = StartupTasks::default();
    tasks.begin("restart", Vec::new()).unwrap();
    tasks.fail("validation failed\noriginal stderr");
    assert_eq!(tasks.snapshot().unwrap().state, "failed");
    tasks.begin("restart", Vec::new()).unwrap();
    tasks.prepared(CurrentConfig {
        hash: "hash".into(),
        content: "{}".into(),
    });
    tasks.begin_cycle();
    tasks.failure("startup", "exit status 1");
    assert_eq!(tasks.snapshot().unwrap().state, "failed");
}
