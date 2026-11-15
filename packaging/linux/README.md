# Linux systemd installation

Evidence: documented; no real analyzer or Linux runtime qualification is claimed.
Requires systemd, a `dialout` group (use the distribution's serial group instead
where different), and a Linux release binary built without `test-hooks`.
Run these steps as an administrator from the repository root.

1. Create the dedicated account if absent:

   ```sh
   getent group analyte >/dev/null || sudo groupadd --system analyte
   id analyte >/dev/null 2>&1 || sudo useradd --system --gid analyte --home-dir /var/lib/analyte --shell /usr/sbin/nologin analyte
   sudo install -o root -g root -m 0755 target/release/analyte /usr/local/bin/analyte
   sudo install -d -o root -g analyte -m 0750 /etc/analyte
   sudo install -d -o analyte -g analyte -m 0700 /var/lib/analyte
   ```

2. Install the site's validated program, definitions and site files in
   `/etc/analyte`, owned by root, readable by group `analyte`, and not writable
   by the service. In `/etc/analyte/analyte.toml`, set
   `data_directory = "/var/lib/analyte"`. Other configuration paths are relative
   to the program file. Configure file handoff/archive/quarantine paths under
   `/var/lib/analyte`; the unit permits writes only there and in private temporary
   storage. Do not copy the synthetic example as an operational configuration.
   Check it with `sudo -u analyte /usr/local/bin/analyte check-config /etc/analyte/analyte.toml`.

3. Supply required `ANALYTE_SECRET_<NAME>` variables in
   `/etc/analyte/environment` using systemd EnvironmentFile syntax (`NAME=value`,
   no `export`). Create this file as root with mode 0600; do not put credentials
   in the unit, command line or source control. The system manager reads it before
   dropping privileges. Configure only nonclinical diagnostics with `RUST_LOG`.

4. Install and start the unit:

   ```sh
   sudo install -o root -g root -m 0644 packaging/linux/analyte.service /etc/systemd/system/analyte.service
   sudo systemd-analyze verify /etc/systemd/system/analyte.service
   sudo systemctl daemon-reload
   sudo systemctl enable --now analyte.service
   sudo systemctl status analyte.service
   ```

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

To remove the service, stop and disable it with
`sudo systemctl disable --now analyte.service`, remove only the installed unit,
and run `sudo systemctl daemon-reload`. Keep `/var/lib/analyte`, backups and
credentials protected; uninstalling does not authorize deleting clinical data.

References: [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)
and [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html).
