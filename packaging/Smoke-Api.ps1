[CmdletBinding()]
param([string]$Server)
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Server)) {
    $Server = Join-Path $PSScriptRoot '../target/release/urdwms-server.exe'
}
$Server = (Resolve-Path $Server).Path
$data = Join-Path ([IO.Path]::GetTempPath()) ("urdwms smoke " + [guid]::NewGuid())
# Exercise Windows argument quoting with a data directory containing spaces.
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$process = $null
try {
    $process = Start-Process -FilePath $Server -ArgumentList "--listen 127.0.0.1:$port --data-dir `"$data`"" -PassThru -NoNewWindow
    $healthy = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ($process.HasExited) { throw "API exited: $($process.ExitCode)" }
        try {
            $response = Invoke-WebRequest "http://127.0.0.1:$port/api/health" -UseBasicParsing -TimeoutSec 1
            if ($response.StatusCode -eq 200) { $healthy = $true; break }
        } catch { }
        Start-Sleep -Milliseconds 500
    }
    if (!$healthy) { throw 'API health check timed out.' }
    if (!(Test-Path (Join-Path $data 'urdwms.db'))) { throw 'API did not create the database in the requested data directory.' }
    Write-Host 'Headless API health and first-run database smoke passed.'
} finally {
    if ($null -ne $process) {
        if (!$process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
        $process.Dispose()
    }
    if (Test-Path $data) { Remove-Item -Recurse -Force $data }
}
