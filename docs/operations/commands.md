# Commands

Everything the admin pages and the desktop app show or do can also be done
from PowerShell with `analyte.exe`. This page lists every command, grouped by
task, with one example each. The pages and the commands use the same rules
and the same words; only the layout differs.

## Before you start

Open PowerShell **as administrator** (the commands read the protected data
folder) and set two things once per window:

```powershell
Set-Alias analyte "C:\Program Files\Analyte\analyte.exe"
$env:ANALYTE_CONFIG = "C:\ProgramData\Analyte\config\analyte.toml"   # used below as --config $env:ANALYTE_CONFIG
```

- Every command takes `--config <program file>`; without it Analyte looks for
  `analyte.toml` in the current folder.
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
| Is everything OK? The Overview in words, with where and how to fix each thing. Exit code 0 all good, 1 needs a look, 2 a problem stops data, 3 unknown. | `analyte --config $env:ANALYTE_CONFIG overview` |
| One read-only check of the whole installation: program file, engine, each machine's port, each lab app (address, HTTPS certificate, keys, last order list), disk, Windows sign-in group, firewall rule. Exit code 0 OK, 1 warning, 2 failure. | `analyte --config $env:ANALYTE_CONFIG doctor` |
| Check a program file before using it | `analyte check-config C:\ProgramData\Analyte\config\analyte.toml` |
| Technical health lines for support | `analyte status C:\ProgramData\Analyte\config\analyte.toml` |
| Active alerts, each with its reason and fix | `analyte --config $env:ANALYTE_CONFIG alerts list` |
| Where the log is and its last lines (the log holds no patient data) | `analyte --config $env:ANALYTE_CONFIG logs --tail 100` |
| What Analyte stored, newest first (the Activity page) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna activity --limit 50` |

`overview` and `doctor` ask the running engine at its admin address, because
only the engine knows each machine's live link. If the engine does not answer,
`overview` says the state is unknown (exit code 3), never "All good".

In a script:

```powershell
analyte --config $env:ANALYTE_CONFIG overview --json | Out-File status.json
if ($LASTEXITCODE -ge 2) { Write-Warning "Analyte needs someone now" }
```

## Machines

These use the Setup page's own checks and write the same audit row; changes
apply after `Restart-Service Analyte`. Options for each (models, ports,
addresses, `--dry-run`, `--anyway`) are in [Connect](connect.md#from-the-command-line).

| Task | Command |
| --- | --- |
| List machines, with the live state when the engine runs | `analyte --config $env:ANALYTE_CONFIG machines list` |
| List machine models, ready or not | `analyte --config $env:ANALYTE_CONFIG machines models` |
| List serial ports with adapter and USB serial number | `analyte --config $env:ANALYTE_CONFIG machines ports` |
| Add a machine (not set up yet) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna machines add hl7-1` |
| Set it up: model, connection, lab app | `analyte --config $env:ANALYTE_CONFIG --actor it-anna machines set hl7-1 --model vendor/model --address 192.168.1.20 --port 5600 --lab-app main-lab` |
| Listen-only connection test (nothing sent or stored) | `analyte --config $env:ANALYTE_CONFIG machines test hl7-1 --seconds 30` |
| Remove a machine (its results and history stay) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna machines remove hl7-1` |

## Lab apps

Keys are never options: the command asks for them without showing them, or
reads them from standard input. Details are in [Connect](connect.md#from-the-command-line).

| Task | Command |
| --- | --- |
| List lab apps and where each key is found (never its value) | `analyte --config $env:ANALYTE_CONFIG lab-apps list` |
| Add a lab app, then enter its two keys | `analyte --config $env:ANALYTE_CONFIG --actor it-anna lab-apps add main-lab --address https://lab.example.org` |
| Change its address or timing | `analyte --config $env:ANALYTE_CONFIG --actor it-anna lab-apps set main-lab --request-timeout-seconds 20` |
| Replace its keys | `analyte --config $env:ANALYTE_CONFIG --actor it-anna lab-apps keys main-lab` |
| Remove a lab app no machine uses | `analyte --config $env:ANALYTE_CONFIG --actor it-anna lab-apps remove main-lab` |

## Needs review

`review` and `parked` are the same command.

| Task | Command |
| --- | --- |
| List items that need review | `analyte --config $env:ANALYTE_CONFIG --actor it-anna review list` |
| Show one item (protected evidence) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna review show 12` |
| Process an item again with the current settings | `analyte --config $env:ANALYTE_CONFIG --actor it-anna review reprocess 12 --reason "Mapping fixed"` |
| Mark an item resolved (nothing is deleted) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna review resolve 12 --reason "Checked with the lab"` |
| Process one stored message again | `analyte --config $env:ANALYTE_CONFIG --actor it-anna receipts reprocess <message-id> --reason "Mapping fixed"` |

## Deliveries

| Task | Command |
| --- | --- |
| List deliveries to lab apps | `analyte --config $env:ANALYTE_CONFIG --actor it-anna deliveries list --reason "Daily check"` |
| Send a delivery again, with its original ID and bytes | `analyte --config $env:ANALYTE_CONFIG --actor it-anna deliveries resend <delivery-id> --reason "Lab app fixed"` |
| Send alert messages again after the relay is repaired | `analyte --config $env:ANALYTE_CONFIG --actor it-anna retry-alerts lab-relay --reason "Relay repaired"` |

## Orders

| Task | Command |
| --- | --- |
| List machine orders and their state | `analyte --config $env:ANALYTE_CONFIG --actor it-anna orders list` |
| Orders a machine cancelled on its own (shows sample IDs, as the page does) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna orders cancelled` |
| Record what really happened to an order with an unknown outcome | `analyte --config $env:ANALYTE_CONFIG --actor it-anna orders reconcile 7 --as accepted --reason "Seen on the machine"` |
| Resume order sending after a restore, once reconciled | `analyte --config $env:ANALYTE_CONFIG --actor it-anna orders resume-after-restore --reason "All machines reconciled"` |

## Data keeping

| Task | Command |
| --- | --- |
| How long data is kept, the last clean-up, the oldest message, the holds | `analyte --config $env:ANALYTE_CONFIG data-keeping` |
| Keep one message from clean-up | `analyte --config $env:ANALYTE_CONFIG --actor it-anna holds receipt <message-id> --reason "Investigation"` |
| Release that hold | `analyte --config $env:ANALYTE_CONFIG --actor it-anna holds receipt <message-id> --release --reason "Closed"` |
| Keep an order and its messages | `analyte --config $env:ANALYTE_CONFIG --actor it-anna holds order <lab-app> <order> --reason "Investigation"` |

## Backup and restore

See [Protect and recover](recovery.md) before restoring.

| Task | Command |
| --- | --- |
| Write a verified backup | `analyte --config $env:ANALYTE_CONFIG backup D:\Backups\analyte-2026-11-21.db` |
| Restore from a backup (stop the engine first) | `analyte --config $env:ANALYTE_CONFIG --actor it-anna restore --from D:\Backups\analyte-2026-11-21.db --reason "Disk replaced"` |

## The engine service

Windows runs the engine as the `Analyte` service; use Windows' own commands
for it (Analyte does not wrap them).

| Task | Command |
| --- | --- |
| Start the engine | `Start-Service Analyte` |
| Stop the engine (wait for Stopped before maintenance) | `Stop-Service Analyte` |
| Restart, for example to apply saved settings | `Restart-Service Analyte` |
| Is it running? | `Get-Service Analyte` |
| Run the engine in this window instead, for a test (Ctrl+C stops it) | `analyte run $env:ANALYTE_CONFIG` |
| Make a new admin key (prints it once; restart afterwards) | `analyte --config $env:ANALYTE_CONFIG admin token --rotate` |

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
