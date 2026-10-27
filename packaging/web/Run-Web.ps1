# Run in the foreground. Ctrl+C stops both child processes via finally.
[CmdletBinding()]
param(
    [string]$DataDir = $(if ($env:URDWMS_DATA_DIR) { $env:URDWMS_DATA_DIR } else { Join-Path $env:LOCALAPPDATA 'UrdWMS-Web' }),
    [ValidateRange(1, 65535)]
    [int]$ApiPort = $(if ($env:URDWMS_API_PORT) { $env:URDWMS_API_PORT } else { 8071 }),
    [string]$WebAddress = $(if ($env:URDWMS_WEB_ADDRESS) { $env:URDWMS_WEB_ADDRESS } else { 'http://localhost:8080' }),
    [string]$WebBind = $(if ($env:URDWMS_WEB_BIND) { $env:URDWMS_WEB_BIND } else { '127.0.0.1' }),
    [string]$Caddy = 'caddy.exe',
    [switch]$Demo
)
$ErrorActionPreference = 'Stop'
$api = $null
$web = $null
$lock = $null
try {
    $server = Join-Path $PSScriptRoot 'urdwms-server.exe'
    $config = Join-Path $PSScriptRoot 'Caddyfile'
    $www = Join-Path $PSScriptRoot 'www'
    if (!(Test-Path $server) -or !(Test-Path (Join-Path $www 'index.html'))) {
        throw 'Incomplete web package: urdwms-server.exe and www/index.html are required.'
    }
    $caddyPath = (Get-Command $Caddy -CommandType Application -ErrorAction Stop).Source
    $DataDir = [IO.Path]::GetFullPath($DataDir)
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

    $apiArgs = "--listen 127.0.0.1:$ApiPort --data-dir `"$($DataDir.Replace('\', '/'))`""
    if ($Demo) { $apiArgs += ' --demo' }
    $api = Start-Process -FilePath $server -ArgumentList $apiArgs -WorkingDirectory $PSScriptRoot -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $DataDir 'api.stdout.log') -RedirectStandardError (Join-Path $DataDir 'api.stderr.log')
    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
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
    Write-Host "Urd-WMS: $WebAddress | Data: $DataDir | Ctrl+C to stop"
    while (!$api.HasExited -and !$web.HasExited) { Start-Sleep -Milliseconds 500 }
    throw "A server process exited; see logs in $DataDir"
} finally {
    foreach ($child in @($web, $api)) {
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
