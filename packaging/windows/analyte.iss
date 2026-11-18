; Builds the click-through Windows installer (Inno Setup 6). The installer copies
; the files, then runs install.ps1, so manual and installer setups share one
; set of service and permission rules. Unsigned for now: see docs/release.md.
;
; Build from a staged release folder (analyte.exe, LICENSE, README.md, machines,
; packaging\windows):
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
UninstallDisplayIcon={app}\analyte.exe
UninstallDisplayName=Analyte
; The service is stopped in PrepareToInstall; nothing else should hold the files.
CloseApplications=no
; No setup log unless /LOG asks for one. The log holds the scripts' output
; (paths and errors), never the admin token.
SetupLogging=no

[Messages]
ConfirmUninstall=This removes the Analyte service and program files.%n%nData, configuration, backups, credentials and the admin token in C:\ProgramData\Analyte are kept. Delete them yourself only after the lab has decided what to do with the clinical data they hold.%n%nRemove Analyte now?

[Files]
Source: "{#SourceDir}\analyte.exe"; DestDir: "{app}"; Flags: ignoreversion
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

[Run]
Filename: "{code:SetupUrl}"; Description: "Open the Analyte setup page"; Flags: postinstall shellexec nowait skipifsilent; Check: CanOpenSetup

[Code]
var
  FirstInstall: Boolean;
  TokenExisted: Boolean;
  ServiceReady: Boolean;
  { Why install.ps1 failed; empty after a successful setup. }
  InstallError: String;
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
  Result := Exec(ExpandConstant('{cmd}'),
    '/c ""' + PowerShell + '" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'
      + Script + '" ' + Arguments + ' > "' + Output + '" 2>&1"',
    '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
  Log(ExtractFileName(Script) + ' exit code ' + IntToStr(Code) + '; output:' + #13#10
    + ReadOutput(Output));
end;

{ Stops a running service before its program file is replaced. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  Output: String;
begin
  Result := '';
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

procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
  Output: String;
begin
  if CurStep <> ssPostInstall then
    exit;
  Output := ExpandConstant('{tmp}\install-output.txt');
  ServiceReady := RunScript(ExpandConstant('{app}\packaging\windows\install.ps1'),
    '-Executable "' + ExpandConstant('{app}\analyte.exe') + '" -Config "' + ConfigFile
      + '" -DataDirectory "' + DataDirectory + '" -ConfigWritable -CreateAdminToken -Start',
    Output, Code);
  if ServiceReady then
    TokenPage.RichEditViewer.Lines.Text := TokenMessage
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

{ A failed service setup ends with exit code 10 instead of 0. }
function GetCustomSetupExitCode: Integer;
begin
  if InstallError <> '' then
    Result := 10
  else
    Result := 0;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpFinished) and (InstallError <> '') then
  begin
    WizardForm.FinishedHeadingLabel.Caption := 'Analyte was not set up';
    WizardForm.FinishedLabel.Caption := 'The program files were copied, but the Analyte '
      + 'service was not set up. The error was shown before this page. Fix the cause, then '
      + 'run this installer again, or uninstall Analyte.';
  end;
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

function CanOpenSetup: Boolean;
begin
  Result := ServiceReady and (AdminPort <> '');
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
    Output := ExpandConstant('{tmp}\uninstall-output.txt');
    if not RunScript(ExpandConstant('{app}\packaging\windows\uninstall.ps1'), '', Output, Code) then
      SuppressibleMsgBox('The Analyte service could not be removed:' + #13#10 + #13#10
        + ReadOutput(Output) + #13#10 + #13#10
        + 'Run packaging\windows\uninstall.ps1 as administrator after fixing the cause.',
        mbError, MB_OK, IDOK);
  end;
  if CurUninstallStep = usPostUninstall then
    SuppressibleMsgBox('Analyte was removed. Data, configuration, backups, credentials and the '
      + 'admin token in ' + ExpandConstant('{commonappdata}\Analyte') + ' were kept.',
      mbInformation, MB_OK, IDOK);
end;
