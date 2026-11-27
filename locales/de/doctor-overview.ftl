### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossar für die Prüfung
### sample = Probe; result = Ergebnis; test = Test; resend = erneute Übertragung
### held for review = zur Prüfung zurückgehalten; Needs review = Prüfung erforderlich
### machine = Gerät; analyzer = Analysegerät; lab app = Labor-App; engine = Dienst
### delivery = Übermittlung; order = Auftrag; store = Datenspeicher; program file = Programmdatei
### admin pages = Verwaltungsseiten; Setup = Einrichtung; listen-only test = Mithörtest
### OK, WARN, FAIL, Befehle, CSS-Klassen und PowerShell-Befehle bleiben unverändert.

doctor-overview-free-disk-space = Freier Speicherplatz

doctor-overview-always-kept-free = Immer frei gehalten

doctor-overview-analyte-stops-receiving-below-this = Darunter hört Analyte auf zu empfangen

doctor-overview-disk-status = Zustand des Datenträgers

doctor-overview-active-alerts = Aktive Warnungen

doctor-overview-alert-relays = Warnungsweiterleitungen

doctor-overview-relay-status-for-support = Weiterleitungsstatus für den Support

doctor-overview-main-center = main--center

doctor-overview-let-s-connect-your-first-machine = Verbinden wir Ihr erstes Gerät.

doctor-overview-analyte-reads-results-from-your-lab-s-analyzers-keeps = Analyte liest Ergebnisse von den Analysegeräten Ihres Labors, bewahrt eine Kopie auf und sendet sie an Ihre Labor-App. Aufträge gehen den umgekehrten Weg.

doctor-overview-set-up-your-first-machine = Erstes Gerät einrichten

doctor-overview-the-engine-is-running = Der Dienst läuft.

doctor-overview-what-to-do = Was zu tun ist: { $error }

doctor-overview-fix-on = { "   " }Beheben auf: { $page } ({ $href })

doctor-overview-command = { "   " }Befehl: { $command }

doctor-overview-analyte-cannot-say-how-things-are-right-now = Analyte kann im Moment keinen Zustand melden

doctor-overview-problem = PROBLEM

doctor-overview-attention = HINWEIS

doctor-overview-add-an-admin-table-with-a-port-to-the = fügen Sie der Programmdatei eine [admin]-Tabelle mit einem Port hinzu und starten Sie den Dienst neu (siehe docs/operations/operate.md)

doctor-overview-run-this-from-an-administrator-powershell-if-the-key = führen Sie dies in einer PowerShell mit Administratorrechten aus; fehlt die Schlüsseldatei, führen Sie analyte admin token --rotate aus und starten Sie den Dienst neu

doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start = starten Sie den Dienst (Start-Service Analyte oder unter Linux systemctl start analyte), warten Sie einige Sekunden und versuchen Sie es erneut; startet er nicht, führen Sie analyte doctor und analyte logs aus

doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection = suchen Sie das Programm, das den Verwaltungsport belegt (Get-NetTCPConnection -LocalPort <port>), und beenden Sie es, oder starten Sie den Dienst mit Restart-Service Analyte neu, falls der Verwaltungsschlüssel seit seinem Start erneuert wurde

doctor-overview-check-that-this-command-and-the-engine-are-the = prüfen Sie, ob dieser Befehl und der Dienst dieselbe Analyte-Version haben, und führen Sie dann analyte logs aus

doctor-overview-the-engine-could-not-read-its-state = Der Dienst konnte seinen Zustand nicht lesen.

doctor-overview-sending-to-lab-app-is-failing-waiting = Senden an Labor-App { $destination } schlägt fehl: { $waiting } wartend

doctor-overview-sending-to-lab-app-is-paused-waiting = Senden an Labor-App { $destination } ist angehalten: { $waiting } wartend

doctor-overview-analyte-stopped-working-with = Analyte arbeitet nicht mehr mit { $machine }

doctor-overview-orders-cancelled-by-a-machine = Von einem Gerät stornierte Aufträge: { $cancelled }

doctor-overview-for = { $UNCHECKED-WARNING } für { $machine }

doctor-overview-disk-space-is-low-free = Wenig Speicherplatz: { $free } frei

doctor-overview-check-the-software-version-reported-by = Prüfen Sie die von { $machine } gemeldete Softwareversion.

doctor-overview-has-gone-quiet = { $machine } meldet sich nicht mehr

doctor-overview-has-sent-nothing-for = { $machine } hat seit { $seconds } nichts gesendet

doctor-overview-a-result-has-waited-to-be-sent = Ein Ergebnis wartet seit { $seconds } auf das Senden

doctor-overview-waiting = , { $count } wartend

doctor-overview-cannot-send-to-lab-app = Senden an Labor-App { $destination } nicht möglich: { $why }{ $waiting }

doctor-overview-orders-with-unknown-outcome = Aufträge mit unbekanntem Ausgang: { $count }

doctor-overview-sending = Senden

doctor-overview-sending-in-line = Senden

doctor-overview-disk = Datenträger

doctor-overview-analyte-review-list = analyte review list

doctor-overview-analyte-actor-you-deliveries-list-reason-why = analyte --actor <you> deliveries list --reason "<why>"

doctor-overview-analyte-orders-list = analyte orders list

doctor-overview-analyte-orders-cancelled = analyte orders cancelled

doctor-overview-analyte-data-keeping = analyte data-keeping

doctor-overview-analyte-actor-you-orders-resume-after-restore-reason-why = analyte --actor <you> orders resume-after-restore --reason "<why>"

doctor-overview-analyte-activity = analyte activity

doctor-overview-analyte-machines-list = analyte machines list

doctor-overview-analyte-lab-apps-list = analyte lab-apps list

doctor-overview-disk-almost-full-analyte-refuses-new-data-from-machines = Datenträger fast voll: Analyte nimmt keine neuen Daten von Geräten an

doctor-overview-analyte-could-not-save-a-machine-software-version-warning = Analyte konnte eine Warnung zur Softwareversion eines Geräts nicht speichern

doctor-overview-keys-missing = Schlüssel fehlen

doctor-overview-not-set-up-any-more = nicht mehr eingerichtet

doctor-overview-items-may-need-review = Einträge erfordern möglicherweise eine Prüfung

doctor-overview-bytes-free = { $free } Byte frei

doctor-overview-bytes = { $reserve } Byte

doctor-overview-alert = Warnung

doctor-overview-responder = Zuständig

doctor-overview-raised = Ausgelöst

doctor-overview-last-value = Letzter Wert

doctor-overview-pick-the-machine-s-model = Wählen Sie das Modell des Geräts

doctor-overview-choose-how-it-s-connected-and-which-lab-app = Wählen Sie, wie es angeschlossen ist und welche Labor-App seine Ergebnisse erhält

doctor-overview-run-a-listen-only-test-analyte-only-reads-and = Führen Sie einen Mithörtest durch. Analyte liest nur und sendet nichts an das Gerät

doctor-overview-save-then-restart-analyte-to-use-the-new-settings = Speichern Sie und starten Sie Analyte dann neu, um die neuen Einstellungen zu verwenden

doctor-overview-nothing-needs-you-right-now = Im Moment ist nichts zu tun.

doctor-overview-analyte-keeps-everything-it-has-received-while-you-fix = Analyte bewahrt alles Empfangene auf, während Sie diese Punkte beheben.

doctor-overview-legend-line-legend-line-wait = legend__line legend__line--wait

doctor-overview-legend-line-legend-line-cut = legend__line legend__line--cut

doctor-overview-link-open = Verbindung offen

doctor-overview-waiting-2 = Wartet

doctor-overview-broken = Unterbrochen

doctor-overview-enough-space = Genug Speicherplatz

doctor-overview-almost-full-receiving-refused = Fast voll: Empfang wird abgelehnt

doctor-overview-order-list = Auftragsliste · { $name }

doctor-overview-cannot-deliver = Übermittlung nicht möglich

doctor-overview-sending-paused = Senden angehalten

doctor-overview-not-delivering = Keine Übermittlung

doctor-overview-up-to-date = Aktuell

doctor-overview-api-key = API-Schlüssel

doctor-overview-signing-key = Signaturschlüssel

doctor-overview-its-keys-cannot-be-checked = Ihre Schlüssel können nicht geprüft werden.

doctor-overview-out-of-date = Veraltet

doctor-overview-not-checked-yet = Noch nicht geprüft

doctor-overview-items-waiting-to-send = Einträge, die auf das Senden warten

doctor-overview-results-and-order-messages = Ergebnisse und Auftragsnachrichten

doctor-overview-oldest-waiting = Ältester wartender Eintrag

doctor-overview-the-check-did-not-finish-within-seconds = die Prüfung wurde nicht innerhalb von { $as-secs } Sekunden abgeschlossen

doctor-overview-is-valid = { $config } ist gültig

doctor-overview-cannot-be-used = { $config } kann nicht verwendet werden: { $error }

doctor-overview-what-to-do-2 = { "      " }Was zu tun ist: { $fix }

doctor-overview-doctor-and = Diagnose: { $failures } und { $warnings }.

doctor-overview-the-store-did-not-open-within-seconds = der Datenspeicher wurde nicht innerhalb von { $as-secs } Sekunden geöffnet

doctor-overview-running-version-and-answering = läuft in Version { $version } und antwortet

doctor-overview-running-version-but-this-command-is-version = läuft in Version { $version }, aber dieser Befehl hat Version { $CARGO-PKG-VERSION }

doctor-overview-there-is-no-store-at-yet = unter { $file } gibt es noch keinen Datenspeicher

doctor-overview-opened-but-did-not-close-cleanly = geöffnet, aber nicht sauber geschlossen: { $error }

doctor-overview-cannot-be-opened = { $data-directory } kann nicht geöffnet werden: { $error }

doctor-overview-free-is-always-kept-free = { $free } frei; { $reserve } werden immer frei gehalten

doctor-overview-free-at-or-below-the-kept-free-analyte-refuses = { $free } frei, auf oder unter den frei gehaltenen { $reserve }: Analyte nimmt keine neuen Daten von Geräten an

doctor-overview-the-free-space-could-not-be-read = der freie Speicherplatz konnte nicht gelesen werden: { $error }

doctor-overview-a-usb-serial-number-names-a-network-or-folder = eine USB-Seriennummer ({ $number }) ist für eine Netzwerk- oder Ordnerverbindung angegeben

doctor-overview-cable = { $what }: Kabel

doctor-overview-analyzer-settings = { $what }: Einstellungen am Analysegerät

doctor-overview-report-poll-interval = { $what }: Abfrageintervall für Berichte

doctor-overview-mode = { $what }: Modus

doctor-overview-every-byte-is-kept-and-recorded-under-captures = { $recording-only }; jedes Byte wird aufbewahrt und unter captures/{ $id } aufgezeichnet

doctor-overview-devices = { $what }: Geräte

doctor-overview-accepts-connections-only-from = nimmt Verbindungen nur von { $allowed } an

doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s = führen Sie analyte machines set { $id } --allowed-peer <IP-Adresse des Analysegeräts> aus, oder aktivieren Sie „Nur dieses Gerät zulassen“ in Schritt 4 seiner Einrichtung

doctor-overview-the-check-for-folder-failed = die Prüfung des Ordners { $path } ist fehlgeschlagen: { $error }

doctor-overview-folder-is-there = Ordner { $path } ist vorhanden

doctor-overview-is-not-a-folder = { $path } ist kein Ordner

doctor-overview-folder-cannot-be-read = Ordner { $path } kann nicht gelesen werden: { $error }

doctor-overview-the-serial-ports-could-not-be-listed = die seriellen Schnittstellen konnten nicht aufgelistet werden: { $error }

doctor-overview-serial-port-is-present = serielle Schnittstelle { $name } ist vorhanden

doctor-overview-serial-port-is-not-present = serielle Schnittstelle { $name } ist nicht vorhanden

doctor-overview-the-adapter-with-serial-number-is-at = der Adapter mit der Seriennummer { $number } ist an { $port }

doctor-overview-no-adapter-with-serial-number-is-plugged-in = kein Adapter mit der Seriennummer { $number } ist angeschlossen

doctor-overview-adapters-report-serial-number = { $several } Adapter melden die Seriennummer { $number }: { $join }

doctor-overview-nothing-listens-on-although-the-engine-runs = auf { $address } lauscht nichts, obwohl der Dienst läuft

doctor-overview-is-free-for-analyte-to-listen-on = { $address } ist frei, damit Analyte darauf lauschen kann

doctor-overview-is-held-by-the-running-engine = { $address } ist vom laufenden Dienst belegt

doctor-overview-another-program-holds = ein anderes Programm belegt { $address }

doctor-overview-is-held-by-the-engine-or-by-another-program = { $address } ist belegt, vom Dienst oder von einem anderen Programm

doctor-overview-cannot-be-used-on-this-computer = { $address } kann auf diesem Computer nicht verwendet werden: { $error }

doctor-overview-the-engine-connects-to-analyte-overview-says-if-the = der Dienst verbindet sich mit { $address }; analyte overview zeigt, ob die Verbindung getrennt ist

doctor-overview-not-tried-the-engine-may-be-connected-to-and = nicht versucht: Der Dienst ist möglicherweise mit { $address } verbunden, und ein Gerät nimmt eventuell nur eine Verbindung gleichzeitig an

doctor-overview-accepts-a-connection = { $address } nimmt eine Verbindung an

doctor-overview-does-not-accept-a-connection = { $address } nimmt keine Verbindung an: { $error }

doctor-overview-did-not-answer-within-seconds = { $address } hat nicht innerhalb von { $as-secs } Sekunden geantwortet

doctor-overview-lab-app = Labor-App { $name }

doctor-overview-when-its-order-list-last-came-could-not-be = wann ihre Auftragsliste zuletzt kam, konnte nicht gelesen werden: { $error }

doctor-overview-credential-is-missing = Zugangsdaten { $name } fehlen

doctor-overview-its-keys-could-not-be-read-from-this-account = ihre Schlüssel konnten mit diesem Konto nicht gelesen werden: { $error }

doctor-overview-resolves-to-no-address = { $host } wird zu keiner Adresse aufgelöst

doctor-overview-does-not-resolve = { $host } wird nicht aufgelöst: { $error }

doctor-overview-did-not-resolve-within-seconds = { $host } wurde nicht innerhalb von { $as-secs } Sekunden aufgelöst

doctor-overview-answers-over-https-with-a-valid-certificate-http = { $url } antwortet über HTTPS mit einem gültigen Zertifikat (HTTP { $status })

doctor-overview-gives-no-https-answer = { $url } gibt keine HTTPS-Antwort: { $error }

doctor-overview-last-complete-order-list = letzte vollständige Auftragsliste { $when }

doctor-overview-its-order-list-is-not-up-to-date-last = ihre Auftragsliste ist nicht aktuell (letzte vollständige Liste: { $when })

doctor-overview-create-it-with-new-localgroup-name-then-add-people = legen Sie sie mit New-LocalGroup -Name "{ $GROUP }" an und fügen Sie dann Personen mit Add-LocalGroupMember -Group "{ $GROUP }" -Member <user> hinzu

doctor-overview-the-group-exists = die Gruppe { $GROUP } existiert

doctor-overview-the-group-is-missing-so-only-administrators-can-sign = die Gruppe { $GROUP } fehlt, daher können sich nur Administratoren mit Windows anmelden

doctor-overview-is-an-account-but-not-a-local-group = { $GROUP } ist ein Konto, aber keine lokale Gruppe

doctor-overview-the-group-could-not-be-looked-up = die Gruppe { $GROUP } konnte nicht nachgeschlagen werden: { $error }

doctor-overview-the-rule-is-there = die Regel "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) ist vorhanden

doctor-overview-no-rule-lets-analyzers-connect-to = keine Regel "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) erlaubt Analysegeräten, sich mit { $join } zu verbinden

doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended = die Firewall-Regeln konnten nicht gelesen werden (PowerShell endete mit { $status })

doctor-overview-the-firewall-rules-could-not-be-read = die Firewall-Regeln konnten nicht gelesen werden: { $error }

doctor-overview-ok = OK

doctor-overview-warn = WARN

doctor-overview-fail = FAIL

doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run = führen Sie analyte doctor erneut aus; bleibt es langsam, führen Sie analyte logs aus

doctor-overview-fix-what-it-names-or-choose-the-right-file = beheben Sie, was genannt wird (oder wählen Sie mit --config die richtige Datei), und führen Sie dann analyte doctor erneut aus; die anderen Prüfungen brauchen eine gültige Programmdatei

doctor-overview-doctor-everything-checked-is-ok = Diagnose: Alles Geprüfte ist in Ordnung.

doctor-overview-store = Datenspeicher

doctor-overview-listed-but-not-set-up-yet-so-it-does = aufgeführt, aber noch nicht eingerichtet, daher läuft es nicht

doctor-overview-choose-its-model-on-the-setup-page-save-then = wählen Sie sein Modell auf der Einrichtungsseite, speichern Sie und starten Sie dann den Dienst neu

doctor-overview-run-the-analyte-exe-the-service-runs-or-update = führen Sie die analyte.exe aus, die der Dienst verwendet, oder aktualisieren Sie beide auf dieselbe Version

doctor-overview-run-analyte-overview-for-where-and-how-to-fix = führen Sie analyte overview aus, um zu sehen, wo und wie Sie jeden Punkt beheben

doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run = führen Sie analyte doctor erneut aus; bleibt es unbekannt, führen Sie analyte logs aus

doctor-overview-start-the-engine-once-start-service-analyte-it-creates = starten Sie den Dienst einmal (Start-Service Analyte); er legt den Datenspeicher an

doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte = führen Sie analyte doctor erneut aus; wiederholt es sich, führen Sie analyte logs aus

doctor-overview-run-this-from-an-administrator-powershell-if-it-names = führen Sie dies in einer PowerShell mit Administratorrechten aus; wird eine Zugriffsliste genannt, siehe docs/operations/install.md

doctor-overview-check-the-machine-s-settings = prüfen Sie die Einstellungen des Geräts

doctor-overview-machine-software = Gerätesoftware

doctor-overview-check-the-store-and-run-analyte-doctor-again = prüfen Sie den Datenspeicher und führen Sie analyte doctor erneut aus

doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing = schaffen Sie Speicherplatz auf diesem Laufwerk; Analyte löscht nichts, um Platz zu schaffen

doctor-overview-check-the-data-directory-s-drive-in-file-explorer = prüfen Sie das Laufwerk des Datenordners im Datei-Explorer

doctor-overview-choose-the-machine-s-address-or-folder-on-the = wählen Sie die Adresse oder den Ordner des Geräts auf der Einrichtungsseite

doctor-overview-accepts-connections-from-any-device-on-the-network-which = nimmt Verbindungen von jedem Gerät im Netzwerk an, das dann Ergebnisse als dieses Gerät senden könnte

doctor-overview-choose-the-machine-s-export-folder-on-the-setup = wählen Sie den Exportordner des Geräts auf der Einrichtungsseite

doctor-overview-create-the-folder-or-share-it-again-or-choose = legen Sie den Ordner an oder geben Sie ihn erneut frei, oder wählen Sie auf der Einrichtungsseite einen anderen

doctor-overview-check-the-usb-serial-adapter-s-driver-in-device = prüfen Sie den Treiber des USB-Seriell-Adapters im Geräte-Manager

doctor-overview-run-analyte-doctor-again = führen Sie analyte doctor erneut aus

doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the = schließen Sie den USB-Seriell-Adapter an, oder wählen Sie auf der Einrichtungsseite die richtige Schnittstelle

doctor-overview-unplug-the-adapter-that-does-not-belong-to-this = ziehen Sie den Adapter ab, der nicht zu diesem Gerät gehört

doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte = führen Sie analyte overview aus: Die Verarbeitung dieses Geräts in Analyte ist möglicherweise gestoppt; starten Sie den Dienst mit Restart-Service Analyte neu

doctor-overview-stop-the-other-program-or-choose-another-port-on = beenden Sie das andere Programm, oder wählen Sie auf der Einrichtungsseite und am Gerät einen anderen Port

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell = schalten Sie die Verwaltungsseite ein, damit doctor es erkennen kann, oder sehen Sie mit Get-NetTCPConnection -LocalPort <port> nach

doctor-overview-choose-an-address-this-computer-has-on-the-setup = wählen Sie auf der Einrichtungsseite eine Adresse, die dieser Computer hat

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2 = schalten Sie die Verwaltungsseite ein, damit doctor erkennen kann, ob der Dienst läuft

doctor-overview-check-that-the-machine-is-on-its-network-settings = prüfen Sie, ob das Gerät eingeschaltet ist, sowie seine Netzwerkeinstellungen und das Kabel

doctor-overview-the-running-engine-has-its-keys = der laufende Dienst hat ihre Schlüssel

doctor-overview-its-api-key-and-signing-key-are-there = ihr API-Schlüssel und ihr Signaturschlüssel sind vorhanden

doctor-overview-its-keys-were-not-checked = ihre Schlüssel wurden nicht geprüft

doctor-overview-no-order-list-is-kept-for-it = für sie wird keine Auftragsliste geführt

doctor-overview-restart-the-engine = starten Sie den Dienst neu

doctor-overview-start-the-engine-and-run-analyte-doctor-again-the = starten Sie den Dienst und führen Sie analyte doctor erneut aus; der Dienst prüft die Schlüssel selbst

doctor-overview-its-address-has-no-host-name = ihre Adresse hat keinen Hostnamen

doctor-overview-fix-its-address-on-the-lab-apps-page = korrigieren Sie ihre Adresse auf der Seite „Labor-Apps“

doctor-overview-check-the-lab-app-s-address-on-the-lab = prüfen Sie die Adresse der Labor-App auf der Seite „Labor-Apps“ und das DNS dieses Computers

doctor-overview-check-this-computer-s-dns-and-network = prüfen Sie DNS und Netzwerk dieses Computers

doctor-overview-check-the-address-the-lab-app-s-server-and = prüfen Sie die Adresse, den Server der Labor-App und sein Zertifikat; ein Zertifikat, dem dieser Computer nicht vertraut, muss auf dem Server korrigiert werden

doctor-overview-check-that-the-engine-runs-and-the-lab-app = prüfen Sie, ob der Dienst läuft und die Labor-App antwortet; Geräte, die nach Aufträgen fragen, erhalten so lange keine Antwort

doctor-overview-computername-is-not-set-so-the-group-cannot-be = COMPUTERNAME ist nicht gesetzt, daher kann die Gruppe nicht nachgeschlagen werden

doctor-overview-run-doctor-from-a-normal-powershell-window = führen Sie doctor in einem normalen PowerShell-Fenster aus

doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to = führen Sie das Installationsprogramm erneut aus, oder erlauben Sie in der Windows Defender Firewall eingehendes TCP aus dem Labornetz zu diesen Ports

doctor-overview-check-the-rule-in-windows-defender-firewall = prüfen Sie die Regel in der Windows Defender Firewall

doctor-overview-reading-the-firewall-rules-took-too-long = das Lesen der Firewall-Regeln dauerte zu lange

doctor-overview-with-an-unknown-outcome-check-the-machine = { $uncertain } mit unbekanntem Ausgang (Gerät prüfen)

doctor-overview-cancelled-by-a-machine = { $cancelled } von einem Gerät storniert

doctor-overview-analyte-could-not-read-its-store = Analyte konnte seinen Datenspeicher nicht lesen.

doctor-overview-analyte-could-not-read-whether-order-sending-is-paused = Analyte konnte nicht lesen, ob das Senden von Aufträgen nach einer Wiederherstellung angehalten ist.

doctor-overview-analyte-could-not-read-the-free-disk-space = Analyte konnte den freien Speicherplatz nicht lesen.

doctor-overview-analyte-could-not-read-how-many-items-need-review = Analyte konnte nicht lesen, wie viele Einträge eine Prüfung erfordern.

doctor-overview-analyte-could-not-read-when-each-lab-app-last = Analyte konnte nicht lesen, wann jede Labor-App zuletzt eine Übermittlung angenommen hat.

doctor-overview-analyte-could-not-read-which-lab-apps-keep-failing = Analyte konnte nicht lesen, welche Labor-Apps Übermittlungen wiederholt nicht annehmen.

doctor-overview-analyte-could-not-read-the-active-alerts = Analyte konnte die aktiven Warnungen nicht lesen.

doctor-overview-analyte-could-not-read-the-machine-orders = Analyte konnte die Geräteaufträge nicht lesen.

doctor-overview-analyte-could-not-read-when-each-machine-last-sent = Analyte konnte nicht lesen, wann jedes Gerät zuletzt Daten gesendet hat.

doctor-overview-analyte-could-not-read-what-waits-for-each-lab = Analyte konnte nicht lesen, was für jede Labor-App wartet.

doctor-overview-analyte-could-not-read-what-waits-for-lab-apps = Analyte konnte nicht lesen, was für Labor-Apps wartet, die kein eingerichtetes Gerät verwendet.

doctor-overview-analyte-could-not-read-the-alert-relays = Analyte konnte die Warnungsweiterleitungen nicht lesen.

doctor-overview-analyte-read-everything-but-could-not-write-the-page = Analyte hat alles gelesen, konnte die Seite aber nicht schreiben.

doctor-overview-no-orders-with-an-unknown-outcome = Keine Aufträge mit unbekanntem Ausgang

doctor-overview-see-machine-orders = Geräteaufträge ansehen

doctor-overview-no-orders-cancelled-by-a-machine = Keine von einem Gerät stornierten Aufträge

doctor-overview-review-cancelled-orders = Stornierte Aufträge prüfen

doctor-overview-a-result-has-waited-too-long-to-be-sent = Ein Ergebnis wartet zu lange auf das Senden

doctor-overview-machine-has-gone-quiet = Gerät meldet sich nicht mehr

doctor-overview-machine-software-version-changed = Softwareversion des Geräts geändert

doctor-overview-disk-space-is-low = Wenig Speicherplatz

doctor-overview-order-list-is-out-of-date = Auftragsliste ist veraltet

doctor-overview-orders-with-unknown-outcome-2 = Aufträge mit unbekanntem Ausgang

doctor-overview-items-need-review = Einträge erfordern eine Prüfung

doctor-overview-results-cannot-be-delivered = Ergebnisse können nicht übermittelt werden

doctor-overview-error-admin-off = die Verwaltungsseiten sind in den Einstellungen ausgeschaltet (keine [admin]-Tabelle), daher kann der laufende Dienst nicht gefragt werden

doctor-overview-error-not-running = unter der Verwaltungsadresse { $value } antwortet nichts: Der Dienst läuft nicht

doctor-overview-error-unreachable = die Verwaltungsadresse { $address } ist nicht erreichbar: { $source }

doctor-overview-error-timed-out = der Dienst hat nicht innerhalb von { $as-secs } Sekunden geantwortet

doctor-overview-error-key = der Verwaltungsschlüssel kann mit diesem Benutzer nicht gelesen werden ({ $value }); führen Sie den Befehl als Administrator aus

doctor-overview-error-key-thread = das Lesen des Verwaltungsschlüssels wurde abgebrochen: { $value }

doctor-overview-error-client = die Anfrage konnte nicht vorbereitet werden: { $value }

doctor-overview-error-unproven = unter der Verwaltungsadresse antwortet etwas anderes: Es konnte nicht nachweisen, dass es dieser Analyte-Dienst ist, daher wurde der Verwaltungsschlüssel nicht gesendet

doctor-overview-error-silent = der Dienst hat nicht geantwortet: { $value }

doctor-overview-error-status = der Dienst hat mit HTTP { $value } geantwortet

doctor-overview-error-too-large = die Antwort des Dienstes war zu groß

doctor-overview-error-unreadable = die Antwort des Dienstes konnte nicht gelesen werden: { $value }

doctor-overview-error-request = die Anfrage konnte nicht geschrieben werden: { $value }

doctor-overview-error-not-sent = der Dienst war nicht erreichbar: { $value }

doctor-overview-error-timed-out-2 = der Dienst hat nicht innerhalb von { $as-secs } Sekunden auf die Änderung geantwortet

doctor-overview-signal-path-section = 02 · Signalweg

doctor-overview-signal-path-flow = — Geräte → Analyte → Labor-Apps

doctor-overview-disk-alerts-section = 04 · Datenträger und Warnungen

doctor-overview-lab-app-key-advice = geben Sie ihre Schlüssel auf der Seite „Labor-Apps“ ein und starten Sie dann den Dienst neu

doctor-overview-orders-section = 03 · Aufträge
