# Commands

Everything the admin pages and the desktop app show or do can also be done
from PowerShell with `analyte.exe`. This page lists every command, grouped by
task, with one example each. The pages and the commands use the same rules
and the same words; only the layout differs.

## Before you start

Open PowerShell **as administrator** (the commands read the protected data
folder). If the installer's PATH task was ticked (the default), type
`analyte`; otherwise set an alias once per window:

```powershell
Set-Alias analyte "C:\Program Files\Analyte\analyte.exe"   # only without the PATH entry
```

- Every command takes `--config <program file>`. Without it, Analyte uses the
  file named by `ANALYTE_CONFIG`; else `analyte.toml` in the current folder,
  if there is one; else the installed program file
  (`C:\ProgramData\Analyte\config\analyte.toml`; on Linux
  `/etc/analyte/analyte.toml`). A file found this way is named on the first
  line of standard error, so `--json` output stays clean; with none, the
  error names every place looked. The examples below rely on this.
- On a PC with only the engine installed (no desktop app), the first steps
  are in [Install without the app](install.md#install-without-the-app).
- Commands that change something, and reads that show sample IDs, record who
  ran them: pass `--actor <your name>`. Changes also need `--reason "<why>"`,
  which the audit keeps.
- `--json` (where offered) prints the same answer for scripts.
- Long lists come in pages: each page ends with the exact command for the
  next one (`--after` or `--before`).
- No command prints a key, a token or a result value. Lists that show sample
  IDs say so in their help (`analyte <command> --help`).
- Tab completion: `analyte completions powershell | Out-String | Invoke-Expression`
  (add it to your PowerShell profile to keep it).

## Check health

| Task | Command |
| --- | --- |
| Is everything OK? The Overview in words, with where and how to fix each thing. Exit code 0 all good, 1 needs a look, 2 a problem stops data, 3 unknown. | `analyte overview` |
| One read-only check of the whole installation: program file, engine, each machine's port, each lab app (address, HTTPS certificate, keys, last order list), disk, Windows sign-in group, firewall rule. Exit code 0 OK, 1 warning, 2 failure. | `analyte doctor` |
| Check a program file before using it | `analyte check-config C:\ProgramData\Analyte\config\analyte.toml` |
| Technical health lines for support | `analyte status C:\ProgramData\Analyte\config\analyte.toml` |
| Active alerts, each with its reason and fix | `analyte alerts list` |
| Confirm that you checked the current settings for the exact software version shown on the machine's warning. The action is audited. | `analyte --actor NAME confirm-software MACHINE VERSION` |
| Where the log is and its last lines (the log holds no patient data) | `analyte logs --tail 100` |
| What Analyte stored, newest first (the Activity page) | `analyte --actor it-anna activity --limit 50` |

`overview` and `doctor` ask the running engine at its admin address, because
only the engine knows each machine's live link. If the engine does not answer,
`overview` says the state is unknown (exit code 3), never "All good".

In a script:

```powershell
analyte overview --json | Out-File status.json
if ($LASTEXITCODE -ge 2) { Write-Warning "Analyte needs someone now" }
```

## Machines

These use the Setup page's own checks and write the same audit row; changes
apply after `Restart-Service Analyte`. Options for each (models, ports,
addresses, `--dry-run`, `--anyway`) are in [Connect](connect.md#from-the-command-line).

| Task | Command |
| --- | --- |
| List machines, with the live state when the engine runs | `analyte machines list` |
| List machine models, ready or not | `analyte machines models` |
| List serial ports with adapter and USB serial number | `analyte machines ports` |
| Add a machine (not set up yet) | `analyte --actor it-anna machines add hl7-1` |
| Set it up: model, connection, lab app | `analyte --actor it-anna machines set hl7-1 --model vendor/model --address 192.168.1.20 --port 5600 --lab-app main-lab` |
| Choose a serial machine's cable settings to match the analyzer's LIS screen ([ADR 0031](../adr/0031-lab-chooses-serial-cable-settings.md)); also on `add` and `test` | `analyte --actor it-anna machines set cobas-1 --baud 9600 --data-bits 8 --parity none --stop-bits 1 --flow-control none` |
| Choose one of the model's own settings, such as its protocol mode or software version ([ADR 0032](../adr/0032-machine-definitions-declare-lab-choices.md)); repeat for several, also on `test`; `machines models` lists them | `analyte --actor it-anna machines set vidas-1 --choice software_version=v2` |
| Record only, for a first visit or a model not ready yet: keeps every byte, sends nothing to a lab app ([ADR 0034](../adr/0034-record-only-connections.md)); also on `test`. The Autolyser also takes `--poll-seconds` and `--reports-carry-controls`, the VIDAS `--vidas-acknowledgment`, the cobas e 411 `--analyzer-mode elecsys` or `cobas` (its protocol type; `machines models` lists it) | `analyte --actor it-anna machines set cobas-1 --model roche/cobas-e411 --record-only --usb-serial A10K3XYZ --analyzer-mode cobas` |
| Leave record only: connect normally again (needs a ready model and a lab app); recorded messages are never delivered | `analyte --actor it-anna machines set cobas-1 --connect-normally --lab-app main-lab` |
| Choose how repeated results are handled ([ADR 0035](../adr/0035-machine-result-identity-recipes.md)): `automatic` (recommended) or `always_ask` (every repeat waits in Needs review) | `analyte --actor it-anna machines set hl7-1 --repeated-results always_ask` |
| Accept only the analyzer's own device ([ADR 0026](../adr/0026-accept-only-the-machine.md)); repeat for several | `analyte --actor it-anna machines set hl7-1 --allowed-peer 192.168.1.50` |
| Accept any device again (shown as a warning) | `analyte --actor it-anna machines set hl7-1 --any-device` |
| Listen-only connection test (nothing sent or stored); names the device that connected | `analyte machines test hl7-1 --seconds 30` |
| Remove a machine (its results and history stay) | `analyte --actor it-anna machines remove hl7-1` |

## Recordings

Recorded conversations ([ADR 0009](../adr/0009-recording-format.md)) live
under the data folder in `captures/<machine>/`. They **contain patient data**:
every command is audited first and says so. Export copies one file byte for
byte and never overwrites. De-identify writes a new copy with synthetic patient
fields and recomputed checksums and leaves the original unchanged
([ADR 0037](../adr/0037-de-identified-recording-copies.md)); review the copy
before sharing it
([Capture, de-identify and replay](../develop/adding-a-machine.md#capture-de-identify-and-replay)).

| Task | Command |
| --- | --- |
| List each machine's recordings: file, size, last change, and whether capture stopped early | `analyte --actor it-anna captures list --machine cobas-1` |
| Copy one recording to a protected folder | `analyte --actor it-anna captures export cobas-1 <file>.rec.jsonl --to D:\Protected --reason "First visit"` |
| Write a de-identified copy, `<file>.deidentified.rec.jsonl`, for sharing; ASTM, HL7, IHE LAW, VIDAS and Autolyser (POCT is refused). A VIDAS recording may ask for `--checksum-case upper` or `lower`, as the machine's settings say | `analyte --actor it-anna captures de-identify cobas-1 <file>.rec.jsonl --to D:\Share --reason "Proven evidence"` |

## Lab apps

Keys are never options: the command asks for them without showing them, or
reads them from standard input. Details are in [Connect](connect.md#from-the-command-line).

| Task | Command |
| --- | --- |
| List lab apps and where each key is found (never its value) | `analyte lab-apps list` |
| Add a lab app, then enter its two keys | `analyte --actor it-anna lab-apps add main-lab --address https://lab.example.org` |
| Change its address or timing | `analyte --actor it-anna lab-apps set main-lab --request-timeout-seconds 20` |
| Replace its keys | `analyte --actor it-anna lab-apps keys main-lab` |
| Remove a lab app no machine uses | `analyte --actor it-anna lab-apps remove main-lab` |

## Needs review

`review` and `parked` are the same command.

Use `analyte --actor OPERATOR parked compare ITEM` to inspect an eligible
comparison and get its snapshot. Use `analyte --actor OPERATOR parked
suggestions` to list missing-tube-ID suggestions for one page of open
items. Follow the printed `--after` cursor for more.

Confirm with `analyte --actor OPERATOR parked decide ITEM ACTION --snapshot
SNAPSHOT --reason "Checked analyzer history"`. Use `resend`, `rerun` or
`supersede` for ACTION. Both comparison and confirmation are audited. Keep
the command output in a protected location because it shows result data.
If confirmation reports a changed comparison, read it again.

| Task | Command |
| --- | --- |
| List items that need review | `analyte --actor it-anna review list` |
| Show one item (protected evidence) | `analyte --actor it-anna review show 12` |
| Process an item again with the current settings | `analyte --actor it-anna review reprocess 12 --reason "Definition fixed"` |
| Mark an item resolved (nothing is deleted) | `analyte --actor it-anna review resolve 12 --reason "Checked with the lab"` |
| Process one stored message again | `analyte --actor it-anna receipts reprocess <message-id> --reason "Definition fixed"` |

## Deliveries

| Task | Command |
| --- | --- |
| List deliveries to lab apps | `analyte --actor it-anna deliveries list --reason "Daily check"` |
| Send a delivery again, with its original ID and bytes | `analyte --actor it-anna deliveries resend <delivery-id> --reason "Lab app fixed"` |
| Send alert messages again after the relay is repaired | `analyte --actor it-anna retry-alerts lab-relay --reason "Relay repaired"` |

## Orders

| Task | Command |
| --- | --- |
| List machine orders and their state | `analyte --actor it-anna orders list` |
| Orders a machine cancelled on its own (shows sample IDs, as the page does) | `analyte --actor it-anna orders cancelled` |
| Record what really happened to an order with an unknown outcome | `analyte --actor it-anna orders reconcile 7 --as accepted --reason "Seen on the machine"` |
| Resume order sending after a restore, once reconciled | `analyte --actor it-anna orders resume-after-restore --reason "All machines reconciled"` |

## Data keeping

| Task | Command |
| --- | --- |
| How long data is kept, the last clean-up, the oldest message, the holds | `analyte data-keeping` |
| Keep one message from clean-up | `analyte --actor it-anna holds receipt <message-id> --reason "Investigation"` |
| Release that hold | `analyte --actor it-anna holds receipt <message-id> --release --reason "Closed"` |
| Keep an order and its messages | `analyte --actor it-anna holds order <lab-app> <order> --reason "Investigation"` |

## Backup and restore

See [Protect and recover](recovery.md) before restoring.

| Task | Command |
| --- | --- |
| Write a verified backup | `analyte backup D:\Backups\analyte-2026-11-21.db` |
| Restore from a backup (stop the engine first) | `analyte --actor it-anna restore --from D:\Backups\analyte-2026-11-21.db --reason "Disk replaced"` |

## The engine service

Windows runs the engine as the `Analyte` service; use Windows' own commands
for it (Analyte does not wrap them).

| Task | Command |
| --- | --- |
| Start the engine | `Start-Service Analyte` |
| Stop the engine (wait for Stopped before maintenance) | `Stop-Service Analyte` |
| Restart, for example to apply saved settings | `Restart-Service Analyte` |
| Is it running? | `Get-Service Analyte` |
| Run the engine in this window instead, for a test (Ctrl+C stops it) | `analyte run` |
| Make a new admin key (prints it once; restart afterwards) | `analyte admin token --rotate` |

On Linux: `sudo systemctl start analyte`, `stop`, `restart` and
`status`; the log is `journalctl -u analyte`.

## Users

Who may sign in with their Windows account is decided by the local
`Analyte users` group (see [Who may sign in](sign-in.md)). Use Windows' own
commands; `analyte doctor` checks that the group exists.

| Task | Command |
| --- | --- |
| Create the group (the installer normally does) | `New-LocalGroup -Name "Analyte users"` |
| Let someone sign in | `Add-LocalGroupMember -Group "Analyte users" -Member "LABPC\anna"` |
| Stop someone signing in | `Remove-LocalGroupMember -Group "Analyte users" -Member "LABPC\anna"` |
| Who may sign in | `Get-LocalGroupMember -Group "Analyte users"` |

For an Autolyser model that leaves the poll cadence to the lab, pass
`--poll-seconds <1..300>` to `analyte machines set` or `machines test`.
`machines models` lists the question alongside any `--choice NAME=VALUE`
settings. `machines list --json` exposes the saved
`autolyser_poll_seconds` and `doctor` reports the interval in use.
