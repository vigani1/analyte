#Requires -Version 5.1
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run uninstall.ps1 from an elevated PowerShell window (Run as administrator).'
}
$existing = Get-CimInstance -ClassName Win32_Service -Filter "Name='Analyte'"
if (-not $existing) { Write-Output 'Analyte is already uninstalled; data was not changed.'; return }
& (Join-Path $PSScriptRoot 'stop.ps1')
$result = Invoke-CimMethod -InputObject $existing -MethodName Delete
if ($result.ReturnValue -ne 0 -and $result.ReturnValue -ne 16) {
    throw "Service removal failed (Win32_Service code $($result.ReturnValue))."
}
Write-Output 'Analyte service removed. Data, logs, configuration, credentials and their ACLs were retained.'
