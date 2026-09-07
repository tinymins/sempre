param(
    [Parameter()][ValidateNotNullOrEmpty()][string]$Core,
    [Parameter()][ValidateNotNullOrEmpty()][string]$Subscription,
    [Parameter()][ValidateNotNullOrEmpty()][string]$UI,
    [Parameter()][ValidateNotNullOrEmpty()][string]$UISha256
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ManifestUrl = 'https://sempre.run/api/releases/latest.json'
$TemporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("sempre-install-" + [Guid]::NewGuid().ToString('N'))

function Save-RemoteFile {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Invoke-WebRequest -Uri $Uri -OutFile $Destination -UseBasicParsing
}

try {
    $MachineArchitecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    $Architecture = switch ($MachineArchitecture.ToUpperInvariant()) {
        'AMD64' { 'amd64' }
        'ARM64' { 'arm64' }
        default { throw "Unsupported Windows architecture: $MachineArchitecture" }
    }

    $Manifest = Invoke-RestMethod -Uri $ManifestUrl -MaximumRedirection 10 -UseBasicParsing
    if ($Manifest.schema -ne 1) {
        throw "Unsupported release manifest schema: $($Manifest.schema)"
    }
    if ($Manifest.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$') {
        throw "Invalid release version: $($Manifest.version)"
    }
    if ($Manifest.repository -notmatch '^https://') {
        throw 'Invalid release repository URL.'
    }
    $Version = $Manifest.version
    $Asset = "sempre-bundle-windows-$Architecture.zip"
    $Target = "windows-$Architecture"
    $ReleaseAsset = @($Manifest.assets | Where-Object { $_.target -eq $Target -and $_.name -eq $Asset })
    if ($ReleaseAsset.Count -ne 1) {
        throw "Release $Version does not contain exactly one $Asset asset."
    }
    $ReleaseAsset = $ReleaseAsset[0]
    if ($ReleaseAsset.url -notmatch '^https://' -or $ReleaseAsset.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or $ReleaseAsset.size -le 0) {
        throw "Release asset metadata for $Asset is invalid."
    }

    New-Item -ItemType Directory -Path $TemporaryDirectory | Out-Null
    $Archive = Join-Path $TemporaryDirectory $Asset
    Write-Host "Downloading Sempre $Version for windows/$Architecture..."
    Save-RemoteFile -Uri $ReleaseAsset.url -Destination $Archive

    $Expected = $ReleaseAsset.sha256.ToLowerInvariant()
    $Actual = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) {
        throw "SHA-256 verification failed for $Asset."
    }

    $BundleDirectory = Join-Path $TemporaryDirectory 'bundle'
    Expand-Archive -LiteralPath $Archive -DestinationPath $BundleDirectory -Force
    $Binary = Join-Path $BundleDirectory "sempre-windows-$Architecture\sempre.exe"
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
        throw 'Verified bundle does not contain the Sempre executable.'
    }

    Write-Host 'Installing Sempre system service...'
    $InstallArguments = [Collections.Generic.List[string]]::new()
    $InstallArguments.Add('install')
    if ($Core) {
        $InstallArguments.Add("--core=$Core")
    }
    if ($Subscription) {
        $SubscriptionFile = Join-Path $TemporaryDirectory 'subscription-url'
        [IO.File]::WriteAllText($SubscriptionFile, $Subscription, [Text.UTF8Encoding]::new($false))
        $InstallArguments.Add("--subscription-file=$SubscriptionFile")
    }
    if ($UI) {
        $InstallArguments.Add("--ui=$UI")
    }
    if ($UISha256) {
        $InstallArguments.Add("--ui-sha256=$UISha256")
    }
    & $Binary @InstallArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Sempre installer exited with code $LASTEXITCODE."
    }
    Write-Host "Sempre $Version installed successfully. Open a new terminal and run: sempre status"
} finally {
    if (Test-Path -LiteralPath $TemporaryDirectory) {
        Remove-Item -LiteralPath $TemporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
