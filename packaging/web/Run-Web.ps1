# Compatibility entry point: all defaults and lifecycle logic live in Run-WMS.
[CmdletBinding()]
param(
    [string]$DataDir,
    [int]$ApiPort,
    [string]$WebAddress,
    [string]$WebBind,
    [string]$Caddy,
    [int]$StartupTimeoutSeconds,
    [switch]$Demo
)
& (Join-Path $PSScriptRoot 'Run-WMS.ps1') -WebOnly @PSBoundParameters
