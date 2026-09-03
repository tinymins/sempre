$ErrorActionPreference = 'Stop'
try {
    $parent = $null
    try {
        $parent = [Diagnostics.Process]::GetProcessById([int]$env:SEMPRE_UNINSTALL_PID)
    } catch [ArgumentException] {
        # The uninstaller may have exited before this helper started.
    }
    if ($null -ne $parent) {
        try {
            if (-not $parent.WaitForExit(30000)) {
                throw 'Timed out waiting for the uninstaller to exit.'
            }
        } finally {
            $parent.Dispose()
        }
    }
    $root = $env:SEMPRE_UNINSTALL_ROOT
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (Test-Path -LiteralPath $root) {
        try {
            Remove-Item -LiteralPath $root -Recurse -Force
        } catch {
            if ([DateTime]::UtcNow -ge $deadline) { throw }
            Start-Sleep -Milliseconds 200
        }
    }
    Write-Output 'Installation directory removed.'
} catch {
    [Console]::Error.WriteLine(('Sempre installation removal failed: ' + $_.Exception.Message))
    exit 1
}
