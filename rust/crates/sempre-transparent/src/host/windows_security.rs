use crate::TransparentError;
use windows_sys::Win32::{
    Foundation::GetLastError,
    Security::{
        CheckTokenMembership, CreateWellKnownSid, SECURITY_MAX_SID_SIZE,
        WinBuiltinAdministratorsSid,
    },
};

#[allow(unsafe_code)] // SID storage is aligned and sized; null token checks the effective caller token.
pub(super) fn is_administrator() -> Result<bool, TransparentError> {
    let mut sid = [0_u32; SECURITY_MAX_SID_SIZE as usize / 4];
    let mut size = SECURITY_MAX_SID_SIZE;
    let mut member = 0;
    // SAFETY: the aligned SID buffer has the advertised capacity and stays live for both calls.
    if unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &raw mut size,
        )
    } == 0
    {
        return Err(failed("CreateWellKnownSid"));
    }
    // SAFETY: CreateWellKnownSid succeeded, and member is a valid BOOL output buffer.
    if unsafe {
        CheckTokenMembership(
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &raw mut member,
        )
    } == 0
    {
        return Err(failed("CheckTokenMembership"));
    }
    Ok(member != 0)
}

#[allow(unsafe_code)] // GetLastError reads thread-local error state immediately after the failed call.
fn failed(api: &str) -> TransparentError {
    // SAFETY: GetLastError has no preconditions and no pointer arguments.
    let code = unsafe { GetLastError() };
    TransparentError::Invalid(format!("{api}: Windows error {code}"))
}
