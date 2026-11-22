# Install the pilot

## Requirements

| Need | Check before installation |
| --- | --- |
| Host | Always-on Windows or Linux PC. Disable sleep. Schedule OS updates. |
| Windows | PowerShell 5.1 and administrator access for service installation. |
| Linux | systemd for the supplied unit; administrator access to install it. |
| Devices | Confirmed serial adapters, drivers, cable pinouts and stable COM/device paths, or agreed lab-network TCP/file handoff. |
| Network | Outbound HTTPS to the lab app and configured alert relays with valid trusted certificates; correct clock for signatures. No inbound internet port. Analyzers that connect to Analyte must be on the lab PC's own network, or IT must allow them through; see [Firewall](connect.md#firewall). |
| Storage | Dedicated protected local data directory, measured outage capacity, disk reserve and separate protected backups. |
| People | Lab operator, IT recovery lead and app support, with cover and contact procedures. |

There is no qualified minimum CPU, RAM, disk size or OS-version matrix yet.
Disk checks use native operating-system free-space calls through `fs4`.
Measure startup, storage commits and sustained traffic on the pilot PC against
machine deadlines. Capacity must cover the agreed outage plus retained data,
journals, backups and maintenance working space. Rust is not needed to run a
release binary. Never install a build with the test-only `test-hooks` feature.

## Obtain and verify the program

Obtain an owner-approved release for the host: the Windows installer
(`analyte-v<version>-x86_64-windows-setup.exe`), the Linux `.deb` or `.rpm`, or
the archive for a manual installation. Follow the checksum and GitHub
provenance verification in [release instructions](../develop/release.md), including
checking the source tag and commit. If no approved release exists, ask the
maintainer for an approved pilot build; there is no release-download command in
Analyte. There is no automatic updater. Keep the verified previous release for
recovery.

## What a fresh installation holds

The installers write a starter program with the admin page on port 8080 and one
machine, `machine-1`, that has no machine definition yet. Such a machine waits
for the [Setup page](operate.md#setup-page) and does not run, so the service
starts without any lab connection or key. The starter site file holds only the
retention (`keep`, so nothing is ever deleted until the lab chooses a period).
No `lab-apps.toml` is shipped: add the lab app, with its address and keys, on
the [Lab apps page](operate.md#lab-apps-page), which writes that file beside the
program file; the setup page then fills in the connection and picks the lab
app. An upgrade never replaces this configuration.
For more analyzers, press **Add a machine** on the setup page; to take one out of
service, press **Remove machine** (see [Setup page](operate.md#setup-page)).
Nobody needs to edit the files by hand.

The installers also create the admin token (`admin-token` in the data folder)
when there is none. You need it to log in; the setup page needs a browser login.

## Windows installer (recommended)

The Windows installer is unsigned for now: Windows SmartScreen will warn when it
starts. Check the file's checksum first, then choose **More info** → **Run
anyway**.

1. Run the installer and accept the administrator prompt. Choose what to
   install: **Engine and Analyte app (recommended)**, or **Engine only (set up
   from PowerShell or a browser)**; see [Install without the app](#install-without-the-app).
   Leave **Let me type `analyte` in PowerShell (add Analyte to the PATH)**
   ticked, so a new PowerShell window runs `analyte` without its full path. It
   installs Analyte in `C:\Program Files\Analyte`, the machine definitions and
   the starter configuration in `C:\ProgramData\Analyte\config`, and protects
   `C:\ProgramData\Analyte\data`.
2. It registers and starts the `Analyte` service exactly as the script below
   does, lets the service write the config folder for the setup page, and lets
   people signed in to this PC start (never stop) the service, for the app's
   **Start engine** button. It creates the Windows group **Analyte users**, if
   missing, and adds you; see [Who may sign in](sign-in.md). It adds the
   Windows Firewall rule **Analyte - analyzers on the lab network**, which lets
   analyzers on this PC's own network connect to Analyte; see
   [Firewall](connect.md#firewall).
3. It installs the **Analyte app** (`analyte-app.exe`) with a Start menu entry
   "Analyte". The app shows the same pages in its own window and keeps a tray
   icon with the engine's state; see [The Analyte app](#the-analyte-app). Leave
   **Start Analyte when I sign in** ticked to have the tray icon after every
   sign-in.
4. The last pages show the new admin token **once**. Copy it somewhere safe,
   like a password. To read it again later, open PowerShell as administrator and
   run `Get-Content C:\ProgramData\Analyte\data\admin-token`. Do not browse to
   the data folder in File Explorer and accept its permission prompt: that adds
   your account to the folder and Analyte then refuses to start.
5. Leave **Open Analyte** ticked and finish. Sign in with the token, choose
   **Setup**, then **Set up** for `machine-1`. For each further analyzer, press
   **Add a machine** and set it up the same way.

The app's window needs the Microsoft Edge WebView2 Runtime, which Windows 11
has. On a Windows 10 PC without it, the last page says so and opens the setup
page in the browser instead; install the WebView2 Runtime (Evergreen) from
Microsoft, then open Analyte from the Start menu.

Running it again upgrades the program and keeps the configuration, data,
token and your earlier choices. Choosing **Engine only** on an upgrade removes
the app the way uninstall does. Uninstall from Windows Settings → Apps: it
closes the app, stops and removes the service, its firewall rule and its PATH
entry, removes the program files, the Start menu entry and the
installing user's sign-in entry and app folders, and keeps data, configuration, backups, credentials and
the token in `C:\ProgramData\Analyte`. It also keeps the Windows group
**Analyte users**, which may list other people. Delete those yourself only after the lab
has decided what to do with the clinical data they hold.

## Install without the app

For a PC where nobody wants the desktop app, install only the engine and set
everything up from an administrator PowerShell window (or a browser, below).
In the installer choose **Engine only**; for a silent installation by IT:

```powershell
$setup = Start-Process -Wait -PassThru -FilePath .\analyte-v<version>-x86_64-windows-setup.exe `
    -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/TYPE=engine'
$setup.ExitCode   # 0 done; 10 a step failed (add /LOG=<file> to see which)
```

- `/TYPE=engine` installs the engine only, `/TYPE=full` the engine and the app
  (the default). `/COMPONENTS` has no effect, because the installer has no
  custom type.
- The PATH task is ticked unless you add `/MERGETASKS=!path`. `/TASKS=`
  replaces the whole list of tasks, so prefer `/MERGETASKS`.
- An upgrade keeps the earlier type and tasks unless you pass them again.
- A silent installation shows no admin token. Read it from an administrator
  PowerShell with `Get-Content C:\ProgramData\Analyte\data\admin-token`.

Then open a **new** PowerShell window as administrator, so it sees the new
PATH, and set Analyte up. Commands read the installed program file
`C:\ProgramData\Analyte\config\analyte.toml` without `--config` (they say so
on the first line); `--actor` names you in the audit trail. The model key comes
from `analyte machines models`; the names, addresses and port are examples:

```powershell
analyte doctor                                   # checks the installation; changes nothing
analyte --actor it-alex lab-apps add main-lab --address https://lab.example.org
analyte --actor it-alex lab-apps keys main-lab   # asks for both keys without showing them
analyte machines models                          # models, ready or not
analyte --actor it-alex machines set machine-1 --model vendor/model `
    --address 192.168.1.20 --port 5600 --lab-app main-lab --dry-run
analyte --actor it-alex machines set machine-1 --model vendor/model `
    --address 192.168.1.20 --port 5600 --lab-app main-lab
analyte --actor it-alex machines add hl7-2       # each further analyzer, then set it up the same way
analyte machines test machine-1 --seconds 30     # listens only; nothing sent or stored
Restart-Service Analyte                          # changes apply after a restart
analyte machines list
```

`lab-apps keys` reads the keys only from that hidden prompt or from standard
input, never from the command line. While the engine runs, the command hands
them to the engine, which stores them in its locked keys file exactly as the
Lab apps page does, after the engine has proved it holds the admin key. When
the engine is stopped, the command stores them itself if this account may
write the locked file; otherwise it asks you to run `Start-Service Analyte`
and the command again. Every option is in [From the command
line](connect.md#from-the-command-line).

**Using a browser instead:** on this PC open `http://127.0.0.1:8080/setup`,
log in with the admin token, then use **Lab apps** and **Setup** as described
in [Operate](operate.md#setup-page).

## The Analyte app

The app replaces opening `http://127.0.0.1:8080` in a browser. It never runs the
engine and never holds the admin key: you sign in inside its window with the
normal form.

- **Window:** the engine's pages, only them. Links to other sites open in your
  browser. Closing the window frees its memory; the app stays in the tray.
- **Tray icon:** the Analyte mark with a badge: green tick (all good), amber
  triangle (something needs you), red octagon (a problem stops data) or a grey
  pause (the engine is not answering). Its menu has **Open Analyte**, the state
  in words, **Start at sign-in** and **Quit app**, which closes only the app;
  the engine keeps running.
- **Notifications:** one when the state gets worse or something new needs
  someone, saying what happened and what to do, such as "A machine’s
  connection is off". Clicking it, also later in the notification centre,
  opens the window on the Overview. The title bar keeps the last one, with
  "Fixed since" and the time once it is gone.
- **Engine off:** when the engine does not answer, the window shows what the app
  saw and, if the service is stopped, **Start engine**. It switches back to the
  pages by itself once the engine answers.

The app reads the engine's admin port from `analyte-app.toml` beside it, which
setup writes from the program file. If you change `[admin] port`, run the
installer again so the app follows. Its own log is
`%LOCALAPPDATA%\Analyte\analyte-app.log` for each Windows user.

## Windows service by script

Use an elevated PowerShell window. Paths below are the packaged defaults. The
installer runs this same script with `-ConfigWritable -CreateAdminToken -UsersMayStart -Start`,
and `-AddToPath` when its PATH task is ticked.

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
   Analyte refuses to start if the data directory grants anyone else access,
   including inherited BUILTIN\Users read access; the error names the SID.
4. Grant the service Read/Execute on binaries, Read on config and the required
   device access; without switches the script grants none of these. To use the
   admin [Setup page](operate.md#setup-page), copy the archive's `machines`
   folder to `C:\ProgramData\Analyte\config\machines` and add
   `-ConfigWritable`: the config folder then gets the data folder's protected
   access list (SYSTEM and Administrators full control, the service Modify,
   nothing inherited). Provision credentials for the service environment, not
   just the terminal. See [Connect](connect.md). If admin is enabled, add
   `-CreateAdminToken` (creates the token only when missing; read it as
   described above) or provision it as described in [Operate](operate.md).
5. Start and inspect:

   ```powershell
   Start-Service Analyte
   Get-Service Analyte
   ```

Read `C:\ProgramData\Analyte\data\analyte.log` and check the admin page.
Running service status alone does not prove connection or successful delivery.
Configure protected log rotation separately.

The script accepts `-Executable`, `-Config`, `-DataDirectory`,
`-ShutdownTimeoutSecs` (30–300, default 30) and the switches `-ConfigWritable`,
`-CreateAdminToken`, `-Start` (start the service instead of leaving it
stopped) and `-AddToPath` (add the program's folder to the system PATH once,
so new PowerShell windows run `analyte`). The directory parameter must match
TOML; the script does not compare them. Use dedicated local directories without
reparse points. Do not run as LocalSystem to work around permissions.
See [the full Windows procedure](../../packaging/windows/README.md) for ACL
review, script execution policy and installation refusals.

Stop with `Stop-Service Analyte`. Wait for Stopped before maintenance.
Shutdown is bounded. A deadline exit is code 3; Windows can still show a normal
stop. Inspect the log for remaining component IDs. Do not delete journals or
resend uncertain machine orders after an interrupted stop.

The packaged `packaging/windows/uninstall.ps1` removes the service, its
firewall rule and its PATH entry. Data, configuration, backups, credentials
and permissions remain.

## Linux packages (recommended)

Install the package for the distribution:

```sh
sudo apt install ./analyte_<version>-1_amd64.deb     # Debian, Ubuntu
sudo dnf install ./analyte-<version>-1.x86_64.rpm    # Fedora, RHEL family
```

The package creates the `analyte` system account, installs `/usr/bin/analyte`,
the systemd unit and the machine definitions (`/usr/share/analyte/machines`,
linked from `/etc/analyte/machines`). It writes the starter configuration in
`/etc/analyte` (configuration files that an upgrade never replaces), protects
`/var/lib/analyte` (mode 0700, owned by `analyte`) and lets the service write
`/etc/analyte` (owner root, group `analyte`, mode 0770) for the setup page. It
creates the admin token when missing, enables and starts the service, and prints
the setup page address. It never prints the token, because package tools keep
their output in logs. Read it with `sudo cat /var/lib/analyte/admin-token`, then
open `http://127.0.0.1:8080/setup` on that computer and log in.

The packages change no firewall. Where firewalld or ufw is active, open each
port an analyzer connects to, for the analyzer's network only; see
[Firewall](connect.md#firewall).

Removing the package (`apt remove`, `dnf remove`) stops and disables the
service and keeps configuration and data. `apt purge` also deletes
`/etc/analyte`, but never `/var/lib/analyte`: clinical data, credentials and the
token stay until a person deletes them.

The packages are built on the current Ubuntu GitHub runner, so they need a
distribution whose C library is at least as new as that runner's (for example
Ubuntu 24.04 or later). Older distributions are not supported yet.

## Linux by hand or foreground operation

Follow the [systemd procedure](../../packaging/linux/README.md) with the
extracted binary. It uses `/etc/analyte` for config, `/var/lib/analyte` for data
and a dedicated `analyte` account; the unit lets the service write only those two
folders. Use `sudo systemctl stop analyte.service` for maintenance and
`sudo journalctl -u analyte.service` for diagnostics.

For supervised commissioning outside a service:

```text
analyte check-config analyte.toml
analyte run analyte.toml
```

Ctrl-C requests graceful shutdown. Do not run the foreground program alongside
the service against the same devices.
