# Windows service installation

Evidence: documented. These PowerShell 5.1 scripts do not qualify a real analyzer.
Use an elevated Windows PowerShell window. Build a release without `test-hooks`;
there is no binary signing or automatic upgrade mechanism.

## Click-through installer

`analyte.iss` builds the release's Windows installer with Inno Setup 6
(`packaging/build.ps1` pins the version). **It is unsigned for now: Windows
SmartScreen will warn; choose More info → Run anyway** after checking its
checksum. The lab manager's steps are in
[Install](../../docs/operations/install.md#windows-installer-recommended). In order, it:

1. stops a running Analyte service with `stop.ps1`, so the program can be replaced;
2. closes a running desktop app, then installs `analyte.exe`, the desktop app
   `analyte-app.exe` (ADR 0019) and this folder in `C:\Program Files\Analyte`,
   the `machines` definitions in `C:\ProgramData\Analyte\config\machines`, a
   Start menu entry "Analyte" and, if ticked, the installing user's sign-in
   entry (`HKCU\...\Run\Analyte`);
3. on a first installation only, writes the starter program and site file from
   `starter\` into `C:\ProgramData\Analyte\config` (an upgrade never replaces them);
4. runs `install.ps1 -ConfigWritable -CreateAdminToken -UsersMayStart -Start`,
   which checks the config, registers the service, protects the data and config
   folders, lets signed-in users start (not stop or change) the service, creates
   the admin token if missing, creates the local group `Analyte users` if
   missing and adds the person who ran setup (below), and starts the service;
   then writes `analyte-app.toml` with the program's admin port beside the app;
5. shows a new admin token once, read from the protected file (it is never
   logged), and offers to open the Analyte app. Without the WebView2 Runtime it
   says so and offers `http://127.0.0.1:<admin port>/setup` in the browser.

Uninstall closes the app, runs `uninstall.ps1`, removes the program files, the
app's settings, the Start menu entry and the installing user's sign-in entry, and keeps data,
configuration, machine definitions, backups, credentials, the token and the
`Analyte users` group, which may list other people, and says so. To build
the installer by hand from a staged release folder:
`ISCC.exe /DAppVersion=<version> /DSourceDir=<folder> /DOutputDir=<folder> analyte.iss`.

## Scripts

1. Place `analyte.exe` in `C:\Program Files\Analyte`. Administrators must own
   the binary and configuration trees. The service requires Read/Execute on
   binaries and Read on configuration, never Modify on those trees. Audit their
   existing ACLs, including parent directories; do not grant write access to
   ordinary users. The scripts deliberately grant no permissions on those trees.
2. Prepare the site's validated program, definitions and site files in
   `C:\ProgramData\Analyte\config`. Set the program's `data_directory` to
   `C:\ProgramData\Analyte\data` using TOML literal strings or escaped backslashes.
   Paths in the program file are relative to that file. Configure file handoff,
   archive and quarantine paths beneath the data directory. Do not deploy the
   synthetic examples as site settings.
3. Run from the repository root:

   ```powershell
   .\packaging\windows\install.ps1
   ```

   If unsigned scripts are blocked, review the scripts and follow the site's
   execution policy. Where permitted, use a process-only override, for example
   `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\packaging\windows\install.ps1`;
   no persistent policy change is required. The same applies to uninstall.

   Optional parameters: `-Executable`, `-Config`, `-DataDirectory`, and
   `-ShutdownTimeoutSecs` (30 through 300, default 30). Switches:
   `-ConfigWritable` gives the config folder the data folder's protected ACL
   (the service gets Modify, for the setup page); `-UsersMayStart` adds one
   entry to the service's access list, `(A;;RP;;;IU)`: interactive users may
   start it, for the app's Start engine button; `-CreateAdminToken` creates
   `<data>\admin-token` only when it is missing and does not print it;
   `-Start` starts the service at the end. The data directory must
   match the TOML setting; the script runs `check-config` but does not parse TOML
   itself. Use dedicated local paths. Roots, reparse points, and data directories
   containing the binary/configuration/system trees are refused. Existing data
   with explicit child ACLs or protected inheritance requires an operator ACL
   review first; the script does not silently rewrite those boundaries.

The script also creates the local group `Analyte users` when it is missing,
with the LocalAccounts cmdlets of 64-bit Windows PowerShell 5.1 (present on
Windows 10 and 11, Home included). Its members and the PC's administrators may
sign in to Analyte with their Windows account (ADR 0021, Windows sign-in;
[Who may sign in](../../docs/operations/sign-in.md)). It adds the person who
started the script: when UAC asked a standard user for an administrator's
password, setup runs as that administrator, so the script takes the owner of
the nearest older ancestor process in the same Windows session that belongs to
another person's account (the original setup or Explorer); otherwise the account
running it. A service account such as SYSTEM is never added; the script then
says to add members by hand. Rerunning it keeps the group and its members.
`uninstall.ps1` never removes the group.

The script creates or updates `Analyte` under the virtual account
`NT SERVICE\Analyte`, with automatic startup and restart recovery after 5, 15
and 60 seconds (failure count resets after a day). It stops an existing managed
service before updating it; rerunning restores the intended configuration and
permissions. It refuses a service with the same name under a different account.
Close Services consoles if a prior removal remains marked for deletion.

The data root gets a protected ACL with only SYSTEM and Administrators FullControl
and the virtual service account Modify, inherited by child files/directories.
No Modify permission is granted elsewhere, except the config folder with
`-ConfigWritable`, which gets the same protected shape. The service keeps its own
`credentials` folder under the data root even stricter (SYSTEM and the service
only); the script leaves that folder alone. The directory and its database,
WAL/SHM files, raw evidence, handoff areas, backups and logs are clinical storage;
keep them protected. Grant the virtual account read access to configuration
and serial/device access separately as required by the site's hardware policy.
Do not run the service as LocalSystem to bypass a permission problem.

Analyte checks this at startup and before loading the admin token
([ADR 0017](../../docs/adr/0017-windows-access-list-unsafe.md)). It refuses to
run when the data directory, or the token file, has an owner or any allow
entry (explicit or inherited) other than SYSTEM, Administrators, the account
it runs as or `NT SERVICE\Analyte`. The error names the SID to remove. A data
directory that Analyte creates itself gets the installer's protected ACL.

Provision the host's required `ANALYTE_SECRET_<NAME>` environment variables for
the service before starting it. A terminal's environment does not configure an
SCM service. Use your deployment's protected service environment provisioning;
do not put credentials in TOML, command arguments or this repository. Restrict
credential access to administrators and the service identity. Restart after
rotation and verify availability under the service account. The installer does
not manage secrets or machine-wide environment variables.

Start with `Start-Service Analyte`; inspect `Get-Service Analyte` and
`C:\ProgramData\Analyte\data\analyte.log` (or the configured data directory).
No login is needed at boot. Configure the PC not to sleep. Logs contain component
IDs and diagnostics, never patient payloads. Configure protected log rotation.
Without `-Start`, installation configures startup but leaves the service stopped so
permissions and credentials can be verified before the first start.

Stop safely with `Stop-Service Analyte` or Services.msc. The SCM stop and Windows
shutdown controls use the staged graceful drain. After the configured bound,
Analyte reports Stopped, logs one error naming remaining component IDs and exits
3 without waiting on blocked destructors (ADR 0003 crash recovery). The SCM shows
a normal stop: the reason is in the Analyte log and process exit code, not an SCM
service-specific code. Recovery actions do not restart a requested normal stop,
including this deadline case. Windows shutdown policy can impose its own limits;
stop the service explicitly and wait for Stopped before planned OS maintenance.

Uninstall with `.\packaging\windows\uninstall.ps1` in an elevated shell. It waits
up to 310 seconds for a safe stop and removes only the service registration.
Running it again succeeds when already absent. It never deletes the data
directory, configuration, logs, backups or credentials, and retains their ACLs.
Uninstall does not authorize replaying uncertain orders or deleting evidence.

References: [virtual accounts](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-service-accounts),
[service creation](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/sc-create)
(virtual accounts need no password),
and [recovery actions](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsa).
