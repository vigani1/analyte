#Requires -Version 5.1
# Stages a release folder from a built program and makes the release assets:
# Windows: the zip and the click-through installer (Inno Setup);
# Linux: the tar.gz, .deb and .rpm (nfpm).
# The release workflow and the packaging check on pull requests both run it, so
# the build tools are pinned and hash-checked here, in one place.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('windows', 'linux')][string]$Platform,
    [Parameter(Mandatory)][string]$Target,
    # The workspace version without "v", for example 0.1.0.
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory)][string]$Binary,
    # Windows only: the desktop app, analyte-app.exe (ADR 0019).
    [string]$App = '',
    [string]$OutputDirectory = 'dist',
    [string]$StageDirectory = 'stage',
    [string]$ToolDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'analyte-packaging-tools')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Build-time tools only; neither is a Rust dependency or ships inside a package.
$InnoSetup = @{
    Url    = 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_4/innosetup-6.7.4.exe'
    Sha256 = 'a549929319f9120716677c685535ca4671e4b937b5fdc87e2585f32d8eb30e8c'
}
$Nfpm = @{
    Url    = 'https://github.com/goreleaser/nfpm/releases/download/v2.48.0/nfpm_2.48.0_Linux_x86_64.tar.gz'
    Sha256 = '6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750'
}

# Downloads a pinned tool into its own empty folder and refuses a changed file.
function Get-Tool([hashtable]$Tool, [string]$Name) {
    $folder = Join-Path $ToolDirectory $Name
    Remove-Item -LiteralPath $folder -Recurse -Force -ErrorAction SilentlyContinue
    $null = New-Item -ItemType Directory -Path $folder
    $file = Join-Path $folder ([IO.Path]::GetFileName($Tool.Url))
    Invoke-WebRequest -UseBasicParsing -Uri $Tool.Url -OutFile $file
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant()
    if ($actual -ne $Tool.Sha256) { throw "$Name download has SHA-256 $actual, expected $($Tool.Sha256)" }
    return $file
}

$tag = "v$Version"
$name = "analyte-$tag-$Target"
$stage = Join-Path $StageDirectory $name
$null = New-Item -ItemType Directory -Path (Join-Path $stage 'packaging'), $OutputDirectory -Force
$output = (Resolve-Path -LiteralPath $OutputDirectory).ProviderPath
Copy-Item -LiteralPath $Binary -Destination $stage
Copy-Item -LiteralPath "packaging/$Platform" -Destination (Join-Path $stage 'packaging') -Recurse
Copy-Item -LiteralPath LICENSE, README.md -Destination $stage
# The admin setup page lists these definitions from beside the program file.
Copy-Item -LiteralPath machines -Destination (Join-Path $stage 'machines') -Recurse
$stage = (Resolve-Path -LiteralPath $stage).ProviderPath

if ($Platform -eq 'windows') {
    if (-not $App) { throw 'Pass -App with the built analyte-app.exe; the Windows packages include the desktop app.' }
    Copy-Item -LiteralPath $App -Destination (Join-Path $stage 'analyte-app.exe')
    # The installer's and the Apps list's icon; docs/design/README.md says how it is made.
    Copy-Item -LiteralPath crates/analyte/assets/icons/analyte.ico -Destination $stage
    Compress-Archive -Path $stage -DestinationPath (Join-Path $output "$name.zip")
    $installer = Get-Tool $InnoSetup 'inno-setup'
    $inno = Join-Path $ToolDirectory 'inno-setup\portable'
    $process = Start-Process -FilePath $installer -Wait -PassThru -ArgumentList @(
        '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/PORTABLE=1', "/DIR=$inno")
    if ($process.ExitCode -ne 0) { throw "Inno Setup installation failed with exit code $($process.ExitCode)" }
    & (Join-Path $inno 'ISCC.exe') /Q "/DAppVersion=$Version" "/DSourceDir=$stage" "/DOutputDir=$output" packaging/windows/analyte.iss
    if ($LASTEXITCODE -ne 0) { throw "Building the Windows installer failed with exit code $LASTEXITCODE" }
} else {
    & chmod 755 (Join-Path $stage 'analyte')
    if ($LASTEXITCODE -ne 0) { throw 'chmod failed' }
    & tar -czf (Join-Path $output "$name.tar.gz") -C (Split-Path -Parent $stage) $name
    if ($LASTEXITCODE -ne 0) { throw 'tar failed' }
    $archive = Get-Tool $Nfpm 'nfpm'
    & tar -xzf $archive -C (Split-Path -Parent $archive) nfpm
    if ($LASTEXITCODE -ne 0) { throw 'Extracting nfpm failed' }
    $nfpm = Join-Path (Split-Path -Parent $archive) 'nfpm'
    # nfpm reads its sources relative to the folder it runs in.
    Push-Location -LiteralPath $stage
    try {
        $env:ANALYTE_VERSION = $Version
        foreach ($packager in 'deb', 'rpm') {
            & $nfpm package --config packaging/linux/nfpm.yaml --packager $packager --target "$output/"
            if ($LASTEXITCODE -ne 0) { throw "Building the .$packager package failed with exit code $LASTEXITCODE" }
        }
    } finally {
        Pop-Location
    }
}
Get-ChildItem -LiteralPath $output | Format-Table Name, Length
