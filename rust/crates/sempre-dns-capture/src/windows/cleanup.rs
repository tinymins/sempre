use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use windivert::{
    error::{WinDivertError, WinDivertRecvError},
    prelude::{CloseAction, WinDivert, WinDivertFlags},
};
use windivert_sys::WinDivertShutdownMode;
use windows_service::{
    service::{ServiceAccess, ServiceState},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use crate::packet::Error;

pub fn run() -> Result<(), Error> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = match manager.open_service(
        "WinDivert",
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1060) => {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let executable = std::env::current_exe()?;
    let expected = executable
        .parent()
        .ok_or("capture executable has no directory")?
        .join("WinDivert64.sys");
    let installed = service.query_config()?.executable_path;
    let installed = installed.to_string_lossy();
    let installed = Path::new(installed.strip_prefix("\\??\\").unwrap_or(&installed));
    if !std::fs::canonicalize(installed)?
        .to_string_lossy()
        .eq_ignore_ascii_case(&std::fs::canonicalize(expected)?.to_string_lossy())
    {
        return Ok(()); // The shared driver belongs to another installation.
    }
    if service.query_status()?.current_state != ServiceState::Stopped {
        ensure_unused()?;
    }
    let service = manager.open_service(
        "WinDivert",
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    )?;
    if service.query_status()?.current_state != ServiceState::Stopped {
        service.stop()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.query_status()?.current_state != ServiceState::Stopped {
            if Instant::now() >= deadline {
                return Err("DNS capture driver did not stop".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    match service.delete() {
        Ok(()) => Ok(()),
        // WinDivert marks its service for deletion as part of its own installation.
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1072) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn ensure_unused() -> Result<(), Error> {
    let mut observer = WinDivert::reflect("true", 0, WinDivertFlags::new().set_no_installs())?;
    observer.shutdown(WinDivertShutdownMode::Recv)?;
    let mut buffer = vec![0; 65535];
    let result = match observer.recv(Some(&mut buffer)) {
        Ok(packet) => Err(format!(
            "WinDivert is still used by process {}; stop it before removing this installation",
            packet.address.process_id()
        )
        .into()),
        Err(WinDivertError::Recv(WinDivertRecvError::NoData)) => Ok(()),
        Err(error) => Err(error.into()),
    };
    observer.close(CloseAction::Nothing)?;
    result
}
