use std::{fs, io, net::Ipv4Addr, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{TransparentError, command};

const POWERSHELL: &str = "powershell.exe";
const MANAGED_SERVER: &str = "127.0.0.1";

const CAPTURE_SCRIPT: &str = r"
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
$ErrorActionPreference = 'Stop'
$connected = @(Get-NetIPInterface -AddressFamily IPv4 |
    Where-Object { $_.ConnectionState -eq 'Connected' } |
    Select-Object -ExpandProperty InterfaceIndex)
$defaults = @(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty InterfaceIndex)
$items = foreach ($dns in Get-DnsClientServerAddress -AddressFamily IPv4) {
    if ($connected -notcontains $dns.InterfaceIndex -or @($dns.ServerAddresses).Count -eq 0) { continue }
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.ifIndex -eq $dns.InterfaceIndex } |
        Select-Object -First 1
    if (-not $adapter) { continue }
    $guid = $adapter.InterfaceGuid.ToString()
    $key = 'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{' + $guid.Trim('{}') + '}'
    $nameServer = [string](Get-ItemProperty -LiteralPath $key -Name NameServer -ErrorAction SilentlyContinue).NameServer
    [pscustomobject]@{
        guid = $guid
        name = $dns.InterfaceAlias
        original = @($dns.ServerAddresses)
        automatic = [string]::IsNullOrWhiteSpace($nameServer)
        default_route = $defaults -contains $dns.InterfaceIndex
    }
}
ConvertTo-Json -InputObject @($items) -Compress
";

const APPLY_SCRIPT: &str = r"
$ErrorActionPreference = 'Stop'
$state = [Console]::In.ReadToEnd() | ConvertFrom-Json
foreach ($interface in @($state.interfaces)) {
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.InterfaceGuid.ToString() -eq [string]$interface.guid } |
        Select-Object -First 1
    if (-not $adapter) { throw 'DNS interface disappeared before takeover: ' + $interface.guid }
    Set-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -ServerAddresses @('127.0.0.1')
}
";

const READ_SCRIPT: &str = r"
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
$ErrorActionPreference = 'Stop'
$state = [Console]::In.ReadToEnd() | ConvertFrom-Json
foreach ($interface in @($state.interfaces)) {
    $adapter = Get-NetAdapter -IncludeHidden |
        Where-Object { $_.InterfaceGuid.ToString() -eq [string]$interface.guid } |
        Select-Object -First 1
    if (-not $adapter) { continue }
    $dns = Get-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -AddressFamily IPv4
    $current = @($dns.ServerAddresses)
    if ($current.Count -ne 1 -or $current[0] -ne '127.0.0.1') { continue }
    if ([bool]$interface.automatic) {
        Set-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -ResetServerAddresses
    } else {
        Set-DnsClientServerAddress -InterfaceIndex $adapter.InterfaceIndex -ServerAddresses @($interface.original)
    }
}
";

pub(crate) struct SystemDns {
    allowed: bool,
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
    name: String,
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
    pub(crate) fn new(allowed: bool, state_dir: PathBuf) -> Self {
        Self { allowed, state_dir }
    }

    pub(crate) const fn allowed(&self) -> bool {
        self.allowed
    }

    pub(crate) async fn discover_upstreams(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<Vec<String>, TransparentError> {
        self.require_allowed()?;
        let upstreams = if let Some(state) = self.read_state()? {
            state.original_upstreams
        } else {
            let interfaces = capture_interfaces(runner).await?;
            original_upstreams(&interfaces)
        };
        if upstreams.is_empty() {
            Err(TransparentError::Invalid(
                "Windows has no usable original DNS servers".into(),
            ))
        } else {
            Ok(upstreams)
        }
    }

    pub(crate) async fn apply(
        &self,
        runner: &dyn command::Runner,
        original_upstreams: &[String],
    ) -> Result<(), TransparentError> {
        self.require_allowed()?;
        let interfaces = capture_interfaces(runner).await?;
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
        self.verify(runner).await
    }

    pub(crate) async fn restore(
        &self,
        runner: &dyn command::Runner,
    ) -> Result<(), TransparentError> {
        if !self.allowed {
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

    fn require_allowed(&self) -> Result<(), TransparentError> {
        if self.allowed {
            Ok(())
        } else {
            Err(TransparentError::Invalid(
                "Windows DNS takeover requires system mode".into(),
            ))
        }
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

async fn capture_interfaces(
    runner: &dyn command::Runner,
) -> Result<Vec<InterfaceState>, TransparentError> {
    let output = run_script::<State>(runner, CAPTURE_SCRIPT, None).await?;
    serde_json::from_str(&output.stdout).map_err(|error| {
        TransparentError::Invalid(format!("decode Windows DNS interfaces: {error}"))
    })
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
    use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        outputs: Mutex<VecDeque<command::Output>>,
        calls: Mutex<Vec<Call>>,
    }

    struct Call {
        program: String,
        arguments: Vec<String>,
        input: Option<Vec<u8>>,
    }

    impl FakeRunner {
        fn with_outputs(outputs: impl IntoIterator<Item = command::Output>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().collect()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl command::Runner for FakeRunner {
        fn run<'a>(
            &'a self,
            program: &'a str,
            arguments: &'a [&'a str],
            input: Option<&'a [u8]>,
        ) -> Pin<Box<dyn Future<Output = Result<command::Output, TransparentError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls.lock().expect("calls").push(Call {
                    program: program.into(),
                    arguments: arguments.iter().map(|value| (*value).into()).collect(),
                    input: input.map(<[u8]>::to_vec),
                });
                Ok(self
                    .outputs
                    .lock()
                    .expect("outputs")
                    .pop_front()
                    .expect("fake command output"))
            })
        }
    }

    fn output(stdout: &str) -> command::Output {
        command::Output {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn interface(default_route: bool, original: &[&str]) -> InterfaceState {
        InterfaceState {
            guid: "00000000-0000-0000-0000-000000000001".into(),
            name: "Ethernet".into(),
            original: original.iter().map(|value| (*value).into()).collect(),
            automatic: true,
            default_route,
        }
    }

    #[test]
    fn prefers_default_route_dns_and_filters_loopback() {
        let interfaces = [
            interface(false, &["9.9.9.9"]),
            interface(true, &["127.0.0.1", "223.6.6.6", "223.6.6.6"]),
        ];
        assert_eq!(original_upstreams(&interfaces), ["223.6.6.6"]);
    }

    #[test]
    fn falls_back_to_connected_dns_without_a_default_route() {
        let interfaces = [interface(false, &["202.101.172.35"]), interface(false, &[])];
        assert_eq!(original_upstreams(&interfaces), ["202.101.172.35"]);
    }

    #[tokio::test]
    async fn saved_ownership_supplies_original_upstreams_without_recapture() {
        let root = tempfile::tempdir().expect("state directory");
        let dns = SystemDns::new(true, root.path().into());
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

    #[tokio::test]
    async fn apply_verifies_and_restore_removes_owned_state() {
        let root = tempfile::tempdir().expect("state directory");
        let dns = SystemDns::new(true, root.path().into());
        let guid = "00000000-0000-0000-0000-000000000001";
        let capture = format!(
            r#"[{{"guid":"{guid}","name":"Ethernet","original":["223.6.6.6"],"automatic":true,"default_route":true}}]"#
        );
        let current = format!(r#"[{{"guid":"{guid}","present":true,"servers":["127.0.0.1"]}}]"#);
        let runner =
            FakeRunner::with_outputs([output(&capture), output(""), output(&current), output("")]);
        dns.apply(&runner, &["223.6.6.6".into()])
            .await
            .expect("take over DNS");
        assert!(dns.state_path().exists());
        dns.restore(&runner).await.expect("restore DNS");
        assert!(!dns.state_path().exists());

        let calls = runner.calls.lock().expect("calls");
        assert_eq!(calls.len(), 4);
        assert!(calls.iter().all(|call| call.program == POWERSHELL));
        assert_eq!(
            calls[0].arguments.last().map(String::as_str),
            Some(CAPTURE_SCRIPT)
        );
        assert!(calls[0].input.is_none());
        assert_eq!(
            calls[1].arguments.last().map(String::as_str),
            Some(APPLY_SCRIPT)
        );
        assert!(calls[1].input.as_deref().is_some_and(|input| {
            std::str::from_utf8(input).is_ok_and(|input| input.contains("223.6.6.6"))
        }));
        assert_eq!(
            calls[2].arguments.last().map(String::as_str),
            Some(READ_SCRIPT)
        );
        assert_eq!(
            calls[3].arguments.last().map(String::as_str),
            Some(RESTORE_SCRIPT)
        );
    }
}
