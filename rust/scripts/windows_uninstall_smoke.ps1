param([Parameter(Mandatory = $true)][string]$PortableExecutable)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$portable = (Resolve-Path -LiteralPath $PortableExecutable).Path
$root = Join-Path $env:ProgramFiles 'Sempre'
$homeDirectory = Join-Path $env:ProgramData 'Sempre'
$installed = Join-Path $root 'sempre.exe'
$service = Get-CimInstance Win32_Service -Filter "Name='sempre'"
if ($null -eq $service -or $service.PathName -ne ('"' + $installed + '" service-host')) {
    throw 'Expected the smoke installation to own the Sempre service.'
}
$logs = Join-Path $env:TEMP ('sempre-uninstall-smoke-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $logs | Out-Null

function Assert-Removed([bool]$Purge) {
    if (Test-Path -LiteralPath $root) { throw 'Installation directory remains after uninstall.' }
    if (Get-Service sempre -ErrorAction SilentlyContinue) { throw 'Sempre service remains registered.' }
    if ($Purge -and (Test-Path -LiteralPath $homeDirectory)) { throw 'Purged data directory remains.' }
    foreach ($directory in @('cores', 'ui', 'logs', 'run')) {
        if (Test-Path -LiteralPath (Join-Path $homeDirectory $directory)) {
            throw ('Runtime directory remains: ' + $directory)
        }
    }
}

function Invoke-InstalledUninstall([bool]$Purge) {
    $arguments = @('uninstall', '--yes')
    if ($Purge) { $arguments += '--purge' }
    $stdout = Join-Path $logs 'stdout.txt'
    $stderr = Join-Path $logs 'stderr.txt'
    # -Wait also waits for descendants, so verify the asynchronous cleanup result.
    $process = Start-Process -FilePath $installed -ArgumentList $arguments `
        -WorkingDirectory $root -NoNewWindow -Wait -PassThru `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $output = Get-Content -LiteralPath $stdout -Raw
    $errors = Get-Content -LiteralPath $stderr -Raw
    if ($process.ExitCode -ne 0 -or $errors) { throw ('Uninstall failed: ' + $output + $errors) }
    if ($output -notmatch 'removal is pending' -or $output -notmatch 'Installation directory removed\.') {
        throw ('Missing verified uninstall completion: ' + $output)
    }
    Assert-Removed $Purge
    Write-Output $output
}

try {
    $retained = @{}
    foreach ($name in @('web.json', 'subscriptions/catalog.json')) {
        $retained[$name] = (Get-FileHash -LiteralPath (Join-Path $homeDirectory $name)).Hash
    }
    Invoke-InstalledUninstall $false
    if (-not (Test-Path -LiteralPath (Join-Path $homeDirectory 'state.json'))) {
        throw 'Configuration state was lost during retained uninstall.'
    }
    foreach ($name in $retained.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $homeDirectory $name)).Hash -ne $retained[$name]) {
            throw ('Retained configuration changed: ' + $name)
        }
    }
    & $portable install --yes
    if ($LASTEXITCODE -ne 0) { throw 'Reinstall failed.' }
    if ((Get-Service sempre).Status -ne 'Running') { throw 'Reinstalled service is not running.' }
    foreach ($name in $retained.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $homeDirectory $name)).Hash -ne $retained[$name]) {
            throw ('Reinstall overwrote retained configuration: ' + $name)
        }
    }
    Invoke-InstalledUninstall $true
    # A fresh install after purge must also work, with no stale service or state.
    & $portable install --yes
    if ($LASTEXITCODE -ne 0) { throw 'Fresh install after purge failed.' }
    if ((Get-Service sempre).Status -ne 'Running') { throw 'Fresh service is not running.' }
    Invoke-InstalledUninstall $true
    & $portable uninstall --purge --yes
    if ($LASTEXITCODE -ne 0) { throw 'Repeated uninstall failed.' }
    Assert-Removed $true
    Write-Output 'PASS: retained uninstall, reinstall, purge, fresh install, and repeated uninstall.'
} finally {
    Remove-Item -LiteralPath $logs -Recurse -Force
}
