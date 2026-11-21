# Windows CI end-to-end check of the sign-in pipe (ADR 0021). It makes two
# local users, one in the "Analyte users" group, starts the engine and asks its
# pipe as each user through the probe, started with Start-Process -Credential.
# Run it only on a disposable CI machine: it creates and deletes local accounts.
# Passwords are random, made here, kept as SecureString and never printed.
param(
    [Parameter(Mandatory = $true)] [string] $Engine,
    [Parameter(Mandatory = $true)] [string] $Probe
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$group = 'Analyte users'
$member = 'analyte-e2e-member'
$other = 'analyte-e2e-other'
$root = 'C:\analyte-sign-in-e2e'
$work = Join-Path $env:RUNNER_TEMP 'analyte-sign-in-engine'
$Engine = (Resolve-Path $Engine).Path

function New-Password {
    # 24 random bytes plus one character of each class the password policy asks for.
    $bytes = New-Object byte[] 24
    $random = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $random.GetBytes($bytes)
    $random.Dispose()
    $secure = New-Object System.Security.SecureString
    foreach ($c in ([Convert]::ToBase64String($bytes) + 'aA1!').ToCharArray()) { $secure.AppendChar($c) }
    [Array]::Clear($bytes, 0, $bytes.Length)
    $secure.MakeReadOnly()
    return $secure
}

function Invoke-AsUser([string] $Name, [System.Security.SecureString] $Password, [string] $File, [string[]] $Arguments, [string] $Output) {
    $credential = New-Object System.Management.Automation.PSCredential($Name, $Password)
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -Credential $credential `
        -LoadUserProfile -WorkingDirectory $root -RedirectStandardOutput $Output `
        -RedirectStandardError "$Output.err" -NoNewWindow -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "$File as $Name exited with $($process.ExitCode): $(Get-Content -Raw "$Output.err")"
    }
}

function Assert-Answer([string] $File, [string] $Expected, [string] $Account) {
    $answer = Get-Content -Raw $File | ConvertFrom-Json
    $fields = @($answer.PSObject.Properties.Name) -join ','
    if ($Expected -eq 'code') {
        # The code itself is a secret; only its shape is checked and nothing is printed of it.
        if ($fields -ne 'code,account' -or $answer.code -notmatch '^[A-Za-z0-9+/]{43}=$') {
            throw "expected a code from $File, got the fields $fields"
        }
    } elseif ($fields -ne 'refused,account' -or $answer.refused -ne $Expected) {
        throw "expected '$Expected' from $File, got $(Get-Content -Raw $File)"
    }
    if ($answer.account -ne $Account) {
        throw "expected the account $Account from $File, got $($answer.account)"
    }
    Write-Output "$Account -> $Expected"
}

$engineProcess = $null
try {
    Remove-Item -Recurse -Force $root, $work -ErrorAction SilentlyContinue
    New-Item -ItemType Directory $root, $work | Out-Null
    # BUILTIN\Users may run the probe and write their answers here.
    icacls $root /grant '*S-1-5-32-545:(OI)(CI)M' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'icacls failed' }
    Copy-Item $Probe (Join-Path $root 'sign_in_probe.exe')
    $probeCopy = Join-Path $root 'sign_in_probe.exe'

    $memberPassword = New-Password
    $otherPassword = New-Password
    New-LocalUser -Name $member -Password $memberPassword -PasswordNeverExpires -AccountNeverExpires | Out-Null
    New-LocalUser -Name $other -Password $otherPassword -PasswordNeverExpires -AccountNeverExpires | Out-Null
    # The engine looks the group up once at start, so it exists before the engine runs.
    New-LocalGroup -Name $group | Out-Null
    Add-LocalGroupMember -Group $group -Member $member

    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    $pipe = "\\.\pipe\analyte-sign-in-$port"
    $config = Join-Path $work 'program.toml'
    Set-Content -Encoding Ascii -Path $config -Value @"
schema_version = 1
data_directory = 'data'

[admin]
port = $port

[store]
reserve_bytes = 1048576
database_bytes = 100000000
record_bytes = 2097152
page_rows = 64
busy_timeout_ms = 1000
"@
    # Rotation prints the new admin key; it goes nowhere.
    & $Engine --config $config admin token --rotate | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "admin token rotation exited with $LASTEXITCODE" }
    # Warnings say why sign-in is off or who is missing; the log holds no keys.
    $env:RUST_LOG = 'warn'
    $engineProcess = Start-Process -FilePath $Engine -ArgumentList 'run', $config -WorkingDirectory $work `
        -RedirectStandardOutput (Join-Path $work 'engine.out') -RedirectStandardError (Join-Path $work 'engine.err') `
        -NoNewWindow -PassThru
    $deadline = (Get-Date).AddSeconds(120)
    while (-not ([System.IO.Directory]::GetFiles('\\.\pipe\') -contains $pipe)) {
        if ($engineProcess.HasExited) { throw "the engine exited with $($engineProcess.ExitCode)" }
        if ((Get-Date) -gt $deadline) { throw "the engine did not open $pipe" }
        Start-Sleep -Milliseconds 200
    }

    $computer = $env:COMPUTERNAME
    Invoke-AsUser $member $memberPassword $probeCopy @($pipe) (Join-Path $root 'member.json')
    Assert-Answer (Join-Path $root 'member.json') 'code' "$computer\$member"
    Invoke-AsUser $other $otherPassword $probeCopy @($pipe) (Join-Path $root 'other.json')
    Assert-Answer (Join-Path $root 'other.json') 'not-allowed' "$computer\$other"
    # The runner's own account is an administrator: allowed enabled or deny-only.
    & $probeCopy $pipe > (Join-Path $root 'runner.json')
    if ($LASTEXITCODE -ne 0) { throw "the probe as the runner exited with $LASTEXITCODE" }
    Assert-Answer (Join-Path $root 'runner.json') 'code' ([System.Security.Principal.WindowsIdentity]::GetCurrent().Name)

    # Which logon this is: the pipe's access list admits interactive logons (S-1-5-4).
    Invoke-AsUser $member $memberPassword "$env:SystemRoot\System32\whoami.exe" @('/groups', '/fo', 'csv', '/nh') (Join-Path $root 'member-groups.csv')
    $sids = Import-Csv -Path (Join-Path $root 'member-groups.csv') -Header 'Name', 'Type', 'Sid', 'Attributes' | ForEach-Object { $_.Sid }
    foreach ($sid in @('S-1-5-4')) {
        if ($sids -notcontains $sid) { throw "the Start-Process -Credential token lacks $sid" }
    }
    Write-Output "Start-Process -Credential token: INTERACTIVE (S-1-5-4) present; REMOTE INTERACTIVE (S-1-5-14) present: $($sids -contains 'S-1-5-14')"
} catch {
    if (Test-Path (Join-Path $work 'engine.err')) {
        Write-Output '--- engine log ---'
        Get-Content (Join-Path $work 'engine.err') | Write-Output
    }
    throw
} finally {
    if ($null -ne $engineProcess -and -not $engineProcess.HasExited) { Stop-Process -Id $engineProcess.Id -Force }
    foreach ($user in @($member, $other)) { Remove-LocalUser -Name $user -ErrorAction SilentlyContinue }
    Remove-LocalGroup -Name $group -ErrorAction SilentlyContinue
}
