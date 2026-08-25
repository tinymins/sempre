use std::path::Path;

use crate::ServiceError;

#[cfg(any(target_os = "linux", test))]
pub(crate) fn systemd(executable: &Path, working_directory: &Path) -> Result<String, ServiceError> {
    let executable = path(executable)?;
    let working_directory = path(working_directory)?;
    Ok(format!(
        "[Unit]\n\
Description={}\n\
After=network-online.target\n\
Wants=network-online.target\n\n\
[Service]\n\
Type=simple\n\
WorkingDirectory={}\n\
ExecStart={} --system daemon\n\
Restart=on-failure\n\
RestartSec=5\n\
TimeoutStopSec=20\n\
StateDirectory=sempre\n\
StateDirectoryMode=0700\n\
LogsDirectory=sempre\n\
LogsDirectoryMode=0700\n\
RuntimeDirectory=sempre\n\
RuntimeDirectoryMode=0700\n\n\
[Install]\n\
WantedBy=multi-user.target\n",
        crate::DESCRIPTION,
        systemd_quote(working_directory),
        systemd_quote(executable),
    ))
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn launchd(executable: &Path, working_directory: &Path) -> Result<String, ServiceError> {
    let executable = xml_escape(path(executable)?);
    let working_directory = xml_escape(path(working_directory)?);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key><string>io.github.tinymins.sempre</string>\n\
  <key>ServiceDescription</key><string>{}</string>\n\
  <key>ProgramArguments</key>\n\
  <array><string>{executable}</string><string>--system</string><string>daemon</string></array>\n\
  <key>WorkingDirectory</key><string>{working_directory}</string>\n\
  <key>RunAtLoad</key><true/>\n\
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n\
  <key>ProcessType</key><string>Background</string>\n\
  <key>ThrottleInterval</key><integer>5</integer>\n\
</dict>\n\
</plist>\n",
        crate::DESCRIPTION,
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn path(value: &Path) -> Result<&str, ServiceError> {
    if !value.is_absolute() {
        return Err(ServiceError::InvalidPath(value.display().to_string()));
    }
    let value = value
        .to_str()
        .ok_or_else(|| ServiceError::InvalidPath(value.display().to_string()))?;
    if value.contains(['\0', '\r', '\n']) {
        return Err(ServiceError::InvalidPath(value.into()));
    }
    Ok(value)
}

#[cfg(any(target_os = "linux", test))]
fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
