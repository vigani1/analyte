#Requires -Version 5.1
# Removes the Analyte service, its Windows Firewall rule and its system PATH
# entry; keeps data, configuration, credentials and the 'Analyte users' group.
[CmdletBinding()]
param(
    # The installed program; its folder is the PATH entry install.ps1 -AddToPath added.
    [string]$Executable = "$env:ProgramFiles\Analyte\analyte.exe"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run uninstall.ps1 from an elevated PowerShell window (Run as administrator).'
}
# The Windows Firewall rule install.ps1 added, by its fixed name; removed even
# when the service is already gone.
$firewallRule = 'Analyte-Analyzers-In'
if (Get-NetFirewallRule -Name $firewallRule -ErrorAction SilentlyContinue) {
    Remove-NetFirewallRule -Name $firewallRule -ErrorAction Stop
    Write-Output "Windows Firewall rule 'Analyte - analyzers on the lab network' removed."
}
# Only the entry naming the program's folder; nothing happens when it is absent.
& (Join-Path $PSScriptRoot 'path.ps1') -Action Remove -Folder (Split-Path -Path $Executable -Parent)
# ADR 0021 (Windows sign-in): the group install.ps1 made may list other people.
$groupKept = "The local group 'Analyte users' was kept, since it may list other people. Remove it in Computer Management, or with: net localgroup `"Analyte users`" /delete"
$existing = Get-CimInstance -ClassName Win32_Service -Filter "Name='Analyte'"
if (-not $existing) { Write-Output 'Analyte is already uninstalled; data was not changed.'; Write-Output $groupKept; return }
& (Join-Path $PSScriptRoot 'stop.ps1')
$result = Invoke-CimMethod -InputObject $existing -MethodName Delete
if ($result.ReturnValue -ne 0 -and $result.ReturnValue -ne 16) {
    throw "Service removal failed (Win32_Service code $($result.ReturnValue))."
}
Write-Output 'Analyte service removed. Data, logs, configuration, credentials and their ACLs were retained.'
Write-Output $groupKept
