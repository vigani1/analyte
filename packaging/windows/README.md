# Windows service installation

Evidence: documented. These PowerShell 5.1 scripts do not qualify a real analyzer.
Use an elevated Windows PowerShell window. Build a release without `test-hooks`;
there is no MSI, installer GUI, binary signing or automatic upgrade mechanism.

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
   `-ShutdownTimeoutSecs` (30 through 300, default 30). The data directory must
   match the TOML setting; the script runs `check-config` but does not parse TOML
   itself. Use dedicated local paths. Roots, reparse points, and data directories
   containing the binary/configuration/system trees are refused. Existing data
   with explicit child ACLs or protected inheritance requires an operator ACL
   review first; the script does not silently rewrite those boundaries.

The script creates or updates `Analyte` under the virtual account
`NT SERVICE\Analyte`, with automatic startup and restart recovery after 5, 15
and 60 seconds (failure count resets after a day). It stops an existing managed
service before updating it; rerunning restores the intended configuration and
permissions. It refuses a service with the same name under a different account.
Close Services consoles if a prior removal remains marked for deletion.

The data root gets a protected ACL with only SYSTEM and Administrators FullControl
and the virtual service account Modify, inherited by child files/directories.
No Modify permission is granted elsewhere. The directory and its database,
WAL/SHM files, raw evidence, handoff areas, backups and logs are clinical storage;
keep them protected. Grant the virtual account read access to configuration
and serial/device access separately as required by the site's hardware policy.
Do not run the service as LocalSystem to bypass a permission problem.

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
Installation configures startup but deliberately leaves the service stopped so
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
[service configuration](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/change-method-in-class-win32-service),
and [recovery actions](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsa).
