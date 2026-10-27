# Integrated host: Ctrl+C stops owned processes; closing egui leaves servers running.
[CmdletBinding()]
param(
    [string]$DataDir = $(if ($env:URDWMS_DATA) { $env:URDWMS_DATA } elseif ($env:URDWMS_DATA_DIR) { $env:URDWMS_DATA_DIR } else { Join-Path $env:LOCALAPPDATA 'UrdWMS' }),
    [ValidateRange(1, 65535)]
    [int]$ApiPort = $(if ($env:URDWMS_API_PORT) { $env:URDWMS_API_PORT } else { 8071 }),
    [string]$WebAddress = $(if ($env:URDWMS_WEB_ADDRESS) { $env:URDWMS_WEB_ADDRESS } else { 'http://:8080' }),
    [string]$WebBind = $(if ($env:URDWMS_WEB_BIND) { $env:URDWMS_WEB_BIND } else { '0.0.0.0' }),
    [string]$Caddy,
    [ValidateRange(1, 3600)]
    [int]$StartupTimeoutSeconds = 600,
    [switch]$Demo,
    [switch]$WebOnly
)
$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 may not populate PSScriptRoot during parameter binding.
# Resolve bundled files only after entering the script body.
if ([string]::IsNullOrWhiteSpace($Caddy)) {
    $bundledCaddy = Join-Path $PSScriptRoot 'caddy.exe'
    $Caddy = if (Test-Path -LiteralPath $bundledCaddy) { $bundledCaddy } else { 'caddy.exe' }
}
$api = $null
$web = $null
$desktop = $null
$lock = $null
try {
    $server = Join-Path $PSScriptRoot 'urdwms-server.exe'
    $desktopExe = Join-Path $PSScriptRoot 'Urd-WMS.exe'
    if (!$WebOnly -and !(Test-Path $desktopExe)) { throw 'Urd-WMS.exe is missing; use -WebOnly to run servers only.' }
    $config = Join-Path $PSScriptRoot 'Caddyfile'
    $www = Join-Path $PSScriptRoot 'www'
    if (!(Test-Path $server) -or !(Test-Path (Join-Path $www 'index.html'))) {
        throw 'Incomplete web package: urdwms-server.exe and www/index.html are required.'
    }
    $caddyPath = (Get-Command $Caddy -CommandType Application -ErrorAction Stop).Source
    $DataDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($DataDir)
    if ($DataDir.Contains('"')) { throw 'DataDir cannot contain double quotes.' }
    New-Item -ItemType Directory -Force $DataDir | Out-Null
    # Prevent concurrent launchers from sharing/truncating the same logs.
    $lock = [IO.File]::Open((Join-Path $DataDir 'web-launcher.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    $env:URDWMS_API_PORT = "$ApiPort"
    $env:URDWMS_WEB_ADDRESS = $WebAddress
    $env:URDWMS_WEB_BIND = $WebBind
    $env:URDWMS_WEB_ROOT = $www.Replace('\', '/')
    & $caddyPath validate --config $config --adapter caddyfile
    if ($LASTEXITCODE -ne 0) { throw 'Caddy configuration validation failed.' }

    # Refuse an existing listener instead of accepting another instance's health.
    $probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $ApiPort)
    $probe.Server.ExclusiveAddressUse = $true
    try { $probe.Start() } finally { $probe.Stop() }

    $apiArgs = "--listen 127.0.0.1:$ApiPort --data-dir `"$($DataDir.Replace('\', '/'))`""
    if ($Demo) { $apiArgs += ' --demo' }
    $api = Start-Process -FilePath $server -ArgumentList $apiArgs -WorkingDirectory $PSScriptRoot -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $DataDir 'api.stdout.log') -RedirectStandardError (Join-Path $DataDir 'api.stderr.log')
    $ready = $false
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($api.HasExited) { throw "API exited ($($api.ExitCode)); see api.stderr.log in $DataDir" }
        try {
            $response = Invoke-WebRequest "http://127.0.0.1:$ApiPort/api/health" -UseBasicParsing -TimeoutSec 1
            if ($response.StatusCode -eq 200) { $ready = $true; break }
        } catch { }
        Start-Sleep -Milliseconds 500
    }
    if (!$ready -or $api.HasExited) { throw "API did not become healthy; see logs in $DataDir" }
    $web = Start-Process -FilePath $caddyPath -ArgumentList "run --config `"$config`" --adapter caddyfile" `
        -WorkingDirectory $PSScriptRoot -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $DataDir 'web.stdout.log') -RedirectStandardError (Join-Path $DataDir 'web.stderr.log')
    Start-Sleep -Milliseconds 500
    if ($web.HasExited -or $api.HasExited) { throw "A server exited during startup; see logs in $DataDir" }
    # API health is available only after DB migration/backup has completed.
    # Both processes receive the exact same absolute path, including spaces.
    if (!$WebOnly) {
        $desktop = Start-Process -FilePath $desktopExe -ArgumentList "--data-dir `"$($DataDir.Replace('\', '/'))`"" `
            -WorkingDirectory $PSScriptRoot -PassThru
    }
    Write-Host "Urd-WMS: $WebAddress | Data: $DataDir | Ctrl+C to stop"
    while (!$api.HasExited -and !$web.HasExited) { Start-Sleep -Milliseconds 500 }
    throw "A server process exited; see logs in $DataDir"
} finally {
    foreach ($child in @($desktop, $web, $api)) {
        if ($null -ne $child) {
            if (!$child.HasExited) {
                Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
                $child.WaitForExit()
            }
            $child.Dispose()
        }
    }
    if ($null -ne $lock) { $lock.Dispose() }
}
