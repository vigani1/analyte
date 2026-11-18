#Requires -Version 5.1
# Stops the Analyte service and waits for its bounded drain. install.ps1,
# uninstall.ps1 and the installer (before it replaces the program) all use it.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$existing = Get-CimInstance -ClassName Win32_Service -Filter "Name='Analyte'"
if (-not $existing) { return }
if ($existing.StartName -ine 'NT SERVICE\Analyte') {
    throw 'An unrelated Analyte service exists (different account); refusing to change it.'
}
if ($existing.State -ne 'Stopped') {
    $service = Get-Service -Name Analyte
    $service.Stop()
    # The longest shutdown bound is 300 seconds; wait a little longer than that.
    $service.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(310))
    $service.Dispose()
}
