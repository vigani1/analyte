#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Executable = "$env:ProgramFiles\Analyte\analyte.exe",
    [string]$Config = "$env:ProgramData\Analyte\config\analyte.toml",
    [string]$DataDirectory = "$env:ProgramData\Analyte\data",
    [ValidateRange(30, 300)][int]$ShutdownTimeoutSecs = 30
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run install.ps1 from an elevated PowerShell window (Run as administrator).'
}

function Assert-NoReparsePoint([string]$Path) {
    $current = $Path
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Reparse points are not allowed in installation paths: $current"
            }
        }
        $parent = Split-Path -Path $current -Parent
        if ($parent -eq $current) { break }
        $current = $parent
    }
}

$Executable = (Resolve-Path -LiteralPath $Executable).ProviderPath
$Config = (Resolve-Path -LiteralPath $Config).ProviderPath
if (-not [IO.Path]::IsPathRooted($DataDirectory)) { throw 'DataDirectory must be an absolute local path.' }
$DataDirectory = [IO.Path]::GetFullPath($DataDirectory).TrimEnd('\')
if ($DataDirectory -notmatch '^[A-Za-z]:\\.+' -or $DataDirectory -eq [IO.Path]::GetPathRoot($DataDirectory).TrimEnd('\')) {
    throw 'DataDirectory must be a dedicated local directory, never a drive root or network path.'
}
foreach ($path in @($Executable, $Config, $DataDirectory)) {
    if ($path.Contains('"') -or $path.Contains("`r") -or $path.Contains("`n")) { throw 'Installation paths cannot contain quotes or newlines.' }
    Assert-NoReparsePoint $path
}
foreach ($path in @($Executable, $Config, $env:windir, $env:ProgramFiles, $env:ProgramData)) {
    if ($path.Equals($DataDirectory, [StringComparison]::OrdinalIgnoreCase) -or
        $path.StartsWith($DataDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'DataDirectory cannot contain the executable, configuration or a Windows system directory.'
    }
}
foreach ($systemDirectory in @($env:windir, $env:ProgramFiles)) {
    if ($DataDirectory.StartsWith($systemDirectory.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'DataDirectory cannot be inside a Windows or Program Files directory.'
    }
}
if (Test-Path -LiteralPath $DataDirectory -PathType Leaf) { throw 'DataDirectory is a file.' }

# Refuse ACL boundaries that would keep old permissions or redirect inheritance.
if (Test-Path -LiteralPath $DataDirectory) {
    $pending = New-Object 'Collections.Generic.Stack[string]'
    $pending.Push($DataDirectory)
    while ($pending.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in data directory: $($item.FullName)" }
            $acl = Get-Acl -LiteralPath $item.FullName
            if ($acl.AreAccessRulesProtected -or @($acl.Access | Where-Object { -not $_.IsInherited }).Count -gt 0) {
                throw "Existing data has explicit or protected permissions; review them before installation: $($item.FullName)"
            }
            if ($item.PSIsContainer) { $pending.Push($item.FullName) }
        }
    }
}
& $Executable check-config $Config
if ($LASTEXITCODE -ne 0) { throw "Configuration check failed with exit code $LASTEXITCODE; service not changed." }

$account = 'NT SERVICE\Analyte'
$existing = Get-CimInstance -ClassName Win32_Service -Filter "Name='Analyte'"
if ($existing -and $existing.StartName -ine $account) {
    throw 'An unrelated Analyte service exists (different account); refusing to replace it.'
}
if ($existing -and $existing.State -ne 'Stopped') {
    $service = Get-Service -Name Analyte
    $service.Stop()
    $service.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(310))
    $service.Dispose()
}
$binaryPath = '"{0}" service run --config "{1}" --shutdown-timeout-secs {2}' -f $Executable, $Config, $ShutdownTimeoutSecs
if ($existing) {
    $result = Invoke-CimMethod -InputObject $existing -MethodName Change -Arguments @{
        PathName = $binaryPath; StartMode = 'Automatic'; StartName = $account; StartPassword = ''
    }
} else {
    $result = Invoke-CimMethod -ClassName Win32_Service -MethodName Create -Arguments @{
        Name = 'Analyte'; DisplayName = 'Analyte'; PathName = $binaryPath
        ServiceType = [byte]16; ErrorControl = [byte]1; StartMode = 'Automatic'
        DesktopInteract = $false; StartName = $account; StartPassword = ''
    }
}
if ($result.ReturnValue -ne 0) { throw "Service configuration failed (Win32_Service code $($result.ReturnValue))." }

# No permissions are granted on the executable/configuration trees by this script.
# Replace the root ACL, dropping inherited broad grants; children inherit this ACL.
$null = [IO.Directory]::CreateDirectory($DataDirectory)
$acl = New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true, $false)
$administrators = New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
$acl.SetOwner($administrators)
$inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
$propagate = [Security.AccessControl.PropagationFlags]::None
$allow = [Security.AccessControl.AccessControlType]::Allow
foreach ($sidText in @('S-1-5-18', 'S-1-5-32-544')) {
    $sid = New-Object Security.Principal.SecurityIdentifier($sidText)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', $inherit, $propagate, $allow)
    $acl.AddAccessRule($rule)
}
$identity = New-Object Security.Principal.NTAccount($account)
$rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'Modify', $inherit, $propagate, $allow)
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $DataDirectory -AclObject $acl

$sc = Join-Path $env:windir 'System32\sc.exe'
& $sc failure Analyte reset= 86400 actions= restart/5000/restart/15000/restart/60000
if ($LASTEXITCODE -ne 0) { throw "Setting service recovery failed with exit code $LASTEXITCODE." }
& $sc failureflag Analyte 1
if ($LASTEXITCODE -ne 0) { throw "Setting service failure actions flag failed with exit code $LASTEXITCODE." }
Write-Output 'Analyte installed for automatic start. Review configuration, credentials and ACLs, then run Start-Service Analyte.'
