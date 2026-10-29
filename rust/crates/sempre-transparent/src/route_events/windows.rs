use std::{ffi::c_void, sync::Arc};

use tokio::sync::Notify;
use windows_sys::Win32::{
    Foundation::{HANDLE, NO_ERROR},
    NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE,
        MIB_UNICASTIPADDRESS_ROW, NotifyIpInterfaceChange, NotifyRouteChange2,
        NotifyUnicastIpAddressChange,
    },
    Networking::WinSock::AF_UNSPEC,
};

use crate::TransparentError;

pub(super) struct Watcher {
    // Callbacks borrow this signal until Drop has cancelled every registration.
    signal: Arc<Notify>,
    // Registration handles as addresses, so the watcher stays Send.
    handles: Vec<usize>,
}

impl Watcher {
    #[allow(unsafe_code)] // Registrations are cancelled in Drop before the context is released.
    pub(super) fn open() -> Result<Self, TransparentError> {
        let mut watcher = Self {
            signal: Arc::new(Notify::new()),
            handles: Vec::new(),
        };
        let context = Arc::as_ptr(&watcher.signal).cast::<c_void>();
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: context stays valid until Drop cancels this registration.
        let code =
            unsafe { NotifyRouteChange2(AF_UNSPEC, Some(route), context, false, &raw mut handle) };
        watcher.register("NotifyRouteChange2", code, handle)?;
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: as above.
        let code = unsafe {
            NotifyIpInterfaceChange(AF_UNSPEC, Some(interface), context, false, &raw mut handle)
        };
        watcher.register("NotifyIpInterfaceChange", code, handle)?;
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: as above.
        let code = unsafe {
            NotifyUnicastIpAddressChange(AF_UNSPEC, Some(address), context, false, &raw mut handle)
        };
        watcher.register("NotifyUnicastIpAddressChange", code, handle)?;
        Ok(watcher)
    }

    fn register(&mut self, api: &str, code: u32, handle: HANDLE) -> Result<(), TransparentError> {
        if code != NO_ERROR {
            return Err(TransparentError::Io {
                context: format!("{api} failed"),
                source: std::io::Error::from_raw_os_error(code.cast_signed()),
            });
        }
        self.handles.push(handle as usize);
        Ok(())
    }

    pub(super) async fn next(&mut self) -> Result<(), TransparentError> {
        // Notifications before this wait leave a single permit, coalescing bursts.
        self.signal.notified().await;
        Ok(())
    }
}

impl Drop for Watcher {
    #[allow(unsafe_code)] // Handles come from successful registrations above.
    fn drop(&mut self) {
        for handle in self.handles.drain(..) {
            // SAFETY: cancellation waits for running callbacks, so none outlives the signal.
            unsafe {
                CancelMibChangeNotify2(handle as HANDLE);
            }
        }
    }
}

#[allow(unsafe_code)] // The context is the watcher's live signal.
fn wake(context: *const c_void) {
    // SAFETY: registrations pass Arc::as_ptr of the signal and are cancelled before it drops.
    unsafe { &*context.cast::<Notify>() }.notify_one();
}

extern "system" fn route(
    context: *const c_void,
    _: *const MIB_IPFORWARD_ROW2,
    _: MIB_NOTIFICATION_TYPE,
) {
    wake(context);
}

extern "system" fn interface(
    context: *const c_void,
    _: *const MIB_IPINTERFACE_ROW,
    _: MIB_NOTIFICATION_TYPE,
) {
    wake(context);
}

extern "system" fn address(
    context: *const c_void,
    _: *const MIB_UNICASTIPADDRESS_ROW,
    _: MIB_NOTIFICATION_TYPE,
) {
    wake(context);
}
