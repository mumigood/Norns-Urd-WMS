# Build/CI only: bundle a pinned official Caddy archive, verifying release SHA512.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Destination)
$ErrorActionPreference = 'Stop'
$version = '2.11.7'
$archiveName = "caddy_${version}_windows_amd64.zip"
$base = "https://github.com/caddyserver/caddy/releases/download/v$version"
$temp = Join-Path ([IO.Path]::GetTempPath()) ("urdwms-caddy-" + [guid]::NewGuid())
try {
    New-Item -ItemType Directory $temp | Out-Null
    $zip = Join-Path $temp $archiveName
    $manifest = Join-Path $temp 'checksums.txt'
    Invoke-WebRequest "$base/$archiveName" -OutFile $zip -UseBasicParsing
    Invoke-WebRequest "$base/caddy_${version}_checksums.txt" -OutFile $manifest -UseBasicParsing
    $pattern = '^([0-9a-fA-F]{128})\s+\*?' + [regex]::Escape($archiveName) + '$'
    $line = Get-Content $manifest | Where-Object { $_ -match $pattern }
    if (@($line).Count -ne 1 -or $line -notmatch $pattern) { throw 'Missing/ambiguous Caddy SHA512 entry.' }
    $expected = $Matches[1]
    if ((Get-FileHash $zip -Algorithm SHA512).Hash -ne $expected) { throw 'Caddy checksum mismatch.' }
    $expanded = Join-Path $temp 'expanded'
    Expand-Archive $zip $expanded
    New-Item -ItemType Directory -Force $Destination | Out-Null
    Copy-Item (Join-Path $expanded 'caddy.exe') (Join-Path $Destination 'caddy.exe')
    Copy-Item (Join-Path $expanded 'LICENSE') (Join-Path $Destination 'LICENSE-Caddy.txt')
    @("Caddy $version Windows amd64", "$base/$archiveName", "SHA512 $expected") |
        Set-Content (Join-Path $Destination 'Caddy-version.txt') -Encoding ascii
} finally {
    if (Test-Path $temp) { Remove-Item $temp -Recurse -Force }
}
