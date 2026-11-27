### English source wording.

doctor-overview-free-disk-space = Free disk space

doctor-overview-always-kept-free = Always kept free

doctor-overview-analyte-stops-receiving-below-this = Analyte stops receiving below this

doctor-overview-disk-status = Disk status

doctor-overview-active-alerts = Active alerts

doctor-overview-alert-relays = Alert relays

doctor-overview-relay-status-for-support = Relay status for support

doctor-overview-main-center = main--center

doctor-overview-let-s-connect-your-first-machine = Let’s connect your first machine.

doctor-overview-analyte-reads-results-from-your-lab-s-analyzers-keeps = Analyte reads results from your lab’s analyzers, keeps a copy and sends them to your lab app. Orders go the other way.

doctor-overview-set-up-your-first-machine = Set up your first machine

doctor-overview-the-engine-is-running = The engine is running.

doctor-overview-what-to-do = What to do: { $error }

doctor-overview-fix-on = { "   " }Fix on: { $page } ({ $href })

doctor-overview-command = { "   " }Command: { $command }

doctor-overview-analyte-cannot-say-how-things-are-right-now = Analyte cannot say how things are right now

doctor-overview-problem = PROBLEM

doctor-overview-attention = ATTENTION

doctor-overview-add-an-admin-table-with-a-port-to-the = add an [admin] table with a port to the program file and restart the engine (see docs/operations/operate.md)

doctor-overview-run-this-from-an-administrator-powershell-if-the-key = run this from an administrator PowerShell; if the key file is missing, run analyte admin token --rotate and restart the engine

doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start = start the engine (Start-Service Analyte, or systemctl start analyte on Linux), wait a few seconds and try again; if it does not start, run analyte doctor and analyte logs

doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection = find the program holding the admin port (Get-NetTCPConnection -LocalPort <port>) and stop it, or restart the engine with Restart-Service Analyte if the admin key was rotated since it started

doctor-overview-check-that-this-command-and-the-engine-are-the = check that this command and the engine are the same Analyte version, then run analyte logs

doctor-overview-the-engine-could-not-read-its-state = The engine could not read its state.

doctor-overview-sending-to-lab-app-is-failing-waiting = Sending to lab app { $destination } is failing: { $waiting } waiting

doctor-overview-sending-to-lab-app-is-paused-waiting = Sending to lab app { $destination } is paused: { $waiting } waiting

doctor-overview-analyte-stopped-working-with = Analyte stopped working with { $machine }

doctor-overview-orders-cancelled-by-a-machine = Orders cancelled by a machine: { $cancelled }

doctor-overview-for = { $UNCHECKED-WARNING } for { $machine }

doctor-overview-disk-space-is-low-free = Disk space is low: { $free } free

doctor-overview-check-the-software-version-reported-by = Check the software version reported by { $machine }.

doctor-overview-has-gone-quiet = { $machine } has gone quiet

doctor-overview-has-sent-nothing-for = { $machine } has sent nothing for { $seconds }

doctor-overview-a-result-has-waited-to-be-sent = A result has waited { $seconds } to be sent

doctor-overview-waiting = , { $count } waiting

doctor-overview-cannot-send-to-lab-app = Cannot send to lab app { $destination }: { $why }{ $waiting }

doctor-overview-orders-with-unknown-outcome = Orders with unknown outcome: { $count }

doctor-overview-sending = Sending

doctor-overview-sending-in-line = sending

doctor-overview-disk = Disk

doctor-overview-analyte-review-list = analyte review list

doctor-overview-analyte-actor-you-deliveries-list-reason-why = analyte --actor <you> deliveries list --reason "<why>"

doctor-overview-analyte-orders-list = analyte orders list

doctor-overview-analyte-orders-cancelled = analyte orders cancelled

doctor-overview-analyte-data-keeping = analyte data-keeping

doctor-overview-analyte-actor-you-orders-resume-after-restore-reason-why = analyte --actor <you> orders resume-after-restore --reason "<why>"

doctor-overview-analyte-activity = analyte activity

doctor-overview-analyte-machines-list = analyte machines list

doctor-overview-analyte-lab-apps-list = analyte lab-apps list

doctor-overview-disk-almost-full-analyte-refuses-new-data-from-machines = Disk almost full: Analyte refuses new data from machines

doctor-overview-analyte-could-not-save-a-machine-software-version-warning = Analyte could not save a machine software version warning

doctor-overview-keys-missing = keys missing

doctor-overview-not-set-up-any-more = not set up any more

doctor-overview-items-may-need-review = Items may need review

doctor-overview-bytes-free = { $free } bytes free

doctor-overview-bytes = { $reserve } bytes

doctor-overview-alert = Alert

doctor-overview-responder = Responder

doctor-overview-raised = Raised

doctor-overview-last-value = Last value

doctor-overview-pick-the-machine-s-model = Pick the machine’s model

doctor-overview-choose-how-it-s-connected-and-which-lab-app = Choose how it’s connected and which lab app gets its results

doctor-overview-run-a-listen-only-test-analyte-only-reads-and = Run a listen-only test. Analyte only reads and sends nothing to the machine

doctor-overview-save-then-restart-analyte-to-use-the-new-settings = Save, then restart Analyte to use the new settings

doctor-overview-nothing-needs-you-right-now = Nothing needs you right now.

doctor-overview-analyte-keeps-everything-it-has-received-while-you-fix = Analyte keeps everything it has received while you fix these.

doctor-overview-legend-line-legend-line-wait = legend__line legend__line--wait

doctor-overview-legend-line-legend-line-cut = legend__line legend__line--cut

doctor-overview-link-open = Link open

doctor-overview-waiting-2 = Waiting

doctor-overview-broken = Broken

doctor-overview-enough-space = Enough space

doctor-overview-almost-full-receiving-refused = Almost full: receiving refused

doctor-overview-order-list = Order list · { $name }

doctor-overview-cannot-deliver = Cannot deliver

doctor-overview-sending-paused = Sending paused

doctor-overview-not-delivering = Not delivering

doctor-overview-up-to-date = Up to date

doctor-overview-api-key = API key

doctor-overview-signing-key = signing key

doctor-overview-its-keys-cannot-be-checked = Its keys cannot be checked.

doctor-overview-out-of-date = Out of date

doctor-overview-not-checked-yet = Not checked yet

doctor-overview-items-waiting-to-send = Items waiting to send

doctor-overview-results-and-order-messages = results and order messages

doctor-overview-oldest-waiting = Oldest waiting

doctor-overview-the-check-did-not-finish-within-seconds = the check did not finish within { $as-secs } seconds

doctor-overview-is-valid = { $config } is valid

doctor-overview-cannot-be-used = { $config } cannot be used: { $error }

doctor-overview-what-to-do-2 = { "      " }What to do: { $fix }

doctor-overview-doctor-and = Doctor: { $failures } and { $warnings }.

doctor-overview-the-store-did-not-open-within-seconds = the store did not open within { $as-secs } seconds

doctor-overview-running-version-and-answering = running version { $version } and answering

doctor-overview-running-version-but-this-command-is-version = running version { $version }, but this command is version { $CARGO-PKG-VERSION }

doctor-overview-there-is-no-store-at-yet = there is no store at { $file } yet

doctor-overview-opened-but-did-not-close-cleanly = opened but did not close cleanly: { $error }

doctor-overview-cannot-be-opened = { $data-directory } cannot be opened: { $error }

doctor-overview-free-is-always-kept-free = { $free } free; { $reserve } is always kept free

doctor-overview-free-at-or-below-the-kept-free-analyte-refuses = { $free } free, at or below the { $reserve } kept free: Analyte refuses new data from machines

doctor-overview-the-free-space-could-not-be-read = the free space could not be read: { $error }

doctor-overview-a-usb-serial-number-names-a-network-or-folder = a USB serial number ({ $number }) names a network or folder link

doctor-overview-cable = { $what } cable

doctor-overview-analyzer-settings = { $what } analyzer settings

doctor-overview-report-poll-interval = { $what } report poll interval

doctor-overview-mode = { $what } mode

doctor-overview-every-byte-is-kept-and-recorded-under-captures = { $recording-only }; every byte is kept and recorded under captures/{ $id }

doctor-overview-devices = { $what } devices

doctor-overview-accepts-connections-only-from = accepts connections only from { $allowed }

doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s = run analyte machines set { $id } --allowed-peer <the analyzer's IP address>, or tick Only accept this device in step 4 of its setup

doctor-overview-the-check-for-folder-failed = the check for folder { $path } failed: { $error }

doctor-overview-folder-is-there = folder { $path } is there

doctor-overview-is-not-a-folder = { $path } is not a folder

doctor-overview-folder-cannot-be-read = folder { $path } cannot be read: { $error }

doctor-overview-the-serial-ports-could-not-be-listed = the serial ports could not be listed: { $error }

doctor-overview-serial-port-is-present = serial port { $name } is present

doctor-overview-serial-port-is-not-present = serial port { $name } is not present

doctor-overview-the-adapter-with-serial-number-is-at = the adapter with serial number { $number } is at { $port }

doctor-overview-no-adapter-with-serial-number-is-plugged-in = no adapter with serial number { $number } is plugged in

doctor-overview-adapters-report-serial-number = { $several } adapters report serial number { $number }: { $join }

doctor-overview-nothing-listens-on-although-the-engine-runs = nothing listens on { $address } although the engine runs

doctor-overview-is-free-for-analyte-to-listen-on = { $address } is free for Analyte to listen on

doctor-overview-is-held-by-the-running-engine = { $address } is held by the running engine

doctor-overview-another-program-holds = another program holds { $address }

doctor-overview-is-held-by-the-engine-or-by-another-program = { $address } is held, by the engine or by another program

doctor-overview-cannot-be-used-on-this-computer = { $address } cannot be used on this computer: { $error }

doctor-overview-the-engine-connects-to-analyte-overview-says-if-the = the engine connects to { $address }; analyte overview says if the link is off

doctor-overview-not-tried-the-engine-may-be-connected-to-and = not tried: the engine may be connected to { $address }, and a machine may take one connection at a time

doctor-overview-accepts-a-connection = { $address } accepts a connection

doctor-overview-does-not-accept-a-connection = { $address } does not accept a connection: { $error }

doctor-overview-did-not-answer-within-seconds = { $address } did not answer within { $as-secs } seconds

doctor-overview-lab-app = Lab app { $name }

doctor-overview-when-its-order-list-last-came-could-not-be = when its order list last came could not be read: { $error }

doctor-overview-credential-is-missing = credential { $name } is missing

doctor-overview-its-keys-could-not-be-read-from-this-account = its keys could not be read from this account: { $error }

doctor-overview-resolves-to-no-address = { $host } resolves to no address

doctor-overview-does-not-resolve = { $host } does not resolve: { $error }

doctor-overview-did-not-resolve-within-seconds = { $host } did not resolve within { $as-secs } seconds

doctor-overview-answers-over-https-with-a-valid-certificate-http = { $url } answers over HTTPS with a valid certificate (HTTP { $status })

doctor-overview-gives-no-https-answer = { $url } gives no HTTPS answer: { $error }

doctor-overview-last-complete-order-list = last complete order list { $when }

doctor-overview-its-order-list-is-not-up-to-date-last = its order list is not up to date (last complete list: { $when })

doctor-overview-create-it-with-new-localgroup-name-then-add-people = create it with New-LocalGroup -Name "{ $GROUP }", then add people with Add-LocalGroupMember -Group "{ $GROUP }" -Member <user>

doctor-overview-the-group-exists = the { $GROUP } group exists

doctor-overview-the-group-is-missing-so-only-administrators-can-sign = the { $GROUP } group is missing, so only administrators can sign in with Windows

doctor-overview-is-an-account-but-not-a-local-group = { $GROUP } is an account but not a local group

doctor-overview-the-group-could-not-be-looked-up = the { $GROUP } group could not be looked up: { $error }

doctor-overview-the-rule-is-there = the rule "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) is there

doctor-overview-no-rule-lets-analyzers-connect-to = no rule "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) lets analyzers connect to { $join }

doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended = the firewall rules could not be read (PowerShell ended with { $status })

doctor-overview-the-firewall-rules-could-not-be-read = the firewall rules could not be read: { $error }

doctor-overview-ok = OK

doctor-overview-warn = WARN

doctor-overview-fail = FAIL

doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run = run analyte doctor again; if it stays slow, run analyte logs

doctor-overview-fix-what-it-names-or-choose-the-right-file = fix what it names (or choose the right file with --config), then run analyte doctor again; the other checks need a valid program file

doctor-overview-doctor-everything-checked-is-ok = Doctor: everything checked is OK.

doctor-overview-store = Store

doctor-overview-listed-but-not-set-up-yet-so-it-does = listed but not set up yet, so it does not run

doctor-overview-choose-its-model-on-the-setup-page-save-then = choose its model on the Setup page, save, then restart the engine

doctor-overview-run-the-analyte-exe-the-service-runs-or-update = run the analyte.exe the service runs, or update both to the same version

doctor-overview-run-analyte-overview-for-where-and-how-to-fix = run analyte overview for where and how to fix each

doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run = run analyte doctor again; if it stays unknown, run analyte logs

doctor-overview-start-the-engine-once-start-service-analyte-it-creates = start the engine once (Start-Service Analyte); it creates the store

doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte = run analyte doctor again; if it repeats, run analyte logs

doctor-overview-run-this-from-an-administrator-powershell-if-it-names = run this from an administrator PowerShell; if it names an access list, see docs/operations/install.md

doctor-overview-check-the-machine-s-settings = check the machine's settings

doctor-overview-machine-software = Machine software

doctor-overview-machine = Machine { $id }

doctor-overview-machine-named-software = Machine { $id } software

doctor-overview-check-the-store-and-run-analyte-doctor-again = check the store and run analyte doctor again

doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing = free disk space on this drive; Analyte deletes nothing to make room

doctor-overview-check-the-data-directory-s-drive-in-file-explorer = check the data directory's drive in File Explorer

doctor-overview-choose-the-machine-s-address-or-folder-on-the = choose the machine's address or folder on the Setup page

doctor-overview-accepts-connections-from-any-device-on-the-network-which = accepts connections from any device on the network, which could send results as this machine

doctor-overview-choose-the-machine-s-export-folder-on-the-setup = choose the machine's export folder on the Setup page

doctor-overview-create-the-folder-or-share-it-again-or-choose = create the folder or share it again, or choose another on the Setup page

doctor-overview-check-the-usb-serial-adapter-s-driver-in-device = check the USB serial adapter's driver in Device Manager

doctor-overview-run-analyte-doctor-again = run analyte doctor again

doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the = plug in the USB serial adapter, or choose the right port on the Setup page

doctor-overview-unplug-the-adapter-that-does-not-belong-to-this = unplug the adapter that does not belong to this machine

doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte = run analyte overview: the machine's service in Analyte may have stopped; restart the engine with Restart-Service Analyte

doctor-overview-stop-the-other-program-or-choose-another-port-on = stop the other program, or choose another port on the Setup page and on the machine

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell = turn the admin page on so doctor can tell, or look with Get-NetTCPConnection -LocalPort <port>

doctor-overview-choose-an-address-this-computer-has-on-the-setup = choose an address this computer has on the Setup page

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2 = turn the admin page on so doctor can tell whether the engine runs

doctor-overview-check-that-the-machine-is-on-its-network-settings = check that the machine is on, its network settings and the cable

doctor-overview-the-running-engine-has-its-keys = the running engine has its keys

doctor-overview-its-api-key-and-signing-key-are-there = its API key and signing key are there

doctor-overview-its-keys-were-not-checked = its keys were not checked

doctor-overview-no-order-list-is-kept-for-it = no order list is kept for it

doctor-overview-restart-the-engine = restart the engine

doctor-overview-start-the-engine-and-run-analyte-doctor-again-the = start the engine and run analyte doctor again; the engine checks the keys itself

doctor-overview-its-address-has-no-host-name = its address has no host name

doctor-overview-fix-its-address-on-the-lab-apps-page = fix its address on the Lab apps page

doctor-overview-check-the-lab-app-s-address-on-the-lab = check the lab app's address on the Lab apps page and this computer's DNS

doctor-overview-check-this-computer-s-dns-and-network = check this computer's DNS and network

doctor-overview-check-the-address-the-lab-app-s-server-and = check the address, the lab app's server and its certificate; a certificate this computer does not trust must be fixed on the server

doctor-overview-check-that-the-engine-runs-and-the-lab-app = check that the engine runs and the lab app answers; machines asking for orders get no answer meanwhile

doctor-overview-computername-is-not-set-so-the-group-cannot-be = COMPUTERNAME is not set, so the group cannot be looked up

doctor-overview-run-doctor-from-a-normal-powershell-window = run doctor from a normal PowerShell window

doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to = run the installer again, or allow inbound TCP to these ports from the lab network in Windows Defender Firewall

doctor-overview-check-the-rule-in-windows-defender-firewall = check the rule in Windows Defender Firewall

doctor-overview-reading-the-firewall-rules-took-too-long = reading the firewall rules took too long

doctor-overview-with-an-unknown-outcome-check-the-machine = { $uncertain } with an unknown outcome (check the machine)

doctor-overview-cancelled-by-a-machine = { $cancelled } cancelled by a machine

doctor-overview-analyte-could-not-read-its-store = Analyte could not read its store.

doctor-overview-analyte-could-not-read-whether-order-sending-is-paused = Analyte could not read whether order sending is paused after a restore.

doctor-overview-analyte-could-not-read-the-free-disk-space = Analyte could not read the free disk space.

doctor-overview-analyte-could-not-read-how-many-items-need-review = Analyte could not read how many items need review.

doctor-overview-analyte-could-not-read-when-each-lab-app-last = Analyte could not read when each lab app last accepted a delivery.

doctor-overview-analyte-could-not-read-which-lab-apps-keep-failing = Analyte could not read which lab apps keep failing to accept deliveries.

doctor-overview-analyte-could-not-read-the-active-alerts = Analyte could not read the active alerts.

doctor-overview-analyte-could-not-read-the-machine-orders = Analyte could not read the machine orders.

doctor-overview-analyte-could-not-read-when-each-machine-last-sent = Analyte could not read when each machine last sent data.

doctor-overview-analyte-could-not-read-what-waits-for-each-lab = Analyte could not read what waits for each lab app.

doctor-overview-analyte-could-not-read-what-waits-for-lab-apps = Analyte could not read what waits for lab apps no set-up machine uses.

doctor-overview-analyte-could-not-read-the-alert-relays = Analyte could not read the alert relays.

doctor-overview-analyte-read-everything-but-could-not-write-the-page = Analyte read everything but could not write the page.

doctor-overview-no-orders-with-an-unknown-outcome = No orders with an unknown outcome

doctor-overview-see-machine-orders = See machine orders

doctor-overview-no-orders-cancelled-by-a-machine = No orders cancelled by a machine

doctor-overview-review-cancelled-orders = Review cancelled orders

doctor-overview-a-result-has-waited-too-long-to-be-sent = A result has waited too long to be sent

doctor-overview-machine-has-gone-quiet = Machine has gone quiet

doctor-overview-machine-software-version-changed = Machine software version changed

doctor-overview-disk-space-is-low = Disk space is low

doctor-overview-order-list-is-out-of-date = Order list is out of date

doctor-overview-orders-with-unknown-outcome-2 = Orders with unknown outcome

doctor-overview-items-need-review = Items need review

doctor-overview-results-cannot-be-delivered = Results cannot be delivered

doctor-overview-error-admin-off = the admin pages are switched off in the settings (no [admin] table), so the running engine cannot be asked

doctor-overview-error-not-running = nothing answers on the admin address { $value }: the engine is not running

doctor-overview-error-unreachable = the admin address { $address } cannot be reached: { $source }

doctor-overview-error-timed-out = the engine did not answer within { $as-secs } seconds

doctor-overview-error-key = the admin key cannot be read as this user ({ $value }); run the command as an administrator

doctor-overview-error-key-thread = the admin key read stopped: { $value }

doctor-overview-error-client = the request could not be prepared: { $value }

doctor-overview-error-unproven = something else answers on the admin address: it could not prove it is this Analyte engine, so the admin key was not sent

doctor-overview-error-silent = the engine did not answer: { $value }

doctor-overview-error-status = the engine answered HTTP { $value }

doctor-overview-error-too-large = the engine's answer was too large

doctor-overview-error-unreadable = the engine's answer could not be read: { $value }

doctor-overview-error-request = the request could not be written: { $value }

doctor-overview-error-not-sent = the engine could not be reached: { $value }

doctor-overview-error-timed-out-2 = the engine did not answer the change within { $as-secs } seconds

doctor-overview-signal-path-section = 02 · Signal path

doctor-overview-signal-path-flow = — machines → Analyte → lab apps

doctor-overview-disk-alerts-section = 04 · Disk and alerts

doctor-overview-lab-app-key-advice = enter its keys on the Lab apps page, then restart the engine

doctor-overview-orders-section = 03 · Orders
