#![cfg(target_os = "windows")]

use sempre_service::{ServiceError, State};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
const NAME: &str = "sempre";
const DISPLAY_NAME: &str = "Sempre";
const DESCRIPTION: &str = "fixture";
struct Output {
    success: bool,
    text: String,
}
static STOPPING: AtomicBool = AtomicBool::new(false);
static KILLED: AtomicBool = AtomicBool::new(false);
static COMMANDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
async fn command(program: &str, args: &[&str]) -> Result<Output, ServiceError> {
    COMMANDS
        .lock()
        .unwrap()
        .push(format!("{program} {}", args.join(" ")));
    if args.first() == Some(&"stop") {
        STOPPING.store(true, Ordering::Relaxed);
    }
    if program == "taskkill.exe" {
        KILLED.store(true, Ordering::Relaxed);
    }
    let state = if KILLED.load(Ordering::Relaxed) {
        "STOPPED"
    } else if STOPPING.load(Ordering::Relaxed) {
        "STOP_PENDING"
    } else {
        "RUNNING"
    };
    Ok(Output {
        success: true,
        text: format!("STATE : {state}\nPID : 1234"),
    })
}
async fn checked(program: &str, args: &[&str]) -> Result<(), ServiceError> {
    command(program, args).await.map(|_| ())
}
#[allow(dead_code)]
#[path = "../src/platform/windows.rs"]
mod platform;
#[tokio::test]
async fn stalled_old_service_is_forced_down_within_one_ten_second_budget() {
    let started = tokio::time::Instant::now();
    platform::stop().await.unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_secs(9));
    assert!(elapsed < Duration::from_secs(10));
    assert!(KILLED.load(Ordering::Relaxed));
    let commands = COMMANDS.lock().unwrap();
    assert!(commands.iter().any(|value| value == "sc.exe stop sempre"));
    assert!(
        commands
            .iter()
            .any(|value| value == "taskkill.exe /F /PID 1234")
    );
    assert!(!commands.iter().any(|value| value.contains("/T")));
    println!("stalled service terminated and confirmed after {elapsed:?}");
}
