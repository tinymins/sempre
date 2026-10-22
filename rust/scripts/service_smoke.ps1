# Install, drive, and uninstall the native Rust service on Windows.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
cargo build --manifest-path=rust/Cargo.toml `
  -p sempre-client -p sempre-testcore -p sempre-smoke-setup
New-Item -ItemType Directory -Path smoke
Copy-Item rust/target/debug/sempre.exe smoke/sempre.exe
cargo run --manifest-path=rust/Cargo.toml -p sempre-smoke-setup -- `
  --root "$PWD/smoke" --core "$PWD/rust/target/debug/sempre-testcore.exe"
./smoke/sempre.exe --portable service install --yes
./smoke/sempre.exe --portable service install --yes
if ((./smoke/sempre.exe service status | Out-String) -notmatch 'running') {
  throw 'Sempre service is not running'
}
function Wait-RuntimeState([string]$Desired, [string]$State) {
  $runtimeStatus = ''
  for ($attempt = 0; $attempt -lt 100; $attempt++) {
    try {
      $runtimeStatus = ./smoke/sempre.exe --system runtime status 2>$null | Out-String
      if ($runtimeStatus -match "Desired: $Desired" -and $runtimeStatus -match "State: $State") { return }
    } catch {}
    Start-Sleep -Milliseconds 200
  }
  throw "managed core did not reach ${Desired}/${State}: $runtimeStatus"
}
Wait-RuntimeState running running
$coreDeploy = ./smoke/sempre.exe --portable service deploy core | Out-String
if ($coreDeploy -notmatch 'core deployment completed') { throw "core deployment failed: $coreDeploy" }
Wait-RuntimeState running running
$dataDeploy = ./smoke/sempre.exe --portable service deploy data | Out-String
if ($dataDeploy -notmatch 'data deployment completed') { throw "data deployment failed: $dataDeploy" }
Wait-RuntimeState running running
./smoke/sempre.exe --system runtime stop | Out-Null
Wait-RuntimeState stopped stopped
./smoke/sempre.exe --portable service restart
Wait-RuntimeState stopped stopped
Set-Content -Path smoke/subscription-updated.txt -NoNewline -Value 'trojan://secret@example.com:443#smoke-updated'
$configResult = ./smoke/sempre.exe --system config import smoke/subscription-updated.txt | Out-String
if ($configResult -notmatch 'staged') { throw "stopped config import was not compiled: $configResult" }
Wait-RuntimeState stopped stopped
./smoke/sempre.exe --system runtime start | Out-Null
Wait-RuntimeState running running
./smoke/sempre.exe --system runtime restart | Out-Null
Wait-RuntimeState running running
./smoke/sempre.exe --portable service stop
./smoke/sempre.exe --portable service start
./smoke/sempre.exe --portable service restart
./rust/scripts/windows_uninstall_smoke.ps1 -PortableExecutable ./smoke/sempre.exe
