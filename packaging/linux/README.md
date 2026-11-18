# Linux systemd installation

Evidence: documented; no real analyzer or Linux runtime qualification is claimed.
Requires systemd, a `dialout` group (use the distribution's serial group instead
where different), and a Linux release binary built without `test-hooks`.

## Packages

`nfpm.yaml` builds the release's `.deb` and `.rpm` with nfpm
(`packaging/build.ps1` pins the version and runs nfpm from the staged release
folder). The lab manager's steps are in
[Install](../../docs/operations/install.md#linux-packages-recommended).

| Path | What | Owner, mode |
| --- | --- | --- |
| `/usr/bin/analyte` | The program | root, 0755 |
| `/usr/lib/systemd/system/analyte.service` | The unit in this folder | root, 0644 |
| `/usr/share/analyte/machines`, linked from `/etc/analyte/machines` | Shipped machine definitions, replaced on upgrade | root |
| `/etc/analyte` | Configuration; the setup page writes here | root:analyte, 0770 |
| `/etc/analyte/analyte.toml`, `machine-1.site.toml` | Starter program and site file (`starter/`), conffiles: never replaced on upgrade | root:analyte, 0640 |
| `/var/lib/analyte` | Data, credentials, admin token | analyte:analyte, 0700 |

The scripts in `scripts/`:

- `preinstall.sh` creates the `analyte` system group and account (no login
  shell, home `/var/lib/analyte`) before the files are unpacked.
- `postinstall.sh` sets the folder owners and modes above, creates
  `/var/lib/analyte/admin-token` as the `analyte` account when it is missing,
  enables and (re)starts the service, and prints the setup page address and
  `sudo cat /var/lib/analyte/admin-token`. It never prints the token: package
  tools keep this output in logs. Without a running systemd (a container), it
  says how to start the service instead.
- `preremove.sh` stops and disables the service on removal, not on upgrade.
- `postremove.sh` keeps configuration and data. A deb purge deletes
  `/etc/analyte`, but never `/var/lib/analyte`, which holds clinical data,
  credentials and the token; only people delete those.

The service can write `/etc/analyte`, so it could also replace other files
there, such as `/etc/analyte/environment`; keep only Analyte's own files in that
folder. The binary is built on the current Ubuntu GitHub runner, so the packages
need a C library at least as new as that runner's.

## By hand

Run these steps as an administrator from the extracted release folder.

1. Create the dedicated account if absent:

   ```sh
   getent group analyte >/dev/null || sudo groupadd --system analyte
   id analyte >/dev/null 2>&1 || sudo useradd --system --gid analyte --home-dir /var/lib/analyte --shell /usr/sbin/nologin analyte
   sudo install -o root -g root -m 0755 analyte /usr/bin/analyte
   sudo install -d -o root -g analyte -m 0770 /etc/analyte
   sudo install -d -o analyte -g analyte -m 0700 /var/lib/analyte
   sudo cp -r machines /etc/analyte/machines
   ```

   Use mode 0750 for `/etc/analyte` instead if the setup page will not be used;
   the service then cannot change its configuration.

2. Install the site's program and site files in `/etc/analyte`, or start from
   `starter/` (one machine waiting for the setup page). In
   `/etc/analyte/analyte.toml`, set `data_directory = "/var/lib/analyte"`. This
   setting must be exactly that path: it is the unit's only writable data
   directory. Other configuration paths are relative to the program file.
   Configure file handoff/archive/quarantine paths under `/var/lib/analyte`; the
   unit permits writes only there, in `/etc/analyte` and in private temporary
   storage. Do not copy the synthetic example as an operational configuration.
   Check it with `sudo -u analyte /usr/bin/analyte check-config /etc/analyte/analyte.toml`.

3. Supply required `ANALYTE_SECRET_<NAME>` variables in
   `/etc/analyte/environment` using systemd EnvironmentFile syntax (`NAME=value`,
   no `export`), or store keys on the setup page. Create this file as root with
   mode 0600; do not put credentials in the unit, command line or source control.
   The system manager reads it before dropping privileges. Configure only
   nonclinical diagnostics with `RUST_LOG`. If `[admin]` is enabled, create its
   token as the service account:
   `sudo -u analyte /usr/bin/analyte --config /etc/analyte/analyte.toml admin token --rotate`.

4. Install and start the unit:

   ```sh
   sudo install -o root -g root -m 0644 packaging/linux/analyte.service /etc/systemd/system/analyte.service
   sudo systemd-analyze verify /etc/systemd/system/analyte.service
   sudo systemctl daemon-reload
   sudo systemctl enable --now analyte.service
   sudo systemctl status analyte.service
   ```

## Running

The unit starts without a login and restarts on failures after five seconds.
`Type=simple` does not certify startup readiness; inspect status and diagnostics.
The serial supplementary group grants device access; private devices are
intentionally not enabled. Confirm actual device permissions and stable serial
paths on the lab PC. Configure the PC not to sleep.

Stop safely with `sudo systemctl stop analyte.service`. SIGTERM and SIGINT use
the same staged graceful drain. Analyte has a 30-second shutdown deadline; the
unit's `TimeoutStopSec=35s` gives it a five-second margin. If changing
`--shutdown-timeout-secs` (30 through 300), change `TimeoutStopSec` to the same
bound plus five seconds using `sudo systemctl edit analyte.service`. A deadline
expiry logs the remaining component IDs and exits 3; systemd records failure.
A graceful stop exits 0. Forced exit follows ADR 0003 crash recovery; never delete
journals or automatically resend uncertain orders to get the service running.

Read logs with `sudo journalctl -u analyte.service`. Logs contain IDs and
diagnostics, never patient payloads. Configure journal access and retention.
Clinical evidence, the database and WAL/SHM files remain in the protected data
directory. Back up and restore using the program's documented operator commands.

To remove a manual installation, stop and disable the service with
`sudo systemctl disable --now analyte.service`, remove only the installed unit,
and run `sudo systemctl daemon-reload`. Keep `/var/lib/analyte`, backups and
credentials protected; uninstalling does not authorize deleting clinical data.

References: [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html),
[systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html)
and [nfpm configuration](https://nfpm.goreleaser.com/configuration/).
