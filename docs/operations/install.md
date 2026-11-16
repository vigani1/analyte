# Install the pilot

## Requirements

| Need | Check before installation |
| --- | --- |
| Host | Always-on Windows or Linux PC. Disable sleep. Schedule OS updates. |
| Windows | PowerShell 5.1 and administrator access for service installation. |
| Linux | systemd for the supplied unit; administrator access to install it. |
| Devices | Confirmed serial adapters, drivers, cable pinouts and stable COM/device paths, or agreed lab-network TCP/file handoff. |
| Network | Outbound HTTPS to the lab app and configured alert relays with valid trusted certificates; correct clock for signatures. No inbound internet port. |
| Storage | Dedicated protected local data directory, measured outage capacity, disk reserve and separate protected backups. |
| People | Lab operator, IT recovery lead and app support, with cover and contact procedures. |

There is no qualified minimum CPU, RAM, disk size or OS-version matrix yet.
Disk checks use native operating-system free-space calls through `fs4`.
Measure startup, storage commits and sustained traffic on the pilot PC against
machine deadlines. Capacity must cover the agreed outage plus retained data,
journals, backups and maintenance working space. Rust is not needed to run a
release binary. Never install a build with the test-only `test-hooks` feature.

## Obtain and verify the program

Obtain an owner-approved release archive for the host. Follow the checksum and
GitHub provenance verification in [release instructions](../release.md),
including checking the source tag and commit. Extract the versioned directory.
If no approved release exists, ask the maintainer for an approved pilot build;
there is no release-download command in Analyte.

There is no MSI, installation GUI, Authenticode signing or automatic updater.
Keep the verified previous archive for recovery.

## Windows service

Use an elevated PowerShell window. Paths below are the packaged defaults.

1. Place `analyte.exe` in `C:\Program Files\Analyte`. Administrators own the
   binary and configuration trees. Ordinary users must not write there.
2. Prepare `C:\ProgramData\Analyte\config\analyte.toml` and its referenced
   files using [Connect](connect.md). Use
   `data_directory = 'C:\ProgramData\Analyte\data'` in TOML.
3. From the extracted archive directory, run:

   ```powershell
   .\packaging\windows\install.ps1
   ```

   The script checks config, creates or updates service `Analyte` under
   `NT SERVICE\Analyte`, and leaves it stopped. It sets automatic startup and
   failure restarts after 5, 15 and 60 seconds. It protects the data directory:
   SYSTEM and Administrators get FullControl; the service gets Modify.
4. Grant the service Read/Execute on binaries, Read on config and the required
   device access. The installer does not grant these. Provision credentials
   for the service environment, not just the terminal. See [Connect](connect.md).
   If admin is enabled, provision its token as described in [Operate](operate.md).
5. Start and inspect:

   ```powershell
   Start-Service Analyte
   Get-Service Analyte
   ```

Read `C:\ProgramData\Analyte\data\analyte.log` and check the admin page.
Running service status alone does not prove connection or successful delivery.
Configure protected log rotation separately.

The script accepts `-Executable`, `-Config`, `-DataDirectory` and
`-ShutdownTimeoutSecs` (30–300, default 30). The directory parameter must match
TOML; the script does not compare them. Use dedicated local directories without
reparse points. Do not run as LocalSystem to work around permissions.
See [the full Windows procedure](../../packaging/windows/README.md) for ACL
review, script execution policy and installation refusals.

Stop with `Stop-Service Analyte`. Wait for Stopped before maintenance.
Shutdown is bounded. A deadline exit is code 3; Windows can still show a normal
stop. Inspect the log for remaining component IDs. Do not delete journals or
resend uncertain machine orders after an interrupted stop.

The packaged `packaging/windows/uninstall.ps1` removes the service only.
Data, configuration, backups, credentials and permissions remain.

## Linux or foreground operation

Follow the [systemd procedure](../../packaging/linux/README.md), replacing its
source-build binary path with the extracted binary. It uses `/etc/analyte`
for config, `/var/lib/analyte` for data and a dedicated `analyte` account.
The supplied unit permits persistent writes only under `/var/lib/analyte`.
Use `sudo systemctl stop analyte.service` for maintenance and
`sudo journalctl -u analyte.service` for diagnostics.

For supervised commissioning outside a service:

```text
analyte check-config analyte.toml
analyte run analyte.toml
```

Ctrl-C requests graceful shutdown. Do not run the foreground program alongside
the service against the same devices.
