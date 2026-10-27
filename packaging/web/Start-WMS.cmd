@echo off
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-WMS.ps1" %*
if errorlevel 1 pause
