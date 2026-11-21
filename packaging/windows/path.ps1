#Requires -Version 5.1
# Adds the Analyte program folder to the system PATH, or removes it, so a new
# PowerShell window finds "analyte" without its full path. install.ps1
# -AddToPath, uninstall.ps1 and the installer all use it, so there is one PATH
# rule. It edits only the machine PATH value in the registry, keeps its type
# (REG_EXPAND_SZ keeps %SystemRoot% and similar entries unexpanded), and never
# reorders, expands or drops another entry. Never use setx for this: it cuts
# the value at 1024 characters.
#
#   path.ps1 -Action Add -Folder 'C:\Program Files\Analyte'
#   path.ps1 -Action Remove -Folder 'C:\Program Files\Analyte' -WhatIf
#
# -WhatIf reads the PATH and says what would change without writing it.
# Dot-sourcing the file (". .\path.ps1") only defines the two text functions,
# for tests.
[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateSet('Add', 'Remove')][string]$Action,
    [string]$Folder
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# The same folder whatever the letter case or a trailing backslash.
function Test-SameFolder([string]$Entry, [string]$Folder) {
    $Entry.Trim().Trim('"').TrimEnd('\').Equals($Folder.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
}

# The PATH text with $Folder appended once; unchanged when an entry already
# names it. Handles an empty value and one with or without a trailing ";".
function Add-PathEntry([string]$Path, [string]$Folder) {
    foreach ($entry in $Path.Split(';')) {
        if (Test-SameFolder $entry $Folder) { return $Path }
    }
    if ($Path -eq '') { return $Folder }
    if ($Path.EndsWith(';')) { return $Path + $Folder }
    return $Path + ';' + $Folder
}

# The PATH text without the entries that name $Folder; every other entry,
# empty ones included, stays as written and in order.
function Remove-PathEntry([string]$Path, [string]$Folder) {
    $kept = @($Path.Split(';') | Where-Object { -not (Test-SameFolder $_ $Folder) })
    return $kept -join ';'
}

if ($MyInvocation.InvocationName -eq '.') { return }

if (-not $Action -or -not $Folder) { throw 'Pass -Action Add or -Action Remove, and -Folder.' }
if (-not [IO.Path]::IsPathRooted($Folder) -or $Folder.Contains(';') -or $Folder.Contains('"')) {
    throw "The folder must be an absolute path without ';' or quotes: $Folder"
}
# -WhatIf only reads, so it needs no elevation.
$write = -not $WhatIfPreference
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if ($write -and -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Changing the system PATH needs an elevated PowerShell window (Run as administrator).'
}

$environment = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
$key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($environment, $write)
if (-not $key) { throw "The registry key HKLM\$environment could not be opened." }
try {
    $names = $key.GetValueNames()
    $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
    $current = ''
    if ($names -contains 'Path') {
        $kind = $key.GetValueKind('Path')
        if ($kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString -and $kind -ne [Microsoft.Win32.RegistryValueKind]::String) {
            throw "The system PATH has the registry type $kind, not a string; it was not changed."
        }
        $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    }
    $updated = if ($Action -eq 'Add') { Add-PathEntry $current $Folder } else { Remove-PathEntry $current $Folder }
    if ($updated -ceq $current) {
        if ($Action -eq 'Add') { Write-Output "The system PATH already lists $Folder." }
        else { Write-Output "The system PATH does not list $Folder." }
        return
    }
    $what = if ($Action -eq 'Add') { "add $Folder" } else { "remove $Folder" }
    if (-not $PSCmdlet.ShouldProcess('the system PATH', $what)) { return }
    $key.SetValue('Path', $updated, $kind)
} finally {
    $key.Dispose()
}
if ($Action -eq 'Add') { Write-Output "Added $Folder to the system PATH; new PowerShell windows find analyte." }
else { Write-Output "Removed $Folder from the system PATH." }

# Tell running programs, Explorer above all, to reload the environment, so a
# window opened from the Start menu sees the change without signing out. The
# PATH is already written; when this message cannot be sent, say so and go on.
try {
    if (-not ('Analyte.Environment' -as [type])) {
        Add-Type -Namespace Analyte -Name Environment -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
    }
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, five seconds.
    $sent = [Analyte.Environment]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
    if ($sent -eq [IntPtr]::Zero) { Write-Output 'Windows was not told about the new PATH; sign out and in again if a new window does not find analyte.' }
} catch {
    Write-Output "Windows was not told about the new PATH ($($_.Exception.Message)); sign out and in again if a new window does not find analyte."
}
