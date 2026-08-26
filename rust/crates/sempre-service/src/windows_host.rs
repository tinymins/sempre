use std::{
    error::Error,
    ffi::OsString,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use tokio::sync::watch;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

use crate::NAME;

type Runner = fn(watch::Receiver<bool>) -> Result<(), Box<dyn Error>>;

static RUNNER: OnceLock<Runner> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub fn dispatch(runner: Runner) -> Result<(), Box<dyn Error>> {
    RUNNER
        .set(runner)
        .map_err(|_| "Windows service runner is already configured")?;
    service_dispatcher::start(NAME, ffi_service_main)?;
    Ok(())
}

fn service_main(_: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("ERROR: {error}");
    }
}

fn run_service() -> Result<(), Box<dyn Error>> {
    let (shutdown, receiver) = watch::channel(false);
    let status_slot = Arc::new(Mutex::new(None::<ServiceStatusHandle>));
    let handler_status = Arc::clone(&status_slot);
    let event_handler = move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            if let Some(handle) = handler_status.lock().expect("service status lock").as_ref() {
                let _ = handle.set_service_status(service_status(
                    ServiceState::StopPending,
                    ServiceControlAccept::empty(),
                    ServiceExitCode::Win32(0),
                    Duration::from_secs(20),
                ));
            }
            let _ = shutdown.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let status_handle = service_control_handler::register(NAME, event_handler)?;
    *status_slot.lock().expect("service status lock") = Some(status_handle);
    status_handle.set_service_status(service_status(
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        ServiceExitCode::Win32(0),
        Duration::ZERO,
    ))?;
    let result = RUNNER
        .get()
        .ok_or("Windows service runner is not configured")?(receiver);
    let exit_code = if result.is_ok() {
        ServiceExitCode::Win32(0)
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };
    status_handle.set_service_status(service_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit_code,
        Duration::ZERO,
    ))?;
    result
}

fn service_status(
    state: ServiceState,
    accepted: ServiceControlAccept,
    exit_code: ServiceExitCode,
    wait_hint: Duration,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepted,
        exit_code,
        checkpoint: 0,
        wait_hint,
        process_id: None,
    }
}
