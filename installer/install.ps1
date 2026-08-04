$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Repository = 'https://github.com/tinymins/sempre'
$TemporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("sempre-install-" + [Guid]::NewGuid().ToString('N'))

function Get-EffectiveUri {
    param([Parameter(Mandatory = $true)][string]$Uri)

    $Response = Invoke-WebRequest -Uri $Uri -MaximumRedirection 10 -UseBasicParsing
    $BaseResponse = $Response.BaseResponse
    $ResponseUriProperty = $BaseResponse.PSObject.Properties['ResponseUri']
    if ($ResponseUriProperty -and $ResponseUriProperty.Value) {
        return $ResponseUriProperty.Value.AbsoluteUri
    }
    $RequestMessageProperty = $BaseResponse.PSObject.Properties['RequestMessage']
    if ($RequestMessageProperty -and $RequestMessageProperty.Value.RequestUri) {
        return $RequestMessageProperty.Value.RequestUri.AbsoluteUri
    }
    throw 'Could not determine the final GitHub release URL.'
}

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

    $EffectiveUri = Get-EffectiveUri -Uri "$Repository/releases/latest"
    $Match = [regex]::Match($EffectiveUri, '/releases/tag/(?<tag>v[0-9][0-9A-Za-z._-]*)/?$')
    if (-not $Match.Success) {
        throw "Could not resolve a valid latest release tag from $EffectiveUri"
    }
    $Tag = $Match.Groups['tag'].Value
    $Asset = "sempre-bundle-windows-$Architecture.zip"
    $ReleaseBase = "$Repository/releases/download/$Tag"

    New-Item -ItemType Directory -Path $TemporaryDirectory | Out-Null
    $Archive = Join-Path $TemporaryDirectory $Asset
    $Checksums = Join-Path $TemporaryDirectory 'SHA256SUMS'
    Write-Host "Downloading Sempre $Tag for windows/$Architecture..."
    Save-RemoteFile -Uri "$ReleaseBase/SHA256SUMS" -Destination $Checksums
    Save-RemoteFile -Uri "$ReleaseBase/$Asset" -Destination $Archive

    $Pattern = '^([0-9a-fA-F]{64})\s+\*?' + [regex]::Escape($Asset) + '$'
    $ChecksumMatches = @(Get-Content -LiteralPath $Checksums | Where-Object { $_ -match $Pattern })
    if ($ChecksumMatches.Count -ne 1) {
        throw "Checksum for $Asset is missing or invalid."
    }
    [void]($ChecksumMatches[0] -match $Pattern)
    $Expected = $Matches[1].ToLowerInvariant()
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
    & $Binary install
    if ($LASTEXITCODE -ne 0) {
        throw "Sempre installer exited with code $LASTEXITCODE."
    }
    Write-Host "Sempre $Tag installed successfully. Open a new terminal and run: sempre status"
} finally {
    if (Test-Path -LiteralPath $TemporaryDirectory) {
        Remove-Item -LiteralPath $TemporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
