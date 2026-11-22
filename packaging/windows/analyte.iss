; Builds the click-through Windows installer (Inno Setup 6). The installer copies
; the files, then runs install.ps1, so manual and installer setups share one
; set of service, permission and firewall rules (install.ps1 adds the Windows
; Firewall rule for analyzers; uninstall.ps1 removes it). Unsigned for now: see
; docs/release.md.
;
; Two setup types (ADR 0019 and the owner's 2026-11-21 engine-only request):
; the engine with the desktop app, or the engine only, set up from PowerShell
; or a browser. Silent installs choose one with /TYPE=full or /TYPE=engine;
; /COMPONENTS has no effect, since there is no custom type.
;
; Build from a staged release folder (analyte.exe, analyte-app.exe, analyte.ico,
; LICENSE, README.md, machines, packaging\windows):
;   ISCC.exe /DAppVersion=0.1.0 /DSourceDir=<folder> /DOutputDir=<folder> analyte.iss

#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<staged release folder>
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{90257357-2491-4FBE-B9EE-1F3F778B1E2A}
AppName=Analyte
AppVersion={#AppVersion}
AppPublisher=Analyte contributors
AppPublisherURL=https://github.com/vigani1/analyte
; Fixed paths: they match install.ps1's defaults and the starter program.
DefaultDirName={autopf}\Analyte
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile={#SourceDir}\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=analyte-v{#AppVersion}-x86_64-windows-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; The Analyte icon (crates/analyte/analyte.ico, staged by
; packaging/build.ps1) for the installer itself and the Apps list.
SetupIconFile={#SourceDir}\analyte.ico
UninstallDisplayIcon={app}\analyte.ico
UninstallDisplayName=Analyte
; The service is stopped and the desktop app closed in PrepareToInstall;
; nothing else should hold the files.
CloseApplications=no
; No setup log unless /LOG asks for one. The log holds the scripts' output
; (paths and errors), never the admin token.
SetupLogging=no
; The PATH task changes the system PATH (path.ps1); new windows should see it.
ChangesEnvironment=yes

[Messages]
ConfirmUninstall=This removes the Analyte service, its Windows Firewall rule, its PATH entry, the Analyte app and the program files.%n%nData, configuration, backups, credentials and the admin token in C:\ProgramData\Analyte are kept. Delete them yourself only after the lab has decided what to do with the clinical data they hold.%n%nRemove Analyte now?

WizardSelectComponents=Choose what to install
SelectComponentsDesc=Install the Analyte app too, or only the engine?
SelectComponentsLabel2=The engine is the Analyte service that talks to the analyzers and the lab apps. The Analyte app shows what the engine is doing, in a window and a tray icon. Without the app, set Analyte up from PowerShell or in a browser.

[Types]
; The first is the default; an upgrade keeps the type chosen before
; (UsePreviousSetupType), and an installation from before the choice existed
; had the app, so it stays "full".
Name: "full"; Description: "Engine and Analyte app (recommended)"
Name: "engine"; Description: "Engine only (set up from PowerShell or a browser)"

[Components]
Name: "engine"; Description: "Analyte engine (the Windows service)"; Types: full engine; Flags: fixed
Name: "app"; Description: "Analyte app (window and tray icon)"; Types: full

[Tasks]
; ADR 0019: the desktop app starts in the tray when this Windows user signs in;
; the tray's "Start at sign-in" switches it later.
Name: "autostart"; Description: "Start Analyte when I sign in"; Components: app
; For both types: install.ps1 -AddToPath adds the program folder to the system
; PATH once; unticked, path.ps1 removes it; uninstall.ps1 removes it.
Name: "path"; Description: "Let me type ""analyte"" in PowerShell (add Analyte to the PATH)"

[Files]
Source: "{#SourceDir}\analyte.exe"; DestDir: "{app}"; Flags: ignoreversion
; The desktop app (ADR 0019): the engine's pages in a window, and a tray icon.
Source: "{#SourceDir}\analyte-app.exe"; DestDir: "{app}"; Flags: ignoreversion; Components: app
Source: "{#SourceDir}\analyte.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\packaging\windows\*"; DestDir: "{app}\packaging\windows"; Flags: ignoreversion recursesubdirs
; Shipped machine definitions, listed by the setup page beside the program file.
; Kept on uninstall because the configuration refers to them.
Source: "{#SourceDir}\machines\*"; DestDir: "{commonappdata}\Analyte\config\machines"; Flags: ignoreversion recursesubdirs uninsneveruninstall
; The starter program and site file, only on a first installation; an upgrade
; never replaces a lab's configuration.
Source: "{#SourceDir}\packaging\windows\starter\machine-1.site.toml"; DestDir: "{commonappdata}\Analyte\config"; Flags: onlyifdoesntexist uninsneveruninstall; Check: IsFirstInstall
Source: "{#SourceDir}\packaging\windows\starter\analyte.toml"; DestDir: "{commonappdata}\Analyte\config"; Flags: onlyifdoesntexist uninsneveruninstall; Check: IsFirstInstall

[Icons]
; The Start menu entry. Its AppUserModelID is the app's identifier
; (crates/app/tauri.conf.json), so notifications show as "Analyte".
Name: "{autoprograms}\Analyte"; Filename: "{app}\analyte-app.exe"; AppUserModelID: "io.github.vigani1.analyte.app"; Comment: "See what the Analyte engine is doing"; Components: app

[Registry]
; The same value the tray's "Start at sign-in" writes (crates/app/src/shell/autostart.rs):
; the quoted path and the tray-only argument, for the user running setup.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Analyte"; ValueData: """{app}\analyte-app.exe"" --tray"; Flags: uninsdeletevalue; Components: app; Tasks: autostart
; Switching to the engine only removes the sign-in entry, as uninstall does.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Analyte"; Flags: deletevalue; Components: not app

[InstallDelete]
; Switching from the full type to the engine only removes the app the way
; uninstall does: its program, settings, Start menu entry, and the installing
; user's app log and WebView2 data. PrepareToInstall closed the app first.
Type: files; Name: "{app}\analyte-app.exe"; Components: not app
Type: files; Name: "{app}\analyte-app.toml"; Components: not app
Type: files; Name: "{autoprograms}\Analyte.lnk"; Components: not app
Type: filesandordirs; Name: "{localappdata}\Analyte"; Components: not app
Type: filesandordirs; Name: "{localappdata}\io.github.vigani1.analyte.app"; Components: not app

[UninstallDelete]
; Written by setup from the program file's admin port.
Type: files; Name: "{app}\analyte-app.toml"
; The installing user's app log and WebView2 data (no patient data: the app
; never sees any, and the pages are never stored). Other users' stay.
Type: filesandordirs; Name: "{localappdata}\Analyte"
Type: filesandordirs; Name: "{localappdata}\io.github.vigani1.analyte.app"

[Run]
Filename: "{app}\analyte-app.exe"; Description: "Open Analyte"; Flags: postinstall nowait skipifsilent runasoriginaluser; Components: app; Check: CanOpenApp
; The engine only, or the app without the WebView2 Runtime (no window): the
; setup page opens in the browser.
Filename: "{code:SetupUrl}"; Description: "Open the Analyte setup page in the browser"; Flags: postinstall shellexec nowait skipifsilent; Check: CanOpenSetupInBrowser

[Code]
var
  FirstInstall: Boolean;
  TokenExisted: Boolean;
  ServiceReady: Boolean;
  { Why install.ps1 failed; empty after a successful setup. }
  InstallError: String;
  { Why path.ps1 could not remove the PATH entry the unticked task asked to remove. }
  PathError: String;
  TokenPage: TOutputMsgMemoWizardPage;

function ConfigFile: String;
begin
  Result := ExpandConstant('{commonappdata}\Analyte\config\analyte.toml');
end;

function DataDirectory: String;
begin
  Result := ExpandConstant('{commonappdata}\Analyte\data');
end;

function TokenFile: String;
begin
  Result := DataDirectory + '\admin-token';
end;

function PowerShell: String;
begin
  Result := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');
end;

function IsFirstInstall: Boolean;
begin
  Result := FirstInstall;
end;

function InitializeSetup: Boolean;
begin
  FirstInstall := not FileExists(ConfigFile);
  TokenExisted := FileExists(TokenFile);
  ServiceReady := False;
  InstallError := '';
  PathError := '';
  Result := True;
end;

procedure InitializeWizard;
begin
  TokenPage := CreateOutputMsgMemoPage(wpInstalling, 'Admin token',
    'You need this token to log in to the Analyte admin and setup pages.',
    'Keep the token like a password.', '');
end;

function ReadOutput(Output: String): String;
var
  Text: AnsiString;
begin
  if LoadStringFromFile(Output, Text) then
    Result := Trim(String(Text))
  else
    Result := '(no output)';
end;

{ Runs a PowerShell script hidden and returns True only when it exits 0. Its
  output goes to a file in this setup's temporary folder and, when setup runs
  with /LOG, into the setup log. The scripts never print the admin token. }
function RunScript(Script, Arguments, Output: String; var Code: Integer): Boolean;
begin
  Code := -1;
  { Windows PowerShell gets its own default module path. Setup inherits the
    caller's environment, and PowerShell 7 puts its module folders first in
    PSModulePath; Windows PowerShell then fails to load Get-Acl from them
    (PR #255 run 44410216186: "found in the module
    'Microsoft.PowerShell.Security', but the module could not be loaded"). }
  Result := Exec(ExpandConstant('{cmd}'),
    '/c "set "PSModulePath=' + ExpandConstant('{commonpf64}\WindowsPowerShell\Modules;'
      + '{sys}\WindowsPowerShell\v1.0\Modules') + '" && "' + PowerShell
      + '" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'
      + Script + '" ' + Arguments + ' > "' + Output + '" 2>&1"',
    '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
  Log(ExtractFileName(Script) + ' exit code ' + IntToStr(Code) + '; output:' + #13#10
    + ReadOutput(Output));
end;

{ Closes every running desktop app, which holds no data, so its file can be
  replaced or removed. Nothing to close is fine. }
procedure CloseDesktopApp;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM analyte-app.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Log('taskkill analyte-app.exe exit code ' + IntToStr(Code));
end;

{ Stops a running service before its program file is replaced. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  Output: String;
begin
  Result := '';
  CloseDesktopApp;
  ExtractTemporaryFile('stop.ps1');
  Output := ExpandConstant('{tmp}\stop-output.txt');
  if not RunScript(ExpandConstant('{tmp}\stop.ps1'), '', Output, Code) then
    Result := 'The Analyte service could not be stopped, so nothing was changed:' + #13#10 + ReadOutput(Output);
end;

function TokenMessage: String;
var
  Token: AnsiString;
begin
  if TokenExisted then
    Result := 'An admin token already existed and was kept, so you can log in as before.'
  else if LoadStringFromFile(TokenFile, Token) then
    Result := 'Your admin token (shown only now):' + #13#10 + #13#10 + Trim(String(Token)) + #13#10 + #13#10
      + 'Select it, copy it and keep it somewhere safe, like a password. You enter it on the '
      + 'login page; then choose Setup.'
  else
    Result := 'No admin token was created; see the message shown during installation.';
  Result := Result + #13#10 + #13#10
    + 'The token is stored in ' + TokenFile + ', which only administrators and the Analyte '
    + 'service can read. To see it again, open PowerShell as administrator and run:' + #13#10
    + '  Get-Content ''' + TokenFile + '''' + #13#10 + #13#10
    + 'Do not open that folder in File Explorer and accept its "You don''t currently have '
    + 'permission" prompt: that adds your account to the folder, and Analyte then refuses to start.';
end;

{ The admin port from the program file's [admin] table; empty when there is none. }
function AdminPort: String;
var
  Lines: TArrayOfString;
  I, Equals, Comment: Integer;
  InAdmin: Boolean;
  Line, Value: String;
begin
  Result := '';
  if not LoadStringsFromFile(ConfigFile, Lines) then
    exit;
  InAdmin := False;
  for I := 0 to GetArrayLength(Lines) - 1 do
  begin
    Line := Trim(Lines[I]);
    if (Length(Line) > 0) and (Line[1] = '[') then
      InAdmin := Line = '[admin]'
    else if InAdmin and (Pos('port', Line) = 1) then
    begin
      Equals := Pos('=', Line);
      if Equals = 0 then
        exit;
      Value := Copy(Line, Equals + 1, Length(Line));
      Comment := Pos('#', Value);
      if Comment > 0 then
        Value := Copy(Value, 1, Comment - 1);
      Value := Trim(Value);
      if StrToIntDef(Value, 0) > 0 then
        Result := Value;
      exit;
    end;
  end;
end;

{ The WebView2 Runtime the app's window needs: Windows 11 has it; Windows 10
  may not. Microsoft's documented check is a non-empty, non-zero "pv" version. }
function WebView2Present: Boolean;
var
  Version: String;
  Key: String;
begin
  Key := 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  Result := (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\' + Key, 'pv', Version)
      or RegQueryStringValue(HKLM, 'SOFTWARE\' + Key, 'pv', Version)
      or RegQueryStringValue(HKCU, 'Software\' + Key, 'pv', Version))
    and (Version <> '') and (Version <> '0.0.0.0');
end;

{ The app's settings beside it: the engine's admin port, which only
  administrators may read in the program file. Without an admin port the app
  uses the starter's 8080 and says when nothing answers there. }
procedure WriteAppSettings;
var
  Port: String;
begin
  Port := AdminPort;
  if Port = '' then
    exit;
  if not SaveStringToFile(ExpandConstant('{app}\analyte-app.toml'),
      '# Written by setup from the [admin] port in ' + ConfigFile + '.' + #13#10
      + 'admin_port = ' + Port + #13#10, False) then
    Log('analyte-app.toml could not be written');
end;

function CanOpenApp: Boolean;
begin
  Result := ServiceReady and WebView2Present;
end;

function CanOpenSetupInBrowser: Boolean;
begin
  Result := ServiceReady and (AdminPort <> '')
    and not (WizardIsComponentSelected('app') and WebView2Present);
end;

{ How the finish page tells someone without the app to run the program. }
function AnalyteCommand: String;
begin
  if WizardIsTaskSelected('path') then
    Result := 'analyte'
  else
    Result := '& ''' + ExpandConstant('{app}\analyte.exe') + '''';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
  Output, PathOutput, Switches: String;
begin
  if CurStep <> ssPostInstall then
    exit;
  Output := ExpandConstant('{tmp}\install-output.txt');
  Switches := '-ConfigWritable -CreateAdminToken -UsersMayStart -Start';
  if WizardIsTaskSelected('path') then
    Switches := Switches + ' -AddToPath';
  ServiceReady := RunScript(ExpandConstant('{app}\packaging\windows\install.ps1'),
    '-Executable "' + ExpandConstant('{app}\analyte.exe') + '" -Config "' + ConfigFile
      + '" -DataDirectory "' + DataDirectory + '" ' + Switches,
    Output, Code);
  { An upgrade with the PATH task unticked takes the entry out again. }
  if ServiceReady and not WizardIsTaskSelected('path') then
  begin
    PathOutput := ExpandConstant('{tmp}\path-output.txt');
    if not RunScript(ExpandConstant('{app}\packaging\windows\path.ps1'),
        '-Action Remove -Folder "' + ExpandConstant('{app}') + '"', PathOutput, Code) then
      PathError := 'Analyte is installed and running, but its folder could not be taken off '
        + 'the system PATH (path.ps1 exit code ' + IntToStr(Code) + '):' + #13#10 + #13#10
        + ReadOutput(PathOutput);
  end;
  if WizardIsComponentSelected('app') then
    WriteAppSettings;
  if ServiceReady then
  begin
    TokenPage.RichEditViewer.Lines.Text := TokenMessage;
    if PathError <> '' then
      SuppressibleMsgBox(PathError, mbError, MB_OK, IDOK);
  end
  else
  begin
    { Inno Setup cannot roll back once files are copied, and it ignores an
      exception or Abort raised here, so fail loudly instead: an error box, the
      script's output in the setup log, a failed finish page and a non-zero
      exit code (GetCustomSetupExitCode). }
    InstallError := 'The program files were copied, but the Analyte service was not set up '
      + '(install.ps1 exit code ' + IntToStr(Code) + '):' + #13#10 + #13#10
      + ReadOutput(Output) + #13#10 + #13#10
      + 'Fix the cause, then run this installer again, or uninstall Analyte.';
    TokenPage.RichEditViewer.Lines.Text := InstallError;
    SuppressibleMsgBox(InstallError, mbCriticalError, MB_OK, IDOK);
  end;
end;

{ A failed service setup, or a PATH entry left behind, ends with exit code 10
  instead of 0. }
function GetCustomSetupExitCode: Integer;
begin
  if (InstallError <> '') or (PathError <> '') then
    Result := 10
  else
    Result := 0;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpFinished) and (InstallError = '') and (PathError <> '') then
    WizardForm.FinishedLabel.Caption := 'Analyte is installed and running, but its folder is '
      + 'still on the system PATH. The error was shown before this page. Run this installer '
      + 'again, or remove the entry in System, Advanced system settings, Environment Variables.'
  else if (CurPageID = wpFinished) and (InstallError = '') and not WizardIsComponentSelected('app') then
  begin
    WizardForm.FinishedLabel.Caption := 'The Analyte engine is installed and running. Set it up '
      + 'from PowerShell (Run as administrator), for example:' + #13#10 + #13#10
      + '  ' + AnalyteCommand + ' doctor' + #13#10
      + '  ' + AnalyteCommand + ' machines list' + #13#10 + #13#10
      + 'or on the setup page in the browser. The guide "Install without the app" '
      + '(docs/operations/install.md at github.com/vigani1/analyte) lists every step.';
    { The longer text pushes the browser checkbox down instead of covering it. }
    WizardForm.AdjustLabelHeight(WizardForm.FinishedLabel);
    WizardForm.RunList.Top := WizardForm.FinishedLabel.Top + WizardForm.FinishedLabel.Height + ScaleY(12);
  end
  else if (CurPageID = wpFinished) and (InstallError = '') and not WebView2Present then
    WizardForm.FinishedLabel.Caption := 'The Analyte engine is installed and running. The '
      + 'Analyte app needs the Microsoft Edge WebView2 Runtime, which this PC does not have, '
      + 'so it opens the pages in the browser instead. Install the WebView2 Runtime '
      + '(Evergreen) from Microsoft, then open Analyte from the Start menu.';
  if (CurPageID = wpFinished) and (InstallError <> '') then
  begin
    WizardForm.FinishedHeadingLabel.Caption := 'Analyte was not set up';
    WizardForm.FinishedLabel.Caption := 'The program files were copied, but the Analyte '
      + 'service was not set up. The error was shown before this page. Fix the cause, then '
      + 'run this installer again, or uninstall Analyte.';
  end;
end;


function SetupUrl(Param: String): String;
begin
  Result := 'http://127.0.0.1:' + AdminPort + '/setup';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
  Output: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    CloseDesktopApp;
    Output := ExpandConstant('{tmp}\uninstall-output.txt');
    if not RunScript(ExpandConstant('{app}\packaging\windows\uninstall.ps1'),
        '-Executable "' + ExpandConstant('{app}\analyte.exe') + '"', Output, Code) then
      SuppressibleMsgBox('The Analyte service could not be removed:' + #13#10 + #13#10
        + ReadOutput(Output) + #13#10 + #13#10
        + 'Run packaging\windows\uninstall.ps1 as administrator after fixing the cause.',
        mbError, MB_OK, IDOK);
  end;
  if CurUninstallStep = usPostUninstall then
    SuppressibleMsgBox('Analyte was removed. Data, configuration, backups, credentials and the '
      + 'admin token in ' + ExpandConstant('{commonappdata}\Analyte') + ' were kept. The Windows '
      + 'group "Analyte users" was kept too, since it may list other people.',
      mbInformation, MB_OK, IDOK);
end;
