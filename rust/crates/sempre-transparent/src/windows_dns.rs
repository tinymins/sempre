use std::{fs, io, net::Ipv4Addr, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{TransparentError, command};

const POWERSHELL: &str = "powershell.exe";
pub(crate) const MANAGED_SERVER: &str = "192.0.2.1";

const APPLY_SCRIPT: &str = r"
[Console]::InputEncoding = [Text.UTF8Encoding]::new()
$ErrorActionPreference = 'Stop'
$state = [Console]::In.ReadToEnd() | ConvertFrom-Json
foreach ($interface in @($state.interfaces)) {
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.InterfaceGuid.ToString() -eq [string]$interface.guid } |
        Select-Object -First 1
    if (-not $adapter) { throw 'DNS interface disappeared before takeover: ' + $interface.guid }
    $name = 'name=' + $adapter.InterfaceIndex
    & netsh.exe interface ipv4 set dnsservers $name source=static address=192.0.2.1 register=primary validate=no | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to configure DNS interface: ' + $interface.guid }
}
";

const READ_SCRIPT: &str = r"
[Console]::InputEncoding = [Text.UTF8Encoding]::new()
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
$ErrorActionPreference = 'Stop'
$state = [Console]::In.ReadToEnd() | ConvertFrom-Json
$items = foreach ($interface in @($state.interfaces)) {
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.InterfaceGuid.ToString() -eq [string]$interface.guid } |
        Select-Object -First 1
    if (-not $adapter) {
        [pscustomobject]@{ guid = [string]$interface.guid; present = $false; servers = @() }
        continue
    }
    $dns = Get-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -AddressFamily IPv4
    [pscustomobject]@{
        guid = [string]$interface.guid
        present = $true
        servers = @($dns.ServerAddresses)
    }
}
ConvertTo-Json -InputObject @($items) -Compress
";

const RESTORE_SCRIPT: &str = r"
[Console]::InputEncoding = [Text.UTF8Encoding]::new()
$ErrorActionPreference = 'Stop'
$state = [Console]::In.ReadToEnd() | ConvertFrom-Json
foreach ($interface in @($state.interfaces)) {
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.InterfaceGuid.ToString() -eq [string]$interface.guid } |
        Select-Object -First 1
    if (-not $adapter) { continue }
    $dns = Get-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -AddressFamily IPv4
    $current = @($dns.ServerAddresses)
    if ($current.Count -ne 1 -or $current[0] -ne '192.0.2.1') { continue }
    $name = 'name=' + $adapter.InterfaceIndex
    if ([bool]$interface.automatic) {
        & netsh.exe interface ipv4 set dnsservers $name source=dhcp | Out-Null
    } else {
        $original = @($interface.original)
        if ($original.Count -eq 0) { continue }
        $address = 'address=' + $original[0]
        & netsh.exe interface ipv4 set dnsservers $name source=static $address register=primary validate=no | Out-Null
        for ($index = 1; $index -lt $original.Count; $index++) {
            $address = 'address=' + $original[$index]
            $position = 'index=' + ($index + 1)
            & netsh.exe interface ipv4 add dnsservers $name $address $position validate=no | Out-Null
        }
    }
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to restore DNS interface: ' + $interface.guid
    }
}
";

pub(crate) struct SystemDns {
    state_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct State {
    original_upstreams: Vec<String>,
    interfaces: Vec<InterfaceState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct InterfaceState {
    guid: String,
    original: Vec<String>,
    automatic: bool,
    default_route: bool,
}

#[derive(Deserialize)]
struct CurrentInterface {
    guid: String,
    present: bool,
    servers: Vec<String>,
}

impl SystemDns {
    pub(crate) fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    pub(crate) async fn discover_upstreams(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<Vec<String>, TransparentError> {
        let upstreams = if let Some(state) = self.read_state()? {
            state.original_upstreams
        } else {
            let interfaces = capture_interfaces(runner).await?;
            original_upstreams(&interfaces)
        };
        Ok(upstreams)
    }

    pub(crate) async fn apply(
        &self,
        runner: &dyn command::Runner,
        original_upstreams: &[String],
    ) -> Result<(), TransparentError> {
        let interfaces = takeover_interfaces(capture_interfaces(runner).await?);
        if interfaces.is_empty() {
            return Err(TransparentError::Invalid(
                "Windows has no connected DNS interfaces".into(),
            ));
        }
        let state = State {
            original_upstreams: original_upstreams.to_vec(),
            interfaces,
        };
        fs::create_dir_all(&self.state_dir)
            .map_err(|source| Self::io("create Windows DNS state directory", source))?;
        self.write_state(&state)?;
        if let Err(error) = run_script(runner, APPLY_SCRIPT, Some(&state)).await {
            let _ = run_script(runner, RESTORE_SCRIPT, Some(&state)).await;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn restore(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        if self.state_dir.as_os_str().is_empty() {
            return Ok(());
        }
        let Some(state) = self.read_state()? else {
            return Ok(());
        };
        run_script(runner, RESTORE_SCRIPT, Some(&state)).await?;
        match fs::remove_file(self.state_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Self::io("remove Windows DNS state", source)),
        }
    }

    pub(crate) async fn verify(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        let state = self.read_state()?.ok_or_else(|| {
            TransparentError::Invalid("Windows DNS takeover has no ownership state".into())
        })?;
        let output = run_script(runner, READ_SCRIPT, Some(&state)).await?;
        let current: Vec<CurrentInterface> =
            serde_json::from_str(&output.stdout).map_err(|error| {
                TransparentError::Invalid(format!("decode Windows DNS interface state: {error}"))
            })?;
        for interface in current {
            if !interface.present || interface.servers != [MANAGED_SERVER] {
                return Err(TransparentError::Invalid(format!(
                    "Windows DNS interface {} is not using the Sempre DNS frontend",
                    interface.guid
                )));
            }
        }
        Ok(())
    }

    fn write_state(&self, state: &State) -> Result<(), TransparentError> {
        let mut data = serde_json::to_vec_pretty(state).map_err(|error| {
            TransparentError::Invalid(format!("encode Windows DNS state: {error}"))
        })?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.state_path(), &data, 0o600)
            .map_err(|source| Self::io("write Windows DNS state", source))
    }

    fn read_state(&self) -> Result<Option<State>, TransparentError> {
        match fs::read(self.state_path()) {
            Ok(data) => serde_json::from_slice(&data).map(Some).map_err(|error| {
                TransparentError::Invalid(format!("decode Windows DNS state: {error}"))
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Self::io("read Windows DNS state", source)),
        }
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join("network-interfaces.json")
    }

    fn io(context: &str, source: io::Error) -> TransparentError {
        TransparentError::Io {
            context: context.into(),
            source,
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(clippy::unused_async)] // Shared controller interface; discovery uses native read-only APIs.
async fn capture_interfaces(
    _: &dyn command::Runner,
) -> Result<Vec<InterfaceState>, TransparentError> {
    sempre_network::windows::dns_interfaces()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .map(|interface| InterfaceState {
                    guid: interface.guid,
                    original: interface.original,
                    automatic: interface.automatic,
                    default_route: interface.default_route,
                })
                .collect()
        })
        .map_err(|error| {
            TransparentError::Invalid(format!("capture Windows DNS interfaces: {error}"))
        })
}

#[cfg(not(target_os = "windows"))]
#[allow(clippy::unused_async)] // Windows-only system operation.
async fn capture_interfaces(
    _: &dyn command::Runner,
) -> Result<Vec<InterfaceState>, TransparentError> {
    Err(TransparentError::Invalid(
        "Windows DNS discovery is unavailable on this platform".into(),
    ))
}

fn original_upstreams(interfaces: &[InterfaceState]) -> Vec<String> {
    let mut values = usable_upstreams(
        interfaces
            .iter()
            .filter(|interface| interface.default_route)
            .flat_map(|interface| &interface.original),
    );
    if values.is_empty() {
        values = usable_upstreams(interfaces.iter().flat_map(|interface| &interface.original));
    }
    values
}

fn takeover_interfaces(interfaces: Vec<InterfaceState>) -> Vec<InterfaceState> {
    if interfaces.iter().any(|interface| interface.default_route) {
        interfaces
            .into_iter()
            .filter(|interface| interface.default_route)
            .collect()
    } else {
        interfaces
    }
}

fn usable_upstreams<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let Ok(address) = value.parse::<Ipv4Addr>() else {
            continue;
        };
        if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
            continue;
        }
        if !result.contains(value) {
            result.push(value.clone());
        }
    }
    result
}

async fn run_script<T: Serialize>(
    runner: &dyn command::Runner,
    script: &str,
    input: Option<&T>,
) -> Result<command::Output, TransparentError> {
    let input = input
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| TransparentError::Invalid(format!("encode Windows DNS input: {error}")))?;
    command::require_success(
        POWERSHELL,
        runner
            .run(
                POWERSHELL,
                &["-NoProfile", "-NonInteractive", "-Command", script],
                input.as_deref(),
            )
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<()>>,
    }

    impl command::Runner for FakeRunner {
        fn run<'a>(
            &'a self,
            _: &'a str,
            _: &'a [&'a str],
            _: Option<&'a [u8]>,
        ) -> Pin<Box<dyn Future<Output = Result<command::Output, TransparentError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls.lock().expect("calls").push(());
                panic!("saved ownership must not invoke a command")
            })
        }
    }

    fn interface(default_route: bool, original: &[&str]) -> InterfaceState {
        InterfaceState {
            guid: "00000000-0000-0000-0000-000000000001".into(),
            original: original.iter().map(|value| (*value).into()).collect(),
            automatic: true,
            default_route,
        }
    }

    #[tokio::test]
    async fn saved_ownership_supplies_original_upstreams_without_recapture() {
        let root = tempfile::tempdir().expect("state directory");
        let dns = SystemDns::new(root.path().into());
        dns.write_state(&State {
            original_upstreams: vec!["223.6.6.6".into()],
            interfaces: vec![interface(true, &["223.6.6.6"])],
        })
        .expect("ownership state");
        let runner = FakeRunner::default();
        assert_eq!(
            dns.discover_upstreams(&runner).await.expect("upstreams"),
            ["223.6.6.6"]
        );
        assert!(runner.calls.lock().expect("calls").is_empty());
    }
}
