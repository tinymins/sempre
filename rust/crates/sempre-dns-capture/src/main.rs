#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
mod packet;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod windows;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn main() {
    if let Err(error) = windows::run() {
        eprintln!("DNS capture failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn main() {
    eprintln!("WinDivert DNS capture requires Windows x64");
    std::process::exit(1);
}
