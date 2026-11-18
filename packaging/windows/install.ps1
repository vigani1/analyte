#Requires -Version 5.1
# Registers the Analyte service and protects its folders. Run by hand or by the
# click-through installer (analyte.iss), which passes the three switches below.
[CmdletBinding()]
param(
    [string]$Executable = "$env:ProgramFiles\Analyte\analyte.exe",
    [string]$Config = "$env:ProgramData\Analyte\config\analyte.toml",
    [string]$DataDirectory = "$env:ProgramData\Analyte\data",
    [ValidateRange(30, 300)][int]$ShutdownTimeoutSecs = 30,
    # Let the service write the config folder, as the setup page needs.
    [switch]$ConfigWritable,
    # Create <data>\admin-token when it is missing; an existing token is kept.
    [switch]$CreateAdminToken,
    # Start the service at the end instead of leaving it stopped.
    [switch]$Start
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

# Refuses ACL boundaries that would keep old permissions or redirect inheritance.
# $Own names a child that Analyte itself locks (the credentials folder); it keeps
# its own protected ACL and is not entered.
function Assert-InheritedOnly([string]$Directory, [string]$What, [string]$Own = '') {
    if (-not (Test-Path -LiteralPath $Directory)) { return }
    $pending = New-Object 'Collections.Generic.Stack[string]'
    $pending.Push($Directory)
    while ($pending.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($Own -and $item.FullName.Equals($Own, [StringComparison]::OrdinalIgnoreCase)) {
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in $What directory: $($item.FullName)" }
                continue
            }
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in $What directory: $($item.FullName)" }
            $acl = Get-Acl -LiteralPath $item.FullName
            if ($acl.AreAccessRulesProtected -or @($acl.Access | Where-Object { -not $_.IsInherited }).Count -gt 0) {
                throw "Existing $What has explicit or protected permissions; review them before installation: $($item.FullName)"
            }
            if ($item.PSIsContainer) { $pending.Push($item.FullName) }
        }
    }
}

$administrators = New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
$account = 'NT SERVICE\Analyte'

# Replaces a folder's ACL: SYSTEM and Administrators FullControl, the service
# Modify, nothing inherited from the parent; children inherit this ACL.
function Set-ServiceFolderAcl([string]$Directory) {
    $null = [IO.Directory]::CreateDirectory($Directory)
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
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
    Set-Acl -LiteralPath $Directory -AclObject $acl
}

$Executable = (Resolve-Path -LiteralPath $Executable).ProviderPath
$Config = (Resolve-Path -LiteralPath $Config).ProviderPath
$ConfigDirectory = Split-Path -Path $Config -Parent
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
if ($ConfigWritable) {
    # The service may write only its own dedicated config folder.
    foreach ($protected in @($env:windir, $env:ProgramFiles, $env:ProgramData, [IO.Path]::GetPathRoot($ConfigDirectory))) {
        if ($ConfigDirectory.TrimEnd('\').Equals($protected.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'With -ConfigWritable the configuration must be in its own folder, not a system folder or drive root.'
        }
    }
    if ($ConfigDirectory.StartsWith($env:windir.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $ConfigDirectory.StartsWith($env:ProgramFiles.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'With -ConfigWritable the configuration cannot be inside a Windows or Program Files directory.'
    }
    Assert-InheritedOnly $ConfigDirectory 'configuration'
}
Assert-InheritedOnly $DataDirectory 'data' (Join-Path $DataDirectory 'credentials')
& $Executable check-config $Config
if ($LASTEXITCODE -ne 0) { throw "Configuration check failed with exit code $LASTEXITCODE; service not changed." }

& (Join-Path $PSScriptRoot 'stop.ps1')
$existing = Get-CimInstance -ClassName Win32_Service -Filter "Name='Analyte'"
$binaryPath = '"{0}" service run --config "{1}" --shutdown-timeout-secs {2}' -f $Executable, $Config, $ShutdownTimeoutSecs
# sc.exe, not WMI: Win32_Service.Create refuses a virtual account with code 22
# (invalid service account), because NT SERVICE\Analyte exists only once the
# service does (PR #255 run 44411503596). sc.exe passes no password, as the
# Service Control Manager requires for a virtual account. The command line is
# built here and passed as is, because Windows PowerShell 5.1 does not escape
# quotes inside native arguments; the paths were checked to hold no quotes.
$sc = Join-Path $env:windir 'System32\sc.exe'
$verb = if ($existing) { 'config' } else { 'create' }
$arguments = '{0} Analyte type= own start= auto error= normal binPath= "{1}" obj= "{2}" DisplayName= Analyte' -f $verb, $binaryPath.Replace('"', '\"'), $account
$process = Start-Process -FilePath $sc -ArgumentList $arguments -NoNewWindow -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Service configuration failed (sc.exe $verb exit code $($process.ExitCode))." }

# No permissions are granted on the executable tree by this script, and none on
# the configuration tree unless -ConfigWritable asks for it.
Set-ServiceFolderAcl $DataDirectory
if ($ConfigWritable) { Set-ServiceFolderAcl $ConfigDirectory }

& $sc failure Analyte reset= 86400 actions= restart/5000/restart/15000/restart/60000
if ($LASTEXITCODE -ne 0) { throw "Setting service recovery failed with exit code $LASTEXITCODE." }
& $sc failureflag Analyte 1
if ($LASTEXITCODE -ne 0) { throw "Setting service failure actions flag failed with exit code $LASTEXITCODE." }

if ($CreateAdminToken) {
    $token = Join-Path $DataDirectory 'admin-token'
    if (Test-Path -LiteralPath $token) {
        Write-Output "Existing admin token kept: $token"
    } else {
        # The token is printed once; discard it here. The installer shows it from
        # the protected file, which only SYSTEM, Administrators and the service can read.
        $null = & $Executable --config $Config admin token --rotate
        if ($LASTEXITCODE -ne 0) { throw "Creating the admin token failed with exit code $LASTEXITCODE." }
        # Analyte accepts only SYSTEM, Administrators or the service as owner.
        $tokenAcl = Get-Acl -LiteralPath $token
        $tokenAcl.SetOwner($administrators)
        Set-Acl -LiteralPath $token -AclObject $tokenAcl
        Write-Output "Admin token created: $token"
    }
}

if ($Start) {
    $service = Get-Service -Name Analyte
    $service.Start()
    $service.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(60))
    $service.Dispose()
    Write-Output 'Analyte installed and started.'
} else {
    Write-Output 'Analyte installed for automatic start. Review configuration, credentials and ACLs, then run Start-Service Analyte.'
}
