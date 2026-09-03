param([string]$Capture, [string]$FrontendExecutable, [string]$Upstream = "", [string[]]$Addresses = @('198.51.100.53', '127.0.0.2', '::1'))
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Capture) -or -not (Test-Path -LiteralPath $FrontendExecutable)) {
    throw 'Pass the capture executable and the sempre-dns capture_frontend example'
}
function Start-Child([string]$File, [string]$Arguments) {
    $child = New-Object Diagnostics.Process
    $child.StartInfo.FileName = $File; $child.StartInfo.Arguments = $Arguments
    $child.StartInfo.UseShellExecute = $false
    $child.StartInfo.RedirectStandardInput = $true
    $child.StartInfo.RedirectStandardOutput = $true
    $child.StartInfo.RedirectStandardError = $true
    [void]$child.Start()
    return $child
}
function Stop-Child($Child) {
    if (-not $Child) { return }
    $Child.StandardInput.Close()
    if (-not $Child.WaitForExit(5000)) { $Child.Kill(); $Child.WaitForExit() }
}
$frontend = $null; $captureProcess = $null; $failures = @()
try {
    $frontend = Start-Child $FrontendExecutable $Upstream
    $frontendError = $frontend.StandardError.ReadToEndAsync()
    $ready = $frontend.StandardOutput.ReadLineAsync()
    if (-not $ready.Wait(15000) -or $ready.Result -notmatch '^READY (\d+)$') { throw 'Fixture did not become ready' }
    $port = [int]$Matches[1]
    $frontendOutput = $frontend.StandardOutput.ReadToEndAsync()
    $captureProcess = Start-Child $Capture ("127.0.0.1:$port " + $frontend.Id)
    $captureError = $captureProcess.StandardError.ReadToEndAsync()
    $ready = $captureProcess.StandardOutput.ReadLineAsync()
    if (-not $ready.Wait(5000) -or $ready.Result -ne 'READY') { throw 'Capture did not become ready' }
    foreach ($address in $Addresses) {
        foreach ($protocol in @('UDP', 'TCP')) {
            $uniqueName = ([guid]::NewGuid().ToString('N')) + '.sempre.invalid'
            foreach ($name in @('www.baidu.com', $uniqueName)) {
                $query = New-Object 'Collections.Generic.List[byte]'
                $query.AddRange([byte[]](0x12,0x34,1,0,0,1,0,0,0,0,0,0))
                foreach ($label in $name.Split('.')) { $query.Add([byte]$label.Length); $query.AddRange([Text.Encoding]::ASCII.GetBytes($label)) }
                $query.AddRange([byte[]](0,0,1,0,1)); $q = $query.ToArray()
                $ip = [Net.IPAddress]::Parse($address); $socket = $null
                try {
                    if ($protocol -eq 'UDP') {
                        $socket = New-Object Net.Sockets.UdpClient($ip.AddressFamily)
                        $socket.Client.ReceiveTimeout = 7000; $socket.Connect($ip, 53)
                        [void]$socket.Send($q, $q.Length); $peer = New-Object Net.IPEndPoint($ip, 53)
                        $answer = $socket.Receive([ref]$peer)
                    } else {
                        $socket = New-Object Net.Sockets.TcpClient($ip.AddressFamily)
                        if (-not $socket.ConnectAsync($ip, 53).Wait(2500)) { throw 'Connect timeout' }
                        $stream = $socket.GetStream(); $stream.ReadTimeout = 7000
                        [byte[]]$framed = @(0,$q.Length) + $q; $stream.Write($framed,0,$framed.Length)
                        $hi = $stream.ReadByte(); $lo = $stream.ReadByte(); if ($hi -lt 0 -or $lo -lt 0) { throw 'Missing DNS frame' }
                        $length = $hi * 256 + $lo; $answer = New-Object byte[] $length; $offset = 0
                        while ($offset -lt $length) { $n = $stream.Read($answer,$offset,$length-$offset); if ($n -eq 0) { throw 'Early EOF' }; $offset += $n }
                    }
                    $result = [string]::Join('.', $answer[($answer.Length-4)..($answer.Length-1)])
                    if ($name -eq 'www.baidu.com') {
                        if (($answer[3] -band 15) -ne 0 -or ($answer[6] * 256 + $answer[7]) -eq 0) { throw 'DoT upstream returned no answers' }
                    } elseif ($result -ne '203.0.113.11') { throw "Unexpected answer: $result" }
                    if ($answer[0] -ne 0x12 -or $answer[1] -ne 0x34) { throw 'Mismatched transaction' }
                    Write-Output "PASS $protocol $address $name"
                } catch { $failures += "$protocol $address $($_.Exception.Message)" }
                finally { if ($socket) { $socket.Close() } }
            }
        }
    }
} finally {
    Stop-Child $captureProcess
    Stop-Child $frontend
    if ($captureProcess) {
        & $Capture --cleanup
        if ($LASTEXITCODE -ne 0) { $failures += 'DNS capture driver cleanup failed' }
    }
    if ($captureProcess) { Write-Output "CAPTURE_EXIT=$($captureProcess.ExitCode)"; Write-Output $captureError.Result }
    if ($frontend) {
        Write-Output $frontendOutput.Result; Write-Output $frontendError.Result
        if ($frontendOutput.Result -notmatch 'QUERY www.baidu.com. tls://') { $failures += 'No production DoT query observed' }
    }
}
if ($failures.Count) { throw ($failures -join "`n") }
