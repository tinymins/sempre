use std::io;

use windows_sys::Win32::{
    Globalization::{GetOEMCP, MB_ERR_INVALID_CHARS, MultiByteToWideChar},
    System::Console::GetConsoleOutputCP,
};

#[allow(unsafe_code)] // Windows code-page queries and conversion use bounded local buffers.
pub(super) fn decode(bytes: &[u8]) -> io::Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let byte_count = i32::try_from(bytes.len()).map_err(io::Error::other)?;
    // SAFETY: These queries take no pointers and do not modify the console code page.
    let code_page = unsafe {
        let console = GetConsoleOutputCP();
        if console == 0 { GetOEMCP() } else { console }
    };
    let conversion_error = || {
        io::Error::other(format!(
            "decode taskkill output using Windows code page {code_page}: {}",
            io::Error::last_os_error()
        ))
    };
    // SAFETY: The input slice is valid for byte_count bytes. A null destination with
    // zero capacity queries the required UTF-16 buffer length without writing.
    let wide_count = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            byte_count,
            std::ptr::null_mut(),
            0,
        )
    };
    if wide_count == 0 {
        return Err(conversion_error());
    }
    let mut wide = vec![0; usize::try_from(wide_count).map_err(io::Error::other)?];
    // SAFETY: Both slices remain alive, and the destination has the exact capacity
    // returned by the same conversion query for this input and code page.
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            byte_count,
            wide.as_mut_ptr(),
            wide_count,
        )
    };
    if written == 0 {
        return Err(conversion_error());
    }
    String::from_utf16(&wide[..usize::try_from(written).map_err(io::Error::other)?])
        .map_err(io::Error::other)
}
