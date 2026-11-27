### English source wording.

cli-lang-help = Interface language (a BCP 47 tag). Defaults to LC_ALL, LANG, or English

cli-interpretation-recorded-decisions-inspect-parked-items-for-failures = Interpretation { $version } recorded; decisions: { $outcomes }. Inspect parked items for failures.

cli-using = Using { $path } ({ $describe }).

cli-scm-dispatch-was-not-selected = SCM dispatch was not selected ({ $shutdown-timeout-secs })

cli-resolution = { $id } { $evidence } resolution={ $as-deref }

cli-item-has-no-eligible-measurement-comparison = Item { $id } has no eligible measurement comparison.

cli-continue-with-after = Continue with --after { $id }

cli-not-set-up-choose-its-definition-on-the-setup = { $id } not set up: choose its definition on the setup page

cli-restored-order-sending-paused-pending-reconciliation-previous-store = Restored; order sending paused pending reconciliation. Previous store: { $kept }

cli-actor-is-required-for-recovery-commands = --actor is required for recovery commands

cli-dispatched-in-main = dispatched in main

cli-reprocess-must-use-the-recovery-entry-point = reprocess must use the recovery entry point

cli-resolve-must-use-the-recovery-entry-point = resolve must use the recovery entry point

cli-no-machines-add-one-on-the-setup-page = no machines: add one on the setup page

cli-delivery-queued-with-its-original-id-and-bytes = Delivery queued with its original ID and bytes.

cli-backup-verified = Backup verified.

cli-no-log-file-at = No log file at { $path }.

cli-the-windows-service-writes-it-while-it-runs-on = The Windows service writes it while it runs. On Linux the service logs to the system journal:

cli-more-analyte-activity-before = More: analyte activity --before { $next }

cli-raised-for = { $alert } ({ $signal }) · raised { $raised } · for { $responder }

cli-fix-on = { "  " }Fix on: { $page }

cli-command = { "  " }Command: { $command }

cli-oldest-message-kept = Oldest message kept: { $oldest }

cli-more-analyte-data-keeping-after = More: analyte data-keeping --after { $next }

cli-sample-order = Sample { $sample } · order { $order }

cli-more-analyte-orders-cancelled-after = More: analyte orders cancelled --after { $next }

cli-no-earlier-activity-this-is-past-the-oldest-line = No earlier activity: this is past the oldest line Analyte keeps.

cli-no-activity-yet = No activity yet.

cli-no-active-alerts = No active alerts.

cli-not-recorded = not recorded

cli-how-long-messages-are-kept = How long messages are kept

cli-no-machine-is-set-up = { "  No machine is set up." }

cli-not-run-yet = { "  Not run yet" }

cli-no-holds-messages-are-deleted-only-as-kept-above = { "  No holds: messages are deleted only as kept above." }

cli-no-orders-cancelled-by-a-machine = No orders cancelled by a machine.

cli-settings-checked-for-this-version = Settings checked for this version.

cli-exhausted-alert-events-queued-with-original-identities = Exhausted alert events queued with original identities.

cli-retention-hold-updated = Retention hold updated.

cli-no-recordings = { $machine }: no recordings

cli-bytes-last-changed = { $machine } { $file } { $bytes } bytes, last changed { $at }{ $else }

cli-copied-to-it-contains-patient-data-de-identify-a = Copied to { $target }. It contains patient data: de-identify a copy before sharing it.

cli-wrote-values-replaced-distinct-the-original-is-unchanged = Wrote { $path }: { $replaced } values replaced ({ $distinct } distinct). The original is unchanged.

cli-copy-to = copy to

cli-the-copy-exceeds-the-recording-file-bound = the copy exceeds the recording file bound

cli-write-a-reason-it-is-recorded-in-the-history = Write a reason. It is recorded in the history.

cli-keep-the-reason-to-256-characters = Keep the reason to 256 characters.

cli-order-sending-paused-restored-store-requires-machine-app-reconciliation = ORDER SENDING PAUSED: restored store requires machine/app reconciliation.

cli-help-main-cli-config = Program file for operator commands. Without it: the file named by ANALYTE_CONFIG; else analyte.toml in the current folder, if there is one; else the installed program file (Windows: %ProgramData%\Analyte\config\analyte.toml; Linux: /etc/analyte/analyte.toml), if there is one. A file found this way is named on standard error

cli-help-main-cli-actor = Explicit local operator identity, recorded in the audit trail; it may not begin like an admin page sign-in's (`admin-session-`, `windows:`)

cli-help-main-command-retry-alerts = Retries exhausted alert events with their original identities and bytes

cli-help-main-command-captures = Recorded conversations (ADR 0009, ADR 0034): list them, or copy one to a folder. They contain patient data; both are audited

cli-help-main-command-holds = Places or releases an indefinite retention hold

cli-help-main-command-admin = The admin page's key

cli-help-main-command-machines = Lists, adds, sets up, tests and removes machines through the Setup page's own checks; changes apply after a restart

cli-help-main-command-lab-apps = Lists, adds, changes and removes lab apps and stores their keys through the Lab apps page's own checks; changes apply after a restart

cli-help-main-command-orders = Machine orders: list, cancelled by a machine, reconcile, resume after a restore

cli-help-main-command-check-config = Checks a program file and lists each machine's settings version and evidence level

cli-help-main-command-run = Runs the engine in this window until Ctrl+C (the service uses `service run`)

cli-help-main-command-run-shutdown-timeout-secs = Hard process deadline after shutdown begins

cli-help-main-command-service = Entry point for the Windows service; start it with Start-Service Analyte

cli-help-main-command-status = Technical health lines for support: alerts, queues, last data per machine

cli-help-main-command-parked = Needs review: items Analyte kept but could not process. `review` and `parked` are the same command

cli-help-main-command-receipts = Reprocesses one stored message with the current settings

cli-help-main-command-deliveries = Deliveries to lab apps: list, send again

cli-help-main-command-restore = Restores the store from a verified backup; order sending stays paused until reconciled

cli-help-main-command-backup = Writes a verified backup of the store to a file

cli-help-main-command-completions = Prints a script that completes analyte's commands in `shell`. PowerShell: analyte completions powershell | Out-String | Invoke-Expression

cli-help-main-admin-token = Replaces the protected admin credential and prints it once

cli-help-main-parked-compare = Show the protected result comparison and its confirmation snapshot

cli-help-main-parked-suggestions = List missing-sample suggestions for one page of open items

cli-help-main-parked-decide = Confirm a displayed measurement comparison

cli-help-main-parked-list-after = Continue after the last ID of a previous bounded page

cli-about = Faithful analyzer receive service

cli-help-operator-command-resume-after-restore = Releases the restore pause after machines and app have been reconciled

cli-help-operator-command-resume-after-restore-reason = Evidence for the operator decision

cli-help-operator-command-list = Lists one bounded page of machine-order lifecycle snapshots

cli-help-operator-command-list-after = Continue after this local machine-order ID

cli-help-operator-command-list-machine = Restrict the page to one installed machine

cli-help-operator-command-cancelled = The Cancelled by machine page: order items a machine cancelled on its own, to check with the lab and the lab app. Shows each item's sample ID from the lab app's copy, as the page does, never result values; each run is recorded in the audit

cli-help-operator-command-cancelled-after = Continue after this position, as the previous page printed it

cli-help-operator-command-cancelled-json = Print the items as JSON for scripts

cli-help-operator-command-reconcile = Resolves an uncertain machine outcome using operator evidence

cli-help-operator-command-reconcile-id = Local machine-order ID printed by orders list

cli-help-operator-command-reconcile-outcome = Outcome verified by the operator

cli-help-operator-command-reconcile-reason = Required explanation retained in the audit and app event

cli-help-holds-command-receipt = Places or releases a hold on one receipt

cli-help-holds-command-receipt-id = Host receipt UUID

cli-help-holds-command-receipt-release = Release an existing hold instead of placing one

cli-help-holds-command-receipt-reason = Required operator explanation

cli-help-holds-command-order = Places or releases a hold covering an app order and its linked receipts

cli-help-holds-command-order-destination = Destination namespace

cli-help-holds-command-order-id = App order identity

cli-help-captures-command-list = Lists each machine's recordings: file, size and last change. They contain patient data

cli-help-captures-command-list-machine = Only this machine's recordings

cli-help-captures-command-export = Copies one recording to a folder, byte for byte, without overwriting. It contains patient data: de-identify a copy before sharing it

cli-help-captures-command-export-machine = The machine's name

cli-help-captures-command-export-file = The recording's file name, as `captures list` shows it

cli-help-captures-command-export-to = The folder to copy it into; it must exist

cli-help-captures-command-export-reason = Why it is exported; the audit keeps it

cli-help-captures-command-de-identify = Writes a de-identified copy of one recording to a folder: patient fields become synthetic values of the same shape and checksums are recomputed. The original is not changed. Review the copy before sharing it

cli-help-captures-command-de-identify-to = The folder to write the copy into; it must exist

cli-help-captures-command-de-identify-reason = Why it is de-identified; the audit keeps it

cli-help-captures-command-de-identify-checksum-case = The checksum letter case the machine uses (VIDAS); needed only when the recording cannot show it, and then the command says so

cli-help-command-command-overview = The Overview in plain words: "All good", or each thing that needs you with the page and the command that fixes it. Asks the running engine. Exit code: 0 all good, 1 needs a look, 2 a problem stops data, 3 unknown

cli-help-command-command-overview-json = Print the answer as JSON for scripts

cli-help-command-command-activity = The Activity page: messages stored, delivery tries (failed tries in a row as one line), orders and changes, newest first. Lines name sample IDs and counts, never result values; each run is recorded in the audit

cli-help-command-command-activity-limit = How many lines to print (1 to 200)

cli-help-command-command-activity-machine = Only this machine's lines

cli-help-command-command-activity-before = Continue before this position, as the previous page printed it

cli-help-command-command-activity-json = Print the lines as JSON for scripts

cli-help-command-command-alerts = Active alerts, each with its reason and where it is fixed

cli-help-command-command-data-keeping = The Data keeping page: how long each machine's messages are kept, the last clean-up, the oldest message kept and the active holds

cli-help-command-command-data-keeping-after = Continue after this hold number, as the previous page printed it

cli-help-command-command-data-keeping-json = Print the page as JSON for scripts

cli-help-command-command-doctor = A read-only health check for IT: the program file, the engine, each machine's connection, each lab app, disk space and Windows sign-in. Every line says OK, WARN or FAIL and what to do. It changes nothing. Exit code: 0 all OK, 1 a warning, 2 a failure

cli-help-command-command-doctor-json = Print the checks as JSON for scripts

cli-help-command-command-confirm-software = Confirm that the current settings were checked for the reported version

cli-help-command-command-confirm-software-machine = Machine ID from the Machines page

cli-help-command-command-confirm-software-version = Exact software version shown in the warning

cli-help-command-command-logs = Where the engine's log is, and its last lines. The log holds no patient data: Analyte logs names, counts and IDs, never messages or values

cli-help-command-command-logs-tail = How many of the last lines to print (1 to 1000)

cli-help-command-alerts-list = Lists every active alert

cli-help-command-alerts-list-json = Print the alerts as JSON for scripts

cli-help-lab-apps-command-list = Lists the lab apps: address, timing, the machines that use each, and where each key is found (never its value)

cli-help-lab-apps-command-add = Adds a lab app. Its two keys are then asked for without showing them, or read from standard input, one per line: the API key, then the signing key

cli-help-lab-apps-command-add-name = Letters A to Z, digits, - and _ (at most 64); it also names its keys

cli-help-lab-apps-command-set = Changes a lab app's address or timing; values not given stay as they are

cli-help-lab-apps-command-set-name = The lab app's name

cli-help-lab-apps-command-keys = Replaces a lab app's keys, asked for without showing them or read from standard input (the API key, then the signing key, one per line). An empty answer keeps the stored key. Keys are never taken from options

cli-help-lab-apps-command-remove = Removes a lab app that no machine uses and nothing waits for

cli-help-lab-apps-settings-address = Its HTTPS address, such as `https://lab.example.org`

cli-help-lab-apps-settings-freshness-seconds = How long its order list counts as current, in whole seconds

cli-help-lab-apps-settings-request-timeout-seconds = How long to wait for its answer, in whole seconds

cli-help-lab-apps-settings-review-notices = Whether to send it Needs review notices: `yes` or `no` (ADR 0038). A new lab app gets none unless this says yes

cli-help-args-cable-baud = Serial cable: the baud rate: 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 or 115200

cli-help-args-cable-data-bits = Serial cable: data bits, 5 to 8

cli-help-args-cable-parity = Serial cable: parity: none, odd or even

cli-help-args-cable-stop-bits = Serial cable: stop bits, 1 or 2

cli-help-args-cable-flow-control = Serial cable: flow control: none, hardware (RTS/CTS) or software (XON/XOFF)

cli-help-args-choice-model = The machine model: its key as `machines models` lists it, or `installed` to keep the current one

cli-help-args-choice-serial-port = A serial port by name, such as COM3

cli-help-args-choice-usb-serial = A USB serial adapter by its serial number, so it is found in any USB socket

cli-help-args-choice-address = The IP address of a network link (this PC's when the machine connects to Analyte, the machine's when Analyte connects to it), with --port

cli-help-args-choice-port = The network port number, with --address

cli-help-args-choice-folder = The folder Analyte reads results from

cli-help-args-choice-lab-app = The lab app this machine sends results to

cli-help-args-choice-allowed-peer = For a machine that connects to Analyte: accept connections only from this device, the analyzer's own IP address; repeat for several. Any other device is refused (ADR 0026)

cli-help-args-choice-any-device = For a machine that connects to Analyte: accept connections from any device again, removing its allowed devices

cli-help-args-choice-choice = One of the model's own settings, as step 2 lists them under Analyzer settings and `machines models` names them; repeat for several. A setting not given keeps its saved value, else the model's suggestion

cli-help-args-choice-record-only = Record only (ADR 0034): keep the analyzer talking and record every byte, for writing its definition; nothing is sent to a lab app, so no --lab-app. Also for a model not ready yet. Recordings contain patient data: export them with `analyte captures export`

cli-help-args-choice-connect-normally = Leaves record only: connect the machine normally again (needs a ready model and --lab-app). Recorded messages are never delivered

cli-help-args-choice-poll-seconds = Autolyser: seconds between report polls, when the model or capture profile asks

cli-help-args-choice-reports-carry-controls = Record only, Autolyser: whether its reports carry controls: yes or no

cli-help-args-choice-vidas-acknowledgment = Record only, VIDAS: the VIDAS PC acknowledgment: literal or alternate

cli-help-args-choice-analyzer-mode = Record only, for a model that asks it (cobas e 411: elecsys or cobas): the mode the analyzer's own settings show; `machines models` lists it

cli-help-args-choice-repeated-results = Repeated results, for a normal connection: `automatic` (recommended; a resend is kept once, a correction becomes a new revision) or `always_ask` (every repeat waits in Needs review). Not given keeps the saved choice, else automatic (ADR 0035)

cli-help-machines-command-list = Lists the machines in the settings: model, connection, lab app, whether set up, settings version and, from the running engine, the live state

cli-help-machines-command-list-json = Prints JSON for scripts instead of lines

cli-help-machines-command-models = Lists the machine models in the machines folder: ready or not, and how each was checked

cli-help-machines-command-ports = Lists this PC's serial ports with their adapter and USB serial number

cli-help-machines-command-add = Adds a machine to the settings; it is not set up yet. Serial cable settings given here are kept for its setup

cli-help-machines-command-add-name = Letters A to Z, digits, - and _; it also names its settings file

cli-help-machines-command-add-same-analyzer = The name has stored history and this is the same analyzer

cli-help-machines-command-set = Sets a machine up: its model, connection and lab app, checked as the Save step checks them. Choices not given keep their saved values

cli-help-machines-command-set-dry-run = Prints what would be written and writes nothing

cli-help-machines-command-test = Runs the listen-only connection test of step 3 with the saved settings or the choices given: nothing is sent and nothing is stored

cli-help-machines-command-test-seconds = How long to listen, 1 to 60 seconds

cli-help-machines-command-test-show-bytes = Also prints the bytes received (at most 64 KiB). They may contain patient data: never save or share them

cli-help-machines-command-remove = Removes a machine from the settings; its stored results and history stay

cli-help-machines-command-remove-anyway = Removes it even though work is still unfinished

cli-error-reserved = --actor may not begin with "{ $value }": Analyte records that only for people signed in on the admin pages

cli-error-text = --actor: { $value }

cli-named-by-config = named by --config

cli-named-by-analyte-config = named by ANALYTE_CONFIG

cli-in-the-current-folder = in the current folder

cli-the-installed-program-file = the installed program file

cli-log-path = Log: { $path }

cli-choice-write-name-value = write NAME=VALUE, such as protocol_mode=alternate

cli-token-missing = admin token file is missing; run analyte admin token --rotate

cli-token-invalid = admin token file must contain one base64-encoded 256-bit token

cli-token-permissions = admin token file must be a protected regular file (Unix mode 0600)

cli-token-access = admin token file: { $error }

cli-token-random = admin token OS random source failed: { $error }

cli-token-io = admin token { $operation } failed: { $error }

cli-token-op-inspect = inspect credential

cli-token-op-read = read credential

cli-token-op-create-staging = create protected staging file

cli-token-op-write-staging = write staging file

cli-token-op-flush-staging = flush staging file

cli-token-op-install = install credential

cli-token-op-flush-directory = flush credential directory

cli-setup-settings-do-not-load = Analyte's settings do not load: { $error }

cli-setup-store-not-opened = Analyte's stored data could not be opened: { $error }

cli-setup-ports-failed = listing serial ports failed: { $error }

cli-setup-console-failed = the console could not be used: { $error }

cli-setup-no-actor = name who makes this change with --actor <name>; it is recorded in the audit trail

cli-setup-not-confirmed = nothing was saved: it was not confirmed

cli-setup-engine-stopped = the keys can be stored only by the Analyte engine, which is not running ({ $error })

cli-setup-keys-not-sent = the keys were not sent: { $error }

cli-setup-engine-no-answer = whether the engine stored the change is unknown: { $error }

cli-advice-run-as-service-account = Run this command as the account the Analyte service runs as, or set the keys on the Lab apps page.

cli-advice-remove-anyway = To remove it regardless, run the command again with --anyway.

cli-advice-same-analyzer = If it is the same analyzer, run the command again with --same-analyzer.

cli-advice-check-config = See what is wrong with: analyte check-config <file>.

cli-advice-start-engine = Start the engine with Start-Service Analyte in an administrator PowerShell, then run this command again.

cli-advice-check-lab-apps-list = Check with analyte lab-apps list before running the command again.

cli-setup-engine-runs-unknown = What the engine runs is unknown ({ $why }), so every machine in the settings counts as running.

cli-setup-ask-actor = Your name for the audit record

cli-setup-restart-note = { $restart } On Windows run Restart-Service Analyte in an administrator PowerShell, or press Restart now on the Setup page.

cli-fix-this = Fix this: { $reason }

cli-nothing-was-changed = Nothing was changed.

cli-reason-character-cannot-be-recorded = The reason holds a character that cannot be recorded.

cli-captures-patient-data = These recordings contain patient data. Keep them on this PC; before sharing one, make a de-identified copy with captures de-identify and review it (docs/develop/adding-a-machine.md, "Capture, de-identify and replay").

cli-captures-machine-name = a machine name uses only letters, digits, - and _: { $name }

cli-captures-file-name = name one recording as captures list shows it (a file name ending .rec.jsonl), not a path: { $name }

cli-captures-missing = machine { $machine } has no recording called { $file }

cli-captures-no-folder = the folder { $path } does not exist; create it first

cli-captures-exists = { $path } already exists; nothing was overwritten

cli-captures-too-many = machine { $machine } has more than 4096 recordings; move older ones out first

cli-captures-format = the recording cannot be read: { $error }

cli-captures-io = { $operation } { $path } failed: { $error }

cli-captures-op-read = read

cli-captures-op-open = open

cli-captures-op-flush = flush

cli-captures-op-create = create

cli-captures-op-encode = encode

cli-captures-op-write = write

cli-captures-stopped-early = { " " }(capture stopped early: incomplete)

cli-captures-modified-unknown = unknown

cli-deidentify-checklist = Review the copy before sharing; free-text fields were replaced.

cli-deidentify-unsupported = de-identify is not supported for { $protocol } recordings; nothing was written

cli-deidentify-unknown-stack = the recording names a protocol stack Analyte does not know; nothing was written

cli-deidentify-unparsed = event { $event }: { $reason }; Analyte cannot tell where every patient field is, so nothing was written

cli-deidentify-checksum = event { $event }: { $reason }; nothing was written

cli-deidentify-deadline = de-identifying took longer than { $seconds } seconds; nothing was written

cli-deidentify-random = the system random source failed: { $error }

cli-deidentify-limit = a de-identification bound was exceeded; nothing was written

cli-deidentify-checksum-not-hex = the original checksum is not two hex digits

cli-deidentify-checksum-case-unknown = the letter case of this checksum cannot be told from the recording; run again with --checksum-case upper or lower, as the machine's settings say

cli-deidentify-checksum-not-sum = the original checksum is not a three-digit sum modulo 256

cli-deidentify-astm-outside-frames = bytes outside ASTM frames

cli-deidentify-astm-repeat-differs = a repeated ASTM frame differs from the frame it repeats

cli-deidentify-astm-header-delimiters = an ASTM header without four distinct delimiters

cli-deidentify-astm-record-before-header = an ASTM record before its header

cli-deidentify-astm-record-no-delimiter = an ASTM record type without a field delimiter

cli-deidentify-astm-manufacturer-record = a manufacturer (M) record, whose fields only the manufacturer defines

cli-deidentify-astm-unknown-record = an unknown ASTM record type

cli-deidentify-autolyser-host-outside = host bytes outside an Autolyser exchange

cli-deidentify-autolyser-unasked = machine bytes Analyte did not ask for

cli-deidentify-autolyser-short-record = an Autolyser record shorter than its patient code

cli-deidentify-field-escape = a patient field holds an escape sequence that could spell patient text

cli-deidentify-hl7-outside-blocks = bytes outside MLLP blocks

cli-deidentify-hl7-msh-encoding = an MSH without distinct encoding characters

cli-deidentify-hl7-segment-before-msh = an HL7 segment before its MSH

cli-deidentify-hl7-segment-no-separator = an HL7 segment name without a field separator

cli-deidentify-hl7-segment-no-rules = an HL7 segment Analyte has no patient-field rules for

cli-deidentify-vidas-foreign-byte = a byte VIDAS packets never carry

cli-deidentify-vidas-outside-packets = bytes outside VIDAS packets

cli-program-file-not-found = no program file found; looked for { $looked }. Pass --config <file> or set { $variable }

cli-program-file-unreadable = cannot check { $path }: { $error }. The installed program file is readable only by administrators: run the command as administrator (an elevated PowerShell on Windows, sudo on Linux), or pass --config <file>
