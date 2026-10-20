use crate::NetworkError;
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, NO_ERROR},
    System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
};

#[allow(unsafe_code)] // Registry reads use nul-terminated UTF-16 inputs and sized aligned buffers.
pub(super) fn automatic_dns(guid: &str) -> Result<bool, NetworkError> {
    let path =
        format!("SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\{{{guid}}}")
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
    let name = "NameServer"
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut bytes = 0;
    // SAFETY: nul-terminated names remain alive; null output asks for the byte length.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut bytes,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(true);
    }
    if code != NO_ERROR && code != ERROR_MORE_DATA {
        return Err(std::io::Error::other(format!(
            "RegGetValueW NameServer {guid}: Windows error {code}"
        ))
        .into());
    }
    let mut value = vec![0_u16; (bytes as usize).div_ceil(2)];
    // SAFETY: UTF-16 buffer has at least the reported byte size and valid alignment.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &raw mut bytes,
        )
    };
    super::status("RegGetValueW NameServer", code)?;
    Ok(String::from_utf16_lossy(&value)
        .trim_matches(char::from(0))
        .trim()
        .is_empty())
}
