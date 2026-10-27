# Run with Windows PowerShell 5.1 in CI; also runs with PowerShell 7 on Linux.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
foreach ($relative in @('web/Run-WMS.ps1', 'Smoke-Api.ps1')) {
    $path = Join-Path $root $relative
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    foreach ($parameter in $ast.ParamBlock.Parameters) {
        if ($null -ne $parameter.DefaultValue -and $parameter.DefaultValue.Extent.Text -match '\$(PSScriptRoot|PSCommandPath)') {
            throw "$relative resolves script paths during parameter binding; resolve them after param()."
        }
    }
}
$temp = Join-Path ([IO.Path]::GetTempPath()) ('urdwms launcher defaults ' + [guid]::NewGuid())
$oldData = $env:URDWMS_DATA
$oldAlias = $env:URDWMS_DATA_DIR
$oldLocal = $env:LOCALAPPDATA
$oldWebAddress = $env:URDWMS_WEB_ADDRESS
$oldWebBind = $env:URDWMS_WEB_BIND
try {
    New-Item -ItemType Directory $temp | Out-Null
    $code = Get-Content (Join-Path $root 'web/Run-WMS.ps1') -Raw
    $boundary = $code.IndexOf('$api = $null')
    if ($boundary -lt 0) { throw 'Launcher initialization boundary missing.' }
    # Execute the real parameter/default-resolution code in a script file;
    # stop before process startup or database access.
    $fixture = Join-Path $temp 'defaults.ps1'
    $output = '[pscustomobject]@{ Caddy = $Caddy; DataDir = $DataDir; WebAddress = $WebAddress; WebBind = $WebBind }'
    [IO.File]::WriteAllText($fixture, $code.Substring(0, $boundary) + $output)
    $env:URDWMS_WEB_ADDRESS = $null
    $env:URDWMS_WEB_BIND = $null
    $env:URDWMS_DATA = $null
    $env:URDWMS_DATA_DIR = $null
    $env:LOCALAPPDATA = Join-Path $temp 'Local App Data'
    $result = & $fixture
    if ($result.WebAddress -ne 'http://:8080' -or $result.WebBind -ne '0.0.0.0') { throw 'Default web listener must accept LAN connections on 0.0.0.0:8080.' }
    if ($result.Caddy -ne 'caddy.exe') { throw 'Missing bundled Caddy must fall back to PATH.' }
    if ($result.DataDir -ne (Join-Path $env:LOCALAPPDATA 'UrdWMS')) { throw 'Original egui data directory changed.' }
    $bundled = Join-Path $temp 'caddy.exe'
    New-Item -ItemType File $bundled | Out-Null
    $result = & $fixture
    if ($result.Caddy -ne $bundled) { throw 'Bundled Caddy not resolved relative to script directory.' }
    $result = & $fixture -Caddy 'D:/custom server/caddy.exe' -DataDir 'D:/original data'
    if ($result.Caddy -ne 'D:/custom server/caddy.exe' -or $result.DataDir -ne 'D:/original data') {
        throw 'Explicit paths were overwritten.'
    }
    $env:URDWMS_DATA = Join-Path $temp 'existing data'
    $result = & $fixture
    if ($result.DataDir -ne $env:URDWMS_DATA) { throw 'URDWMS_DATA override changed.' }
    Write-Host 'Launcher defaults passed: no binding-time paths, bundled/PATH Caddy, spaces, explicit paths, existing data directory.'
} finally {
    $env:URDWMS_DATA = $oldData
    $env:URDWMS_DATA_DIR = $oldAlias
    $env:LOCALAPPDATA = $oldLocal
    $env:URDWMS_WEB_ADDRESS = $oldWebAddress
    $env:URDWMS_WEB_BIND = $oldWebBind
    if (Test-Path $temp) { Remove-Item -Recurse -Force $temp }
}
