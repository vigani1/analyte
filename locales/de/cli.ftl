### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossar für die Prüfung
### sample = Probe; sample ID = Proben-ID; result = Ergebnis; test = Test
### rerun = Wiederholungsmessung; resend = erneute Übertragung; reprocess = neu verarbeiten
### held for review = zur Prüfung zurückgehalten; Needs review = Prüfung erforderlich
### machine = Gerät; analyzer = Analysegerät; lab app = Labor-App; engine = Dienst
### delivery = Übermittlung; order = Auftrag; record only = nur aufzeichnen
### recording = Aufzeichnung; hold = Löschsperre; store = Datenspeicher; program file = Programmdatei
### Befehle, Optionen, Dateinamen und Werte wie `yes`, `automatic` oder `installed` bleiben unverändert.

cli-lang-help = Sprache der Oberfläche (ein BCP-47-Tag). Standard: LC_ALL, LANG oder Englisch

cli-interpretation-recorded-decisions-inspect-parked-items-for-failures = Interpretation { $version } aufgezeichnet; Entscheidungen: { $outcomes }. Prüfen Sie zurückgehaltene Einträge auf Fehler.

cli-using = Verwendet wird { $path } ({ $describe }).

cli-scm-dispatch-was-not-selected = SCM-Dispatch wurde nicht ausgewählt ({ $shutdown-timeout-secs })

cli-resolution = { $id } { $evidence } resolution={ $as-deref }

cli-item-has-no-eligible-measurement-comparison = Eintrag { $id } hat keinen zulässigen Messwertvergleich.

cli-continue-with-after = Weiter mit --after { $id }

cli-not-set-up-choose-its-definition-on-the-setup = { $id } ist nicht eingerichtet: Wählen Sie seine Definition auf der Einrichtungsseite

cli-restored-order-sending-paused-pending-reconciliation-previous-store = Wiederhergestellt; das Senden von Aufträgen ist bis zum Abgleich angehalten. Vorheriger Datenspeicher: { $kept }

cli-actor-is-required-for-recovery-commands = --actor ist für Wiederherstellungsbefehle erforderlich

cli-dispatched-in-main = wird in main verarbeitet

cli-reprocess-must-use-the-recovery-entry-point = reprocess muss den Wiederherstellungs-Einstiegspunkt verwenden

cli-resolve-must-use-the-recovery-entry-point = resolve muss den Wiederherstellungs-Einstiegspunkt verwenden

cli-no-machines-add-one-on-the-setup-page = keine Geräte: Fügen Sie auf der Einrichtungsseite eines hinzu

cli-delivery-queued-with-its-original-id-and-bytes = Übermittlung mit ihrer ursprünglichen ID und ihren ursprünglichen Bytes eingereiht.

cli-backup-verified = Sicherung geprüft.

cli-no-log-file-at = Keine Protokolldatei unter { $path }.

cli-the-windows-service-writes-it-while-it-runs-on = Der Windows-Dienst schreibt sie, während er läuft. Unter Linux protokolliert der Dienst in das Systemjournal:

cli-more-analyte-activity-before = Mehr: analyte activity --before { $next }

cli-raised-for = { $alert } ({ $signal }) · ausgelöst { $raised } · für { $responder }

cli-fix-on = { "  " }Beheben auf: { $page }

cli-command = { "  " }Befehl: { $command }

cli-oldest-message-kept = Älteste aufbewahrte Nachricht: { $oldest }

cli-more-analyte-data-keeping-after = Mehr: analyte data-keeping --after { $next }

cli-sample-order = Probe { $sample } · Auftrag { $order }

cli-more-analyte-orders-cancelled-after = Mehr: analyte orders cancelled --after { $next }

cli-no-earlier-activity-this-is-past-the-oldest-line = Keine frühere Aktivität: Dies liegt vor der ältesten Zeile, die Analyte aufbewahrt.

cli-no-activity-yet = Noch keine Aktivität.

cli-no-active-alerts = Keine aktiven Warnungen.

cli-not-recorded = nicht aufgezeichnet

cli-how-long-messages-are-kept = Wie lange Nachrichten aufbewahrt werden

cli-no-machine-is-set-up = { "  Kein Gerät ist eingerichtet." }

cli-not-run-yet = { "  Noch nicht ausgeführt" }

cli-no-holds-messages-are-deleted-only-as-kept-above = { "  Keine Löschsperren: Nachrichten werden nur gemäß der obigen Aufbewahrung gelöscht." }

cli-no-orders-cancelled-by-a-machine = Keine von einem Gerät stornierten Aufträge.

cli-settings-checked-for-this-version = Einstellungen für diese Version geprüft.

cli-exhausted-alert-events-queued-with-original-identities = Ausgeschöpfte Warnereignisse mit ihren ursprünglichen Identitäten eingereiht.

cli-retention-hold-updated = Löschsperre aktualisiert.

cli-no-recordings = { $machine }: keine Aufzeichnungen

cli-bytes-last-changed = { $machine } { $file } { $bytes } Byte, zuletzt geändert { $at }{ $else }

cli-copied-to-it-contains-patient-data-de-identify-a = Kopiert nach { $target }. Sie enthält Patientendaten: Anonymisieren Sie eine Kopie, bevor Sie sie weitergeben.

cli-wrote-values-replaced-distinct-the-original-is-unchanged = { $path } geschrieben: { $replaced } Werte ersetzt ({ $distinct } verschiedene). Das Original bleibt unverändert.

cli-copy-to = kopieren nach

cli-the-copy-exceeds-the-recording-file-bound = die Kopie überschreitet die Größengrenze für Aufzeichnungsdateien

cli-write-a-reason-it-is-recorded-in-the-history = Geben Sie einen Grund an. Er wird im Verlauf festgehalten.

cli-keep-the-reason-to-256-characters = Der Grund darf höchstens 256 Zeichen lang sein.

cli-order-sending-paused-restored-store-requires-machine-app-reconciliation = SENDEN VON AUFTRÄGEN ANGEHALTEN: Der wiederhergestellte Datenspeicher erfordert einen Abgleich zwischen Gerät und Labor-App.

cli-help-main-cli-config = Programmdatei für Bedienbefehle. Ohne Angabe: die in ANALYTE_CONFIG genannte Datei; sonst analyte.toml im aktuellen Ordner, falls vorhanden; sonst die installierte Programmdatei (Windows: %ProgramData%\Analyte\config\analyte.toml; Linux: /etc/analyte/analyte.toml), falls vorhanden. Eine so gefundene Datei wird in der Standardfehlerausgabe genannt

cli-help-main-cli-actor = Ausdrückliche lokale Identität der bedienenden Person, im Prüfprotokoll festgehalten; sie darf nicht wie eine Anmeldung an den Verwaltungsseiten beginnen (`admin-session-`, `windows:`)

cli-help-main-command-retry-alerts = Wiederholt ausgeschöpfte Warnereignisse mit ihren ursprünglichen Identitäten und Bytes

cli-help-main-command-captures = Aufgezeichnete Kommunikation (ADR 0009, ADR 0034): auflisten oder eine Aufzeichnung in einen Ordner kopieren. Sie enthalten Patientendaten; beides wird im Prüfprotokoll festgehalten

cli-help-main-command-holds = Setzt oder hebt eine unbefristete Löschsperre auf

cli-help-main-command-admin = Der Schlüssel der Verwaltungsseiten

cli-help-main-command-machines = Listet, fügt hinzu, richtet ein, testet und entfernt Geräte mit denselben Prüfungen wie die Einrichtungsseite; Änderungen gelten nach einem Neustart

cli-help-main-command-lab-apps = Listet, fügt hinzu, ändert und entfernt Labor-Apps und speichert ihre Schlüssel mit denselben Prüfungen wie die Seite „Labor-Apps“; Änderungen gelten nach einem Neustart

cli-help-main-command-orders = Geräteaufträge: auflisten, von einem Gerät storniert, abgleichen, nach einer Wiederherstellung fortsetzen

cli-help-main-command-check-config = Prüft eine Programmdatei und listet für jedes Gerät die Einstellungsversion und die Nachweisstufe

cli-help-main-command-run = Führt den Dienst in diesem Fenster aus, bis Strg+C gedrückt wird (der Windows-Dienst verwendet `service run`)

cli-help-main-command-run-shutdown-timeout-secs = Feste Frist für den Prozess, nachdem das Herunterfahren begonnen hat

cli-help-main-command-service = Einstiegspunkt für den Windows-Dienst; starten Sie ihn mit Start-Service Analyte

cli-help-main-command-status = Technische Zustandszeilen für den Support: Warnungen, Warteschlangen, letzte Daten je Gerät

cli-help-main-command-parked = Prüfung erforderlich: Einträge, die Analyte aufbewahrt, aber nicht verarbeiten konnte. `review` und `parked` sind derselbe Befehl

cli-help-main-command-receipts = Verarbeitet eine gespeicherte Nachricht mit den aktuellen Einstellungen neu

cli-help-main-command-deliveries = Übermittlungen an Labor-Apps: auflisten, erneut senden

cli-help-main-command-restore = Stellt den Datenspeicher aus einer geprüften Sicherung wieder her; das Senden von Aufträgen bleibt bis zum Abgleich angehalten

cli-help-main-command-backup = Schreibt eine geprüfte Sicherung des Datenspeichers in eine Datei

cli-help-main-command-completions = Gibt ein Skript aus, das die Befehle von analyte in `shell` vervollständigt. PowerShell: analyte completions powershell | Out-String | Invoke-Expression

cli-help-main-admin-token = Ersetzt die geschützten Zugangsdaten der Verwaltungsseiten und gibt sie einmal aus

cli-help-main-parked-compare = Zeigt den geschützten Ergebnisvergleich und seinen Bestätigungsstand

cli-help-main-parked-suggestions = Listet Vorschläge für fehlende Proben für eine Seite offener Einträge

cli-help-main-parked-decide = Bestätigt einen angezeigten Messwertvergleich

cli-help-main-parked-list-after = Weiter nach der letzten ID einer vorherigen begrenzten Seite

cli-about = Zuverlässiger Empfangsdienst für Analysegeräte

cli-help-operator-command-resume-after-restore = Hebt die Pause nach einer Wiederherstellung auf, nachdem Geräte und Labor-App abgeglichen wurden

cli-help-operator-command-resume-after-restore-reason = Nachweis für die Entscheidung der bedienenden Person

cli-help-operator-command-list = Listet eine begrenzte Seite mit dem Lebenszyklusstand von Geräteaufträgen

cli-help-operator-command-list-after = Weiter nach dieser lokalen Geräteauftrags-ID

cli-help-operator-command-list-machine = Beschränkt die Seite auf ein installiertes Gerät

cli-help-operator-command-cancelled = Die Seite „Vom Gerät storniert“: Auftragspositionen, die ein Gerät von sich aus storniert hat, zur Klärung mit dem Labor und der Labor-App. Zeigt wie die Seite die Proben-ID jeder Position aus der Kopie der Labor-App, niemals Ergebniswerte; jeder Aufruf wird im Prüfprotokoll festgehalten

cli-help-operator-command-cancelled-after = Weiter nach dieser Position, wie sie die vorherige Seite ausgegeben hat

cli-help-operator-command-cancelled-json = Gibt die Einträge als JSON für Skripte aus

cli-help-operator-command-reconcile = Klärt einen unsicheren Ausgang am Gerät anhand eines Nachweises der bedienenden Person

cli-help-operator-command-reconcile-id = Lokale Geräteauftrags-ID, wie sie orders list ausgibt

cli-help-operator-command-reconcile-outcome = Von der bedienenden Person bestätigter Ausgang

cli-help-operator-command-reconcile-reason = Erforderliche Begründung, die im Prüfprotokoll und im App-Ereignis erhalten bleibt

cli-help-holds-command-receipt = Setzt oder hebt eine Löschsperre für einen Empfang auf

cli-help-holds-command-receipt-id = UUID des Empfangs auf diesem Rechner

cli-help-holds-command-receipt-release = Hebt eine bestehende Löschsperre auf, statt eine zu setzen

cli-help-holds-command-receipt-reason = Erforderliche Begründung der bedienenden Person

cli-help-holds-command-order = Setzt oder hebt eine Löschsperre für einen App-Auftrag und seine verknüpften Empfänge auf

cli-help-holds-command-order-destination = Namensraum des Ziels

cli-help-holds-command-order-id = Identität des App-Auftrags

cli-help-captures-command-list = Listet die Aufzeichnungen jedes Geräts: Datei, Größe und letzte Änderung. Sie enthalten Patientendaten

cli-help-captures-command-list-machine = Nur die Aufzeichnungen dieses Geräts

cli-help-captures-command-export = Kopiert eine Aufzeichnung Byte für Byte in einen Ordner, ohne zu überschreiben. Sie enthält Patientendaten: Anonymisieren Sie eine Kopie, bevor Sie sie weitergeben

cli-help-captures-command-export-machine = Der Name des Geräts

cli-help-captures-command-export-file = Der Dateiname der Aufzeichnung, wie `captures list` ihn anzeigt

cli-help-captures-command-export-to = Der Ordner, in den kopiert wird; er muss vorhanden sein

cli-help-captures-command-export-reason = Warum exportiert wird; das Prüfprotokoll hält es fest

cli-help-captures-command-de-identify = Schreibt eine anonymisierte Kopie einer Aufzeichnung in einen Ordner: Patientenfelder werden zu künstlichen Werten gleicher Form, und Prüfsummen werden neu berechnet. Das Original wird nicht verändert. Prüfen Sie die Kopie, bevor Sie sie weitergeben

cli-help-captures-command-de-identify-to = Der Ordner, in den die Kopie geschrieben wird; er muss vorhanden sein

cli-help-captures-command-de-identify-reason = Warum anonymisiert wird; das Prüfprotokoll hält es fest

cli-help-captures-command-de-identify-checksum-case = Die Groß- oder Kleinschreibung der Prüfsumme, die das Gerät verwendet (VIDAS); nur nötig, wenn die Aufzeichnung sie nicht erkennen lässt, und der Befehl sagt dann Bescheid

cli-help-command-command-overview = Die Übersicht in einfachen Worten: „Alles in Ordnung“ oder jede Sache, die Ihre Aufmerksamkeit erfordert, mit der Seite und dem Befehl, der sie behebt. Fragt den laufenden Dienst. Exit-Code: 0 alles in Ordnung, 1 bitte ansehen, 2 ein Problem hält Daten auf, 3 unbekannt

cli-help-command-command-overview-json = Gibt die Antwort als JSON für Skripte aus

cli-help-command-command-activity = Die Seite „Aktivität“: gespeicherte Nachrichten, Übermittlungsversuche (aufeinanderfolgende Fehlversuche als eine Zeile), Aufträge und Änderungen, neueste zuerst. Die Zeilen nennen Proben-IDs und Anzahlen, niemals Ergebniswerte; jeder Aufruf wird im Prüfprotokoll festgehalten

cli-help-command-command-activity-limit = Wie viele Zeilen ausgegeben werden (1 bis 200)

cli-help-command-command-activity-machine = Nur die Zeilen dieses Geräts

cli-help-command-command-activity-before = Weiter vor dieser Position, wie sie die vorherige Seite ausgegeben hat

cli-help-command-command-activity-json = Gibt die Zeilen als JSON für Skripte aus

cli-help-command-command-alerts = Aktive Warnungen, jeweils mit Grund und Ort der Behebung

cli-help-command-command-data-keeping = Die Seite „Datenaufbewahrung“: wie lange die Nachrichten jedes Geräts aufbewahrt werden, die letzte Bereinigung, die älteste aufbewahrte Nachricht und die aktiven Löschsperren

cli-help-command-command-data-keeping-after = Weiter nach dieser Löschsperren-Nummer, wie sie die vorherige Seite ausgegeben hat

cli-help-command-command-data-keeping-json = Gibt die Seite als JSON für Skripte aus

cli-help-command-command-doctor = Eine schreibgeschützte Zustandsprüfung für die IT: die Programmdatei, der Dienst, die Verbindung jedes Geräts, jede Labor-App, der Speicherplatz und die Windows-Anmeldung. Jede Zeile sagt OK, WARN oder FAIL und was zu tun ist. Es wird nichts geändert. Exit-Code: 0 alles OK, 1 eine Warnung, 2 ein Fehler

cli-help-command-command-doctor-json = Gibt die Prüfungen als JSON für Skripte aus

cli-help-command-command-confirm-software = Bestätigt, dass die aktuellen Einstellungen für die gemeldete Version geprüft wurden

cli-help-command-command-confirm-software-machine = Geräte-ID von der Seite „Geräte“

cli-help-command-command-confirm-software-version = Genaue Softwareversion, wie in der Warnung angezeigt

cli-help-command-command-logs = Wo das Protokoll des Dienstes liegt, und seine letzten Zeilen. Das Protokoll enthält keine Patientendaten: Analyte protokolliert Namen, Anzahlen und IDs, niemals Nachrichten oder Werte

cli-help-command-command-logs-tail = Wie viele der letzten Zeilen ausgegeben werden (1 bis 1000)

cli-help-command-alerts-list = Listet alle aktiven Warnungen

cli-help-command-alerts-list-json = Gibt die Warnungen als JSON für Skripte aus

cli-help-lab-apps-command-list = Listet die Labor-Apps: Adresse, Zeitvorgaben, die Geräte, die sie jeweils verwenden, und wo jeder Schlüssel liegt (niemals sein Wert)

cli-help-lab-apps-command-add = Fügt eine Labor-App hinzu. Ihre beiden Schlüssel werden danach verdeckt abgefragt oder von der Standardeingabe gelesen, einer pro Zeile: der API-Schlüssel, dann der Signaturschlüssel

cli-help-lab-apps-command-add-name = Buchstaben A bis Z, Ziffern, - und _ (höchstens 64); der Name benennt auch ihre Schlüssel

cli-help-lab-apps-command-set = Ändert die Adresse oder die Zeitvorgaben einer Labor-App; nicht angegebene Werte bleiben unverändert

cli-help-lab-apps-command-set-name = Der Name der Labor-App

cli-help-lab-apps-command-keys = Ersetzt die Schlüssel einer Labor-App, verdeckt abgefragt oder von der Standardeingabe gelesen (der API-Schlüssel, dann der Signaturschlüssel, einer pro Zeile). Eine leere Antwort behält den gespeicherten Schlüssel. Schlüssel werden niemals aus Optionen übernommen

cli-help-lab-apps-command-remove = Entfernt eine Labor-App, die kein Gerät verwendet und auf die nichts wartet

cli-help-lab-apps-settings-address = Ihre HTTPS-Adresse, zum Beispiel `https://lab.example.org`

cli-help-lab-apps-settings-freshness-seconds = Wie lange ihre Auftragsliste als aktuell gilt, in ganzen Sekunden

cli-help-lab-apps-settings-request-timeout-seconds = Wie lange auf ihre Antwort gewartet wird, in ganzen Sekunden

cli-help-lab-apps-settings-review-notices = Ob ihr Hinweise zu „Prüfung erforderlich“ gesendet werden: `yes` oder `no` (ADR 0038). Eine neue Labor-App erhält keine, außer hier steht yes

cli-help-args-cable-baud = Serielles Kabel: die Baudrate: 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 oder 115200

cli-help-args-cable-data-bits = Serielles Kabel: Datenbits, 5 bis 8

cli-help-args-cable-parity = Serielles Kabel: Parität: none, odd oder even

cli-help-args-cable-stop-bits = Serielles Kabel: Stoppbits, 1 oder 2

cli-help-args-cable-flow-control = Serielles Kabel: Flusssteuerung: none, hardware (RTS/CTS) oder software (XON/XOFF)

cli-help-args-choice-model = Das Gerätemodell: sein Schlüssel, wie `machines models` ihn listet, oder `installed`, um das aktuelle zu behalten

cli-help-args-choice-serial-port = Eine serielle Schnittstelle nach Namen, zum Beispiel COM3

cli-help-args-choice-usb-serial = Ein USB-Seriell-Adapter nach seiner Seriennummer, damit er an jedem USB-Anschluss gefunden wird

cli-help-args-choice-address = Die IP-Adresse einer Netzwerkverbindung (die dieses PCs, wenn das Gerät sich mit Analyte verbindet, die des Geräts, wenn Analyte sich mit ihm verbindet), zusammen mit --port

cli-help-args-choice-port = Die Netzwerk-Portnummer, zusammen mit --address

cli-help-args-choice-folder = Der Ordner, aus dem Analyte Ergebnisse liest

cli-help-args-choice-lab-app = Die Labor-App, an die dieses Gerät Ergebnisse sendet

cli-help-args-choice-allowed-peer = Für ein Gerät, das sich mit Analyte verbindet: nur Verbindungen von diesem Gerät annehmen, der eigenen IP-Adresse des Analysegeräts; für mehrere wiederholen. Jedes andere Gerät wird abgelehnt (ADR 0026)

cli-help-args-choice-any-device = Für ein Gerät, das sich mit Analyte verbindet: wieder Verbindungen von jedem Gerät annehmen und die erlaubten Geräte entfernen

cli-help-args-choice-choice = Eine der eigenen Einstellungen des Modells, wie Schritt 2 sie unter „Einstellungen am Analysegerät“ listet und `machines models` sie benennt; für mehrere wiederholen. Eine nicht angegebene Einstellung behält ihren gespeicherten Wert, sonst den Vorschlag des Modells

cli-help-args-choice-record-only = Nur aufzeichnen (ADR 0034): die Kommunikation mit dem Analysegerät aufrechterhalten und jedes Byte aufzeichnen, um seine Definition zu schreiben; nichts wird an eine Labor-App gesendet, daher kein --lab-app. Auch für ein noch nicht einsatzbereites Modell. Aufzeichnungen enthalten Patientendaten: Exportieren Sie sie mit `analyte captures export`

cli-help-args-choice-connect-normally = Beendet „nur aufzeichnen“: das Gerät wieder normal verbinden (erfordert ein einsatzbereites Modell und --lab-app). Aufgezeichnete Nachrichten werden niemals übermittelt

cli-help-args-choice-poll-seconds = Autolyser: Sekunden zwischen Berichtsabfragen, wenn das Modell oder das Aufzeichnungsprofil es verlangt

cli-help-args-choice-reports-carry-controls = Nur aufzeichnen, Autolyser: ob seine Berichte Kontrollen enthalten: yes oder no

cli-help-args-choice-vidas-acknowledgment = Nur aufzeichnen, VIDAS: die Bestätigung des VIDAS-PCs: literal oder alternate

cli-help-args-choice-analyzer-mode = Nur aufzeichnen, für ein Modell, das danach fragt (cobas e 411: elecsys oder cobas): der Modus, den die eigenen Einstellungen des Analysegeräts zeigen; `machines models` listet ihn

cli-help-args-choice-repeated-results = Wiederholte Ergebnisse bei normaler Verbindung: `automatic` (empfohlen; eine erneute Übertragung wird einmal behalten, eine Korrektur wird zu einer neuen Revision) oder `always_ask` (jede Wiederholung wartet unter „Prüfung erforderlich“). Ohne Angabe bleibt die gespeicherte Wahl, sonst automatic (ADR 0035)

cli-help-machines-command-list = Listet die Geräte in den Einstellungen: Modell, Verbindung, Labor-App, ob eingerichtet, Einstellungsversion und, vom laufenden Dienst, den aktuellen Zustand

cli-help-machines-command-list-json = Gibt JSON für Skripte statt Zeilen aus

cli-help-machines-command-models = Listet die Gerätemodelle im Geräteordner: einsatzbereit oder nicht, und wie jedes geprüft wurde

cli-help-machines-command-ports = Listet die seriellen Schnittstellen dieses PCs mit Adapter und USB-Seriennummer

cli-help-machines-command-add = Fügt den Einstellungen ein Gerät hinzu; es ist noch nicht eingerichtet. Hier angegebene Einstellungen für das serielle Kabel werden für seine Einrichtung behalten

cli-help-machines-command-add-name = Buchstaben A bis Z, Ziffern, - und _; der Name benennt auch seine Einstellungsdatei

cli-help-machines-command-add-same-analyzer = Der Name hat einen gespeicherten Verlauf, und es ist dasselbe Analysegerät

cli-help-machines-command-set = Richtet ein Gerät ein: Modell, Verbindung und Labor-App, geprüft wie im Schritt „Speichern“. Nicht angegebene Auswahlen behalten ihre gespeicherten Werte

cli-help-machines-command-set-dry-run = Gibt aus, was geschrieben würde, und schreibt nichts

cli-help-machines-command-test = Führt den reinen Mithör-Verbindungstest aus Schritt 3 mit den gespeicherten Einstellungen oder den angegebenen Auswahlen aus: nichts wird gesendet und nichts gespeichert

cli-help-machines-command-test-seconds = Wie lange mitgehört wird, 1 bis 60 Sekunden

cli-help-machines-command-test-show-bytes = Gibt auch die empfangenen Bytes aus (höchstens 64 KiB). Sie können Patientendaten enthalten: Speichern oder teilen Sie sie niemals

cli-help-machines-command-remove = Entfernt ein Gerät aus den Einstellungen; seine gespeicherten Ergebnisse und sein Verlauf bleiben erhalten

cli-help-machines-command-remove-anyway = Entfernt es, obwohl noch Arbeit offen ist

cli-error-reserved = --actor darf nicht mit „{ $value }“ beginnen: Analyte verwendet dies nur für Personen, die an den Verwaltungsseiten angemeldet sind

cli-error-text = --actor: { $value }

cli-named-by-config = genannt durch --config

cli-named-by-analyte-config = genannt durch ANALYTE_CONFIG

cli-in-the-current-folder = im aktuellen Ordner

cli-the-installed-program-file = die installierte Programmdatei

cli-log-path = Protokoll: { $path }

cli-choice-write-name-value = schreiben Sie NAME=WERT, zum Beispiel protocol_mode=alternate

cli-token-missing = die Admin-Token-Datei fehlt; führen Sie analyte admin token --rotate aus

cli-token-invalid = die Admin-Token-Datei muss genau ein base64-kodiertes 256-Bit-Token enthalten

cli-token-permissions = die Admin-Token-Datei muss eine geschützte reguläre Datei sein (Unix-Modus 0600)

cli-token-access = Admin-Token-Datei: { $error }

cli-token-random = die Zufallsquelle des Betriebssystems für das Admin-Token ist ausgefallen: { $error }

cli-token-io = Admin-Token: { $operation } fehlgeschlagen: { $error }

cli-token-op-inspect = Zugangsdaten prüfen

cli-token-op-read = Zugangsdaten lesen

cli-token-op-create-staging = geschützte Zwischendatei anlegen

cli-token-op-write-staging = Zwischendatei schreiben

cli-token-op-flush-staging = Zwischendatei auf den Datenträger schreiben

cli-token-op-install = Zugangsdaten installieren

cli-token-op-flush-directory = Ordner der Zugangsdaten auf den Datenträger schreiben

cli-setup-settings-do-not-load = Die Einstellungen von Analyte lassen sich nicht laden: { $error }

cli-setup-store-not-opened = Die gespeicherten Daten von Analyte konnten nicht geöffnet werden: { $error }

cli-setup-ports-failed = das Auflisten der seriellen Schnittstellen ist fehlgeschlagen: { $error }

cli-setup-console-failed = die Konsole konnte nicht verwendet werden: { $error }

cli-setup-no-actor = geben Sie mit --actor <Name> an, wer diese Änderung vornimmt; dies wird im Prüfprotokoll festgehalten

cli-setup-not-confirmed = nichts wurde gespeichert: es wurde nicht bestätigt

cli-setup-engine-stopped = die Schlüssel kann nur der Analyte-Dienst speichern, und dieser läuft nicht ({ $error })

cli-setup-keys-not-sent = die Schlüssel wurden nicht gesendet: { $error }

cli-setup-engine-no-answer = ob der Dienst die Änderung gespeichert hat, ist unbekannt: { $error }

cli-advice-run-as-service-account = Führen Sie diesen Befehl unter dem Konto aus, unter dem der Analyte-Dienst läuft, oder setzen Sie die Schlüssel auf der Seite „Labor-Apps“.

cli-advice-remove-anyway = Um es trotzdem zu entfernen, führen Sie den Befehl erneut mit --anyway aus.

cli-advice-same-analyzer = Wenn es dasselbe Analysegerät ist, führen Sie den Befehl erneut mit --same-analyzer aus.

cli-advice-check-config = Sehen Sie nach, was falsch ist, mit: analyte check-config <Datei>.

cli-advice-start-engine = Starten Sie den Dienst mit Start-Service Analyte in einer PowerShell mit Administratorrechten und führen Sie diesen Befehl dann erneut aus.

cli-advice-check-lab-apps-list = Prüfen Sie mit analyte lab-apps list, bevor Sie den Befehl erneut ausführen.

cli-setup-engine-runs-unknown = Was der Dienst ausführt, ist unbekannt ({ $why }), daher gilt jedes Gerät in den Einstellungen als laufend.

cli-setup-ask-actor = Ihr Name für das Prüfprotokoll

cli-setup-restart-note = { $restart } Unter Windows führen Sie Restart-Service Analyte in einer PowerShell mit Administratorrechten aus oder klicken auf der Einrichtungsseite auf „Jetzt neu starten“.

cli-fix-this = Bitte beheben: { $reason }

cli-nothing-was-changed = Es wurde nichts geändert.

cli-reason-character-cannot-be-recorded = Der Grund enthält ein Zeichen, das nicht festgehalten werden kann.

cli-captures-patient-data = Diese Aufzeichnungen enthalten Patientendaten. Bewahren Sie sie auf diesem PC auf; bevor Sie eine weitergeben, erstellen Sie mit captures de-identify eine anonymisierte Kopie und prüfen Sie sie (docs/develop/adding-a-machine.md, „Capture, de-identify and replay“).

cli-captures-machine-name = ein Gerätename verwendet nur Buchstaben, Ziffern, - und _: { $name }

cli-captures-file-name = geben Sie eine Aufzeichnung so an, wie captures list sie anzeigt (ein Dateiname mit der Endung .rec.jsonl), nicht als Pfad: { $name }

cli-captures-missing = Gerät { $machine } hat keine Aufzeichnung namens { $file }

cli-captures-no-folder = der Ordner { $path } existiert nicht; legen Sie ihn zuerst an

cli-captures-exists = { $path } existiert bereits; es wurde nichts überschrieben

cli-captures-too-many = Gerät { $machine } hat mehr als 4096 Aufzeichnungen; verschieben Sie zuerst ältere

cli-captures-format = die Aufzeichnung kann nicht gelesen werden: { $error }

cli-captures-io = { $path }: { $operation } fehlgeschlagen: { $error }

cli-captures-op-read = Lesen

cli-captures-op-open = Öffnen

cli-captures-op-flush = Schreiben auf den Datenträger

cli-captures-op-create = Anlegen

cli-captures-op-encode = Kodieren

cli-captures-op-write = Schreiben

cli-captures-stopped-early = { " " }(Aufzeichnung vorzeitig beendet: unvollständig)

cli-captures-modified-unknown = unbekannt

cli-deidentify-checklist = Prüfen Sie die Kopie, bevor Sie sie weitergeben; Freitextfelder wurden ersetzt.

cli-deidentify-unsupported = Anonymisierung wird für { $protocol }-Aufzeichnungen nicht unterstützt; es wurde nichts geschrieben

cli-deidentify-unknown-stack = die Aufzeichnung nennt einen Protokollstapel, den Analyte nicht kennt; es wurde nichts geschrieben

cli-deidentify-unparsed = Ereignis { $event }: { $reason }; Analyte kann nicht erkennen, wo sich jedes Patientenfeld befindet, daher wurde nichts geschrieben

cli-deidentify-checksum = Ereignis { $event }: { $reason }; es wurde nichts geschrieben

cli-deidentify-deadline = die Anonymisierung dauerte länger als { $seconds } Sekunden; es wurde nichts geschrieben

cli-deidentify-random = die Zufallsquelle des Systems ist ausgefallen: { $error }

cli-deidentify-limit = eine Grenze der Anonymisierung wurde überschritten; es wurde nichts geschrieben

cli-deidentify-checksum-not-hex = die ursprüngliche Prüfsumme besteht nicht aus zwei Hexadezimalziffern

cli-deidentify-checksum-case-unknown = die Groß- oder Kleinschreibung dieser Prüfsumme lässt sich aus der Aufzeichnung nicht erkennen; führen Sie den Befehl erneut mit --checksum-case upper oder lower aus, wie es die Einstellungen des Geräts angeben

cli-deidentify-checksum-not-sum = die ursprüngliche Prüfsumme ist keine dreistellige Summe modulo 256

cli-deidentify-astm-outside-frames = Bytes außerhalb von ASTM-Frames

cli-deidentify-astm-repeat-differs = ein wiederholter ASTM-Frame unterscheidet sich von dem Frame, den er wiederholt

cli-deidentify-astm-header-delimiters = ein ASTM-Header ohne vier verschiedene Trennzeichen

cli-deidentify-astm-record-before-header = ein ASTM-Datensatz vor seinem Header

cli-deidentify-astm-record-no-delimiter = ein ASTM-Datensatztyp ohne Feldtrennzeichen

cli-deidentify-astm-manufacturer-record = ein Herstellerdatensatz (M), dessen Felder nur der Hersteller festlegt

cli-deidentify-astm-unknown-record = ein unbekannter ASTM-Datensatztyp

cli-deidentify-autolyser-host-outside = Host-Bytes außerhalb eines Autolyser-Austauschs

cli-deidentify-autolyser-unasked = Gerätebytes, die Analyte nicht angefordert hat

cli-deidentify-autolyser-short-record = ein Autolyser-Datensatz, der kürzer als sein Patientencode ist

cli-deidentify-field-escape = ein Patientenfeld enthält eine Escape-Sequenz, die Patiententext darstellen könnte

cli-deidentify-hl7-outside-blocks = Bytes außerhalb von MLLP-Blöcken

cli-deidentify-hl7-msh-encoding = ein MSH ohne verschiedene Kodierungszeichen

cli-deidentify-hl7-segment-before-msh = ein HL7-Segment vor seinem MSH

cli-deidentify-hl7-segment-no-separator = ein HL7-Segmentname ohne Feldtrennzeichen

cli-deidentify-hl7-segment-no-rules = ein HL7-Segment, für das Analyte keine Regeln zu Patientenfeldern hat

cli-deidentify-vidas-foreign-byte = ein Byte, das VIDAS-Pakete niemals enthalten

cli-deidentify-vidas-outside-packets = Bytes außerhalb von VIDAS-Paketen

cli-program-file-not-found = keine Programmdatei gefunden; gesucht wurde { $looked }. Geben Sie --config <Datei> an oder setzen Sie { $variable }

cli-program-file-unreadable = { $path } kann nicht geprüft werden: { $error }. Die installierte Programmdatei ist nur für Administratoren lesbar: Führen Sie den Befehl als Administrator aus (eine PowerShell mit erhöhten Rechten unter Windows, sudo unter Linux), oder geben Sie --config <Datei> an
