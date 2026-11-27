### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossar für die Prüfung
### sample = Probe; sample ID = Proben-ID; tube ID = Röhrchen-ID; result = Ergebnis; test = Test (Analyse)
### rerun = Wiederholungsmessung; resend = erneute Übertragung; reprocess = neu verarbeiten
### held for review = zur Prüfung zurückgehalten; Needs review = Prüfung erforderlich
### machine = Gerät; analyzer = Analysegerät; lab app = Labor-App; engine = Dienst
### delivery = Übermittlung; order = Auftrag; record only = nur aufzeichnen
### hold = Löschsperre; clean-up = Bereinigung; Data keeping = Datenaufbewahrung
### Setup = Einrichtung; admin pages = Verwaltungsseiten; reading = Auswertung; difference = Abweichung

admin-live-at = Live · { $time }

admin-updated-at = Aktualisiert um { $time }

admin-resume-updates = Aktualisierung fortsetzen

admin-language = Sprache

admin-browser-language = Sprache des Browsers verwenden

admin-count-failed = Ergebnisse, die noch von entfernten Geräten warten, konnten nicht gezählt werden

admin-activity-lede = Nur Proben-IDs und Anzahlen. Patientennamen und Ergebniswerte erscheinen hier nie.

admin-orders-lede = Aufträge aus der Labor-App, die Analyte an ein Gerät gesendet hat, und was das Gerät damit gemacht hat. Analyte sendet einen Auftrag nie von sich aus erneut.

admin-notes-hidden = Die Notizen des Geräts sind ausgeblendet, weil Freitext Patientendaten enthalten kann. Jedes Mal, wenn sie angezeigt werden, hält Analyte zuerst fest, wer sie angefordert hat und warum.

admin-item-lede = Was das Gerät angenommen hat und was es tatsächlich getan hat, wird getrennt angezeigt. Zeitangaben behalten die Schreibweise und Zeitzone des Geräts; lässt das Gerät eine Zeit weg, bleibt die frühere erhalten. Als abgelehnt markierte Nachrichten wurden nicht angewendet.

admin-cancelled-lede = Auftragspositionen, die ein Gerät von sich aus storniert hat. Klären Sie jede mit dem Labor und informieren Sie die Labor-App, wenn eine Probe erneut gemessen werden muss.

admin-cancelled-notice = Das Gerät hat diese Tests von sich aus storniert. Dadurch werden sie in der Labor-App nicht storniert, die anderen Tests des Auftrags nicht storniert und Analyte darf nichts weiter senden; die Labor-App wird nicht automatisch informiert. Wenn eine Stornierungsanfrage der Labor-App offen ist, schließen Sie sie mit der eigenen Antwort der Labor-App ab: Analyte zählt die Stornierung durch das Gerät nicht als diese Antwort.

admin-deliveries-lede = Ergebnisse und Auftragsnachrichten, die Analyte an eine Labor-App sendet, und ihr jeweiliger Stand. Jede behält bei jedem erneuten Versuch dieselbe Übermittlungsnummer, damit die Labor-App sie nie doppelt ablegt.

admin-keeping-lede = Wie lange Analyte empfangene Nachrichten aufbewahrt, und Löschsperren, die ausgewählte Daten aufbewahren, bis jemand sie aufhebt.

admin-holds-text = Eine Löschsperre schützt Daten vor der Bereinigung, bis jemand sie aufhebt; sie läuft nie ab. Eine Löschsperre für einen Auftrag bewahrt auch seine Nachrichten auf. Wenn Analyte nicht sicher ist, was zusammengehört, bewahrt es mehr auf.

admin-settings-lede = Womit Analyte arbeitet, aus seiner Programmdatei und von diesem PC. Nur lesbar: Geräte und Labor-Apps werden unter „Einrichtung“ geändert, alles andere in der Programmdatei.

admin-last-clean-up = Letzte Bereinigung

admin-holds = Löschsperren

admin-hold-not-saved = Löschsperre nicht gespeichert

admin-add-hold = Löschsperre hinzufügen

admin-release = Aufheben

admin-lede = Nachrichten und Ergebnisse, die Analyte aufbewahrt, aber nicht selbst verarbeiten konnte. Einträge bleiben hier, bis jemand sie klärt; ihr Verlauf bleibt erhalten.

admin-sign-in = Anmelden

admin-setup-stopped = Die Einrichtung konnte nicht fortgesetzt werden

admin-feed-undated = Zeit nicht aufgezeichnet

admin-lab-app = Labor-App

admin-nothing-waiting = nichts wartet

admin-send-again = Erneut senden

admin-waiting-to-send = Wartet auf das Senden

admin-recording-only = Nur aufzeichnen — nichts wird an eine Labor-App gesendet

admin-open-to-any = Nimmt Verbindungen von jedem Gerät an

admin-open-to-any-advice = Jedes Gerät im Labornetz, das diesen Port erreicht, kann Ergebnisse als dieses Gerät senden. Bearbeiten Sie seine Einrichtung und wählen Sie nach dem Verbindungstest „Nur dieses Gerät zulassen“.

admin-refused-advice = Nur das Gerät selbst sollte sich mit diesem Port verbinden. Ist es das Gerät selbst, hat sich seine Adresse geändert: Fügen Sie die neue Adresse in seiner Einrichtung hinzu. Wenn nicht, ermitteln Sie dieses Gerät.

admin-any-device = Jedes Gerät

admin-not-set-up = Noch nicht eingerichtet

admin-starts-after-restart = Gespeichert: beginnt nach einem Neustart

admin-stops-at-restart = Entfernt: endet beim Neustart

admin-settings-unreadable = Die Einstellungen von Analyte konnten nicht gelesen werden

admin-add-machine = Gerät hinzufügen

admin-not-chosen = Noch nicht gewählt

admin-not-running = Läuft noch nicht

admin-in-analyte = In Analyte

admin-connected-by = Angeschlossen über

admin-accepts-from = Nimmt Verbindungen an von

admin-cable-settings = Kabeleinstellungen

admin-analyzer-settings = Einstellungen am Analysegerät

admin-last-data = Letzte Daten

admin-sends-results-to = Sendet Ergebnisse an

admin-model = Modell

admin-connection = Verbindung

admin-machine = Gerät

admin-open-machine = Gerät öffnen

admin-nothing-received-yet = Noch nichts empfangen

admin-no-machine-running = Kein Gerät läuft

admin-no-machine-set-up = Kein Gerät ist eingerichtet.

admin-poll-title = Ergebnisabfrage nicht abgeschlossen

admin-how-read = Wie diese Nachricht ausgewertet wurde

admin-compare-measurements = Messungen vergleichen

admin-mark-resolved = Als geklärt markieren

admin-process-again = Erneut verarbeiten

admin-marked-resolved = Als geklärt markiert

admin-processed-again = Erneut verarbeitet

admin-open-item = Eintrag öffnen

admin-linked-record = Verknüpfter Datensatz

admin-as-arrived = Teile so anzeigen, wie sie ankamen

admin-original-message = Originalnachricht

admin-show-original = Originalnachricht anzeigen

admin-restore-paused = Das Senden von Aufträgen ist nach einer Wiederherstellung angehalten

admin-restore-advice = Bitten Sie die IT, die Aufträge über die Befehlszeile abzugleichen; bis dahin wird kein Auftrag an ein Gerät gesendet.

admin-restart-advice = Ein Neustart unterbricht alle Geräteverbindungen für einige Sekunden, und Analyte bestätigt nichts, solange es gestoppt ist. Melden Sie sich nach dem Neustart erneut an.

admin-patient-data = Kann Patientendaten enthalten

admin-patient-data-text = Halten Sie dies aus Bildschirmfotos und Support-Tickets heraus. Analyte hat vor der Anzeige festgehalten, wer sie angefordert hat und warum.

admin-support = Details für den Support

admin-not-recorded = Nicht aufgezeichnet

admin-nothing-yet = Noch nichts

admin-not-applicable = —

admin-slow-read = Analyte hat das Lesen nicht innerhalb von 1,5 Sekunden abgeschlossen.

admin-stopped = In Analyte gestoppt

admin-stopped-sentence = Analyte arbeitet nicht mehr mit diesem Gerät

admin-removed-advice = Nach dem nächsten Neustart verbindet sich Analyte nicht mehr damit; bis dahin läuft es weiter. Seine Ergebnisse, zu prüfenden Einträge und sein Verlauf bleiben erhalten und hier sichtbar.

admin-set-up-advice = richten Sie es ein: Wählen Sie sein Modell, wie es angeschlossen ist und welche Labor-App seine Ergebnisse erhält, führen Sie einen Mithörtest durch, speichern Sie und starten Sie dann neu.

admin-failing-advice = Prüfen Sie, ob die Labor-App läuft und dieser PC sie unter ihrer Adresse erreichen kann.

admin-keys-apply = Wartende Einträge werden gesendet, nachdem Analyte mit den Schlüsseln neu gestartet ist.

admin-keys-advice = fügen Sie die fehlenden Schlüssel auf der Seite „Labor-Apps“ oder in der Umgebung des Dienstes hinzu. Wartende Einträge werden gesendet, nachdem Analyte mit den Schlüsseln neu gestartet ist.

admin-key-needed = Erforderlich, bevor Analyte Ergebnisse senden kann.

admin-key-unchecked = Die verschlossene Schlüsseldatei konnte nicht gelesen werden, daher kann dieser Schlüssel nicht geprüft werden; der Grund steht unter „Details für den Support“.

admin-nothing-changed = Es wurde nichts geändert

admin-restart-now = Jetzt neu starten

admin-first-page = Zurück zur ersten Seite

admin-line-key = Zeilenschlüssel

admin-add-or-set-one-up-on-the-setup-page = Fügen Sie eines auf der Einrichtungsseite hinzu oder richten Sie es dort ein.

admin-analyte-reads-keeps-a-copy-sends = Analyte · liest · bewahrt eine Kopie auf · sendet

admin-no-lab-app = Keine Labor-App

admin-no-running-machine-sends-to-a-lab-app = Kein laufendes Gerät sendet an eine Labor-App.

admin-differences-used-for-this-message = Für diese Nachricht verwendete Abweichungen

admin-where-this-difference-comes-from = Woher diese Abweichung stammt

admin-none-were-used = Es wurden keine verwendet.

admin-this-message-was-read-by-an-older-version-that = Diese Nachricht wurde von einer älteren Version ausgewertet, die nicht festhielt, welche Abweichungen eingeschaltet waren oder verwendet wurden; das bedeutet nicht, dass keine verwendet wurden.

admin-what-to-do = Was zu tun ist:

admin-next-page = Nächste Seite

admin-reason = Grund

admin-cancel = Abbrechen

admin-reason-optional = Grund (optional)

admin-recorded-with-the-view = Wird zusammen mit der Ansicht festgehalten.

admin-paste-it-from-your-lab-app-team-s-message = Fügen Sie ihn aus der Nachricht des Teams Ihrer Labor-App ein.

admin-replace = Ersetzen

admin-back = Zurück

admin-next = Weiter

admin-welcome = Willkommen

admin-analyte = · Analyte

admin-main = Hauptbereich

admin-analyte-2 = Analyte

admin-engine-running = Dienst läuft

admin-more = Mehr

admin-signed-in-as = Angemeldet als

admin-signed-in = Angemeldet

admin-sign-out = Abmelden

admin-signed-in-with-the-admin-key = Mit dem Verwaltungsschlüssel angemeldet

admin-newest-at-the-top = Neueste oben

admin-live = Live

admin-pause-updates = Aktualisierung anhalten

admin-every-try-of-one-delivery-newest-first-one-line = Jeder Versuch einer Übermittlung, neueste zuerst, je eine Zeile

admin-show-all-activity = Alle Aktivitäten anzeigen

admin-all-machines = Alle Geräte

admin-show = Anzeigen

admin-type = Art

admin-today = Heute ·

admin-show-earlier-activity = Frühere Aktivitäten anzeigen

admin-read-from-what-analyte-stored-what-is-kept-is = Gelesen aus dem, was Analyte gespeichert hat; was aufbewahrt wird, ist festgelegt unter

admin-this-reading = Diese Auswertung

admin-differences-switched-on-for-this-machine = Für dieses Gerät eingeschaltete Abweichungen

admin-none-were-switched-on = Es waren keine eingeschaltet.

admin-switched-on-does-not-mean-used-the-next-card = Eingeschaltet heißt nicht verwendet: Die nächste Karte zeigt, welche für diese Nachricht verwendet wurden.

admin-their-declarations = Ihre Angaben

admin-not-recorded-by-the-version-that-read-this-message = Von der Version, die diese Nachricht ausgewertet hat, nicht festgehalten.

admin-result-statuses = Ergebnisstatus

admin-no-results = Keine Ergebnisse

admin-this-reading-produced-no-results = Diese Auswertung hat keine Ergebnisse ergeben.

admin-none-sent = Keine gesendet

admin-the-references-above-point-to-the-manual-sections-and = Die Verweise oben zeigen auf die Handbuchabschnitte und Aufzeichnungen hinter jeder Abweichung. Die Originalbytes bleiben im geschützten Speicher; um sie zu sehen, verwenden Sie „Originalnachricht anzeigen“ bei einem Eintrag unter „Prüfung erforderlich“.

admin-analyte-is-listening-it-reads-connected-once-the-machine = Analyte hört mit; sobald sich das Gerät verbindet, steht hier „Verbunden“. Bleibt es so, prüfen Sie die Host-Einstellungen des Geräts.

admin-check-the-machine-s-software-version = Prüfen Sie die Softwareversion des Geräts

admin-first-seen = Zuerst gesehen

admin-settings-checked-for-this-version = Einstellungen für diese Version geprüft

admin-accepted-differences-from-the-standard = Zugelassene Abweichungen vom Standard

admin-none-its-settings-switch-on-no-difference-from-the = Keine: Seine Einstellungen schalten keine Abweichung vom Standard ein.

admin-this-model-does-a-few-things-its-own-way = Dieses Modell macht einiges auf seine eigene Art. Seine Einstellungen schalten diese Abweichungen ein; die Auswertung jeder Nachricht zeigt, welche verwendet wurden.

admin-where-these-differences-come-from = Woher diese Abweichungen stammen

admin-recent-messages-received = Zuletzt empfangene Nachrichten

admin-messages-this-machine-sends-are-listed-here-newest-first = Nachrichten, die dieses Gerät sendet, werden hier aufgeführt, neueste zuerst.

admin-how-it-was-read = Wie sie ausgewertet wurde

admin-no-order-was-cancelled-by-a-machine = Kein Auftrag wurde von einem Gerät storniert

admin-order-items-a-machine-cancels-on-its-own-are = Auftragspositionen, die ein Gerät von sich aus storniert, werden hier aufgeführt.

admin-check-each-order-below-with-the-lab-and-the = Klären Sie jeden Auftrag unten mit dem Labor und der Labor-App

admin-sample = Probe

admin-order = Auftrag

admin-details = Details

admin-send-it-even-if-a-newer-correction-of-this = Auch senden, wenn eine neuere Korrektur dieses Ergebnisses existiert

admin-this-item = Diese Position

admin-other-items-of-this-order = Andere Positionen dieses Auftrags:

admin-the-lab-app-s-copy-of-this-order = Die Kopie dieses Auftrags in der Labor-App

admin-messages-from-the-machine-about-this-item = Nachrichten des Geräts zu dieser Position

admin-no-message-about-this-item = Keine Nachricht zu dieser Position

admin-messages-the-machine-sends-about-this-item-are-listed = Nachrichten, die das Gerät zu dieser Position sendet, werden hier aufgeführt, älteste zuerst.

admin-notes-the-machine-sent-as-sent = Notizen des Geräts, wie gesendet

admin-to-show-the-original-message = (erforderlich, um die Originalnachricht anzuzeigen).

admin-keeping-times = Aufbewahrungsfristen

admin-no-clean-up-has-run-yet = Es wurde noch keine Bereinigung ausgeführt.

admin-no-holds = Keine Löschsperren

admin-nothing-is-held-now-clean-up-follows-the-keeping = Derzeit ist nichts gesperrt; die Bereinigung folgt den Aufbewahrungsfristen.

admin-hold-a = Löschsperre für

admin-message = Nachricht

admin-message-number-or-lab-app-order-number = Nachrichtennummer oder Auftragsnummer der Labor-App

admin-lab-app-orders-only = Labor-App (nur Aufträge)

admin-recorded-with-the-hold-and-the-time = Wird zusammen mit der Löschsperre und der Uhrzeit festgehalten.

admin-i-confirm-this-hold = Ich bestätige diese Löschsperre

admin-save-hold = Löschsperre speichern

admin-sign-in-to-add-or-release-a-hold = Melden Sie sich an, um eine Löschsperre hinzuzufügen oder aufzuheben

admin-analyte-records-who-places-or-releases-a-hold-so = Analyte hält fest, wer eine Löschsperre setzt oder aufhebt, und braucht dafür eine angemeldete Browsersitzung.

admin-engine = Dienst

admin-running = Läuft

admin-store-limits = Grenzen des Datenspeichers

admin-time-limits = Zeitgrenzen

admin-alerts = Warnungen

admin-keys = Schlüssel

admin-where-each-is-found-never-its-value = Wo jeder liegt, niemals sein Wert

admin-arrived = Eingegangen

admin-item = Eintrag

admin-history = Verlauf

admin-item-2 = · Eintrag

admin-why-it-s-here = Warum er hier ist

admin-everything-linked-to-it = Alles, was damit verknüpft ist

admin-the-original-bytes-are-hidden-because-they-may-contain = Die Originalbytes sind ausgeblendet, weil sie Patientendaten enthalten können. Jedes Mal, wenn sie angezeigt werden, hält Analyte zuerst fest, wer sie angefordert hat und warum.

admin-tube-id = Röhrchen-ID

admin-tube-id-missing = Röhrchen-ID fehlt

admin-analyzer-status = Status des Analysegeräts

admin-source-fields-and-receipt-id = Quellfelder und Empfangs-ID

admin-this-looks-like-the-same-result-now-with-tube = Dies scheint dasselbe Ergebnis zu sein, jetzt mit Röhrchen-ID

admin-resolve-the-held-item = . Den zurückgehaltenen Eintrag klären?

admin-choose-resend-to-record-a-duplicate-without-sending-another = Wählen Sie „erneute Übertragung“, um ein Duplikat festzuhalten, ohne ein weiteres Ergebnis zu senden. Wählen Sie „Wiederholungsmessung“, um eine neue Messung an die Labor-App zu senden.

admin-held-measurement = Zurückgehaltene Messung

admin-later-measurement = Spätere Messung

admin-earlier-measurement = Frühere Messung

admin-resolve-held-item = Zurückgehaltenen Eintrag klären

admin-this-is-a-resend = Dies ist eine erneute Übertragung

admin-this-is-a-rerun = Dies ist eine Wiederholungsmessung

admin-left-every-byte-exactly-in-hex-right-the-same = Links: jedes Byte genau, hexadezimal. Rechts: dieselben Bytes als Text, wobei Bytes, die keine einfachen Buchstaben oder Zeichen sind, als Punkte erscheinen.

admin-sign-in-to-analyte = Bei Analyte anmelden

admin-use-the-admin-key-of-analyte-on-this-pc = Verwenden Sie den Verwaltungsschlüssel von Analyte auf diesem PC.

admin-admin-key = Verwaltungsschlüssel

admin-sign-in-with-windows = Mit Windows anmelden

admin-the-key-is-in-the-file = Der Schlüssel steht in der Datei

admin-admin-token = admin-token

admin-in-analyte-s-data-folder-on-this-pc-ask = im Datenordner von Analyte auf diesem PC. Fragen Sie die IT, wenn Sie ihn nicht haben.

admin-engine-running-it-keeps-running-when-you-sign-out = Dienst läuft · er läuft weiter, wenn Sie sich abmelden

admin-restart-to-apply = Zum Anwenden neu starten

admin-lab-app-2 = Labor-App { $destination }

admin-removed-machine = Entferntes Gerät { $machines }

admin-removed-machines = Entfernte Geräte { $machines }

admin-of-them-need-someone-to-act-on-the-deliveries = { " " }Davon erfordern { $held } ein Eingreifen auf der Seite „Übermittlungen“ (angehalten, abgelehnt oder im Konflikt).

admin-kept-and-never-deleted = { $from }: { $items } { admin-kept-item-agreement } aufbewahrt und nie gelöscht.{ $held }

admin-still-being-delivered-to-lab-app = { $from }: { $items } werden noch an Labor-App { $destination } übermittelt.{ $held }

admin-and-possibly-more = { " und möglicherweise mehr" }

admin-messages-from-machines-deliveries-to-lab-apps-orders-and = Nachrichten von Geräten, Übermittlungen an Labor-Apps, Aufträge und Änderungen werden hier aufgeführt, sobald Analyte sie gespeichert hat.

admin-all = Alle

admin-results = Ergebnisse

admin-orders = Aufträge

admin-problems = Probleme

admin-changes = Änderungen

admin-no-earlier-activity = Keine frühere Aktivität

admin-this-page-is-past-the-oldest-line-analyte-keeps = Diese Seite liegt vor der ältesten Zeile, die Analyte aufbewahrt.

admin-back-to-the-newest = Zurück zu den neuesten

admin-no-tries-kept = Keine Versuche aufbewahrt

admin-analyte-keeps-no-try-of-this-delivery = Analyte bewahrt keinen Versuch dieser Übermittlung auf.

admin-nothing-for-this-filter = Nichts für diesen Filter

admin-no-stored-activity-matches-this-machine-and-type = Keine gespeicherte Aktivität passt zu diesem Gerät und dieser Art.

admin-no-activity-yet = Noch keine Aktivität

admin-expected-the-position-of-a-line-as-at-source = erwartet wurde die Position einer Zeile, als at.source.key

admin-reading = Auswertung { $version }

admin-message-2 = Nachricht: { $id }

admin-reading-2 = Auswertung: { $version }

admin-read-by = Ausgewertet von: { $driver }

admin-settings-version = Einstellungsversion: { $digest }

admin-incomplete-record-reason-as-stored = Unvollständiger Datensatz, Grund wie gespeichert: { $reason }

admin-reason-as-stored = Grund wie gespeichert: { $reason }

admin-record-field = Datensatz { $record }, Feld { $field }

admin-coordinates-spans = Koordinaten: { $coordinates }; Bereiche: { $spans }

admin-marked-final-because-the-manual-says-so-not-because = Als endgültig markiert, weil das Handbuch es so angibt, nicht weil das Gerät einen endgültigen Status gesendet hat

admin-reading-3 = Auswertung

admin-read-by-2 = Ausgewertet von

admin-settings-label = Einstellungsbezeichnung

admin-settings-version-2 = Einstellungsversion

admin-not-sent-to-the-lab-app-the-record-of = Nicht an die Labor-App gesendet: Die Aufzeichnung der verwendeten Abweichungen ist unvollständig

admin-this-message-could-not-be-read = Diese Nachricht konnte nicht ausgewertet werden

admin-earlier-reading = Frühere Auswertung

admin-next-reading = Nächste Auswertung

admin-as-received = Wie empfangen

admin-read-as = Ausgewertet als

admin-standard = Standard

admin-where-in-the-message = Wo in der Nachricht

admin-it-waits-on-the-needs-review-page-instead-of = Es wartet auf der Seite „Prüfung erforderlich“, statt an die Labor-App zu gehen.

admin-production-mode-supplied-by-the-machine-definition = Produktionsmodus aus der Gerätedefinition übernommen

admin-the-received-astm-header-had-an-empty-processing-id = Der empfangene ASTM-Header hatte eine leere Processing-ID.

admin-accepted-on-purpose-values-are-kept-exactly-as-received = Bewusst zugelassen: Werte werden genau wie empfangen aufbewahrt

admin-the-newest-messages-are-listed-older-ones-stay-stored = Die neuesten { $RECENT } Nachrichten werden aufgeführt; ältere bleiben gespeichert.

admin-machine-2 = Gerät: { $id }

admin-message-received = Nachricht { $id } empfangen { $received-at }

admin-is-removed-from-analyte-s-settings = { $id } ist aus den Einstellungen von Analyte entfernt

admin-machines-set-up-or-removed-since-analyte-started-may = Geräte, die seit dem Start von Analyte eingerichtet oder entfernt wurden, fehlen hier möglicherweise.

admin-no-machine-is-set-up-yet = Noch kein Gerät eingerichtet

admin-add-one-then-set-it-up-step-by-step = Fügen Sie eines hinzu und richten Sie es dann Schritt für Schritt ein: Modell, Verbindung, Mithörtest und Speichern.

admin-no-more-machines = Keine weiteren Geräte

admin-this-page-is-past-the-end-of-the-list = Diese Seite liegt hinter dem Ende der Liste.

admin-whether-this-machine-is-still-set-up-is-not = Ob dieses Gerät noch eingerichtet ist, ist im Moment nicht bekannt.

admin-removed-from-analyte-s-settings = Aus den Einstellungen von Analyte entfernt

admin-shown-after-analyte-restarts = Wird nach einem Neustart von Analyte angezeigt

admin-set-it-up = Einrichten

admin-repeated-results = Wiederholte Ergebnisse

admin-handled-automatically-a-resend-is-kept-once-a-correction = Automatisch behandelt: Eine erneute Übertragung wird einmal behalten, eine Korrektur ist eine neue Revision

admin-always-ask-me-every-repeat-waits-in-needs-review = Immer nachfragen: Jede Wiederholung wartet unter „Prüfung erforderlich“

admin-recorded = Aufgezeichnet

admin-stopped-2 = Gestoppt

admin-test-connection = Verbindung testen

admin-edit-setup = Einrichtung bearbeiten

admin-no-reading-stored = keine Auswertung gespeichert

admin-in-needs-review = Unter „Prüfung erforderlich“

admin-machine-removed = Gerät entfernt

admin-since = Seit

admin-sends-to = Sendet an

admin-time = Zeit

admin-status-sent-by-the-machine = Vom Gerät gesendeter Status

admin-shown-as = Angezeigt als

admin-this-machine-has-no-software-warning-you-can-confirm = Für dieses Gerät gibt es keine Softwarewarnung, die Sie bestätigen können.

admin-the-lab-app-s-copy-says = Die Kopie der Labor-App besagt: { $state }

admin-lab-app-order-version-sent-machine-order-item = Labor-App { $destination } · Auftragsversion { $version } gesendet · Geräteauftrag { $id }, Position { $index }

admin-order-item = Auftrag { $number }, Position { $index }

admin-order-2 = Auftrag { $number }

admin-message-receipt-reading = Nachricht { $message } · Empfang { $receipt }, Auswertung { $version }

admin-receipt-reading = Empfang { $receipt }, Auswertung { $interpretation }

admin-recorded-2 = Aufgezeichnet { $shown }{ $else }

admin-the-same-version-that-was-sent = { $version }: dieselbe Version, die gesendet wurde

admin-a-different-version-from-the-one-sent-so-its = { $version }: eine andere Version als die gesendete, daher sind ihre Tests nicht die, die das Gerät erhalten hat

admin-the-lab-app-s-copy-cannot-be-read-so = Die Kopie der Labor-App kann nicht gelesen werden, daher wird ihre Version nicht angezeigt. Details für den Support: { $error }

admin-from-the-machine-hidden = { $notes } vom Gerät, ausgeblendet

admin-message-received-2 = Nachricht empfangen { $now }

admin-cancelled-by-the-machine-recorded = { "Vom Gerät storniert · aufgezeichnet " }

admin-the-lab-app-s-copy-cannot-be-read = Die Kopie der Labor-App kann nicht gelesen werden.

admin-no-copy-of-this-order-from-the-lab-app = Es wird keine Kopie dieses Auftrags aus der Labor-App aufbewahrt.

admin-not-known = nicht bekannt

admin-this-form-is-not-one-analyte-sent-for-this = Dieses Formular hat Analyte nicht für diese Seite gesendet. Öffnen Sie den Auftrag erneut und wiederholen Sie Ihre Aktion.

admin-show-the-notes = Notizen anzeigen

admin-order-status = Auftragsstatus

admin-order-version-sent = Gesendete Auftragsversion

admin-tests-sent = Gesendete Tests

admin-last-change = Letzte Änderung

admin-status = Status

admin-what-the-machine-reports = Was das Gerät meldet

admin-tests = Tests

admin-not-recorded-older-data = Nicht aufgezeichnet (ältere Daten)

admin-order-number-on-the-machine = Auftragsnummer am Gerät

admin-last-note = Letzte Notiz

admin-cancelled-in-the-lab-app = In der Labor-App storniert

admin-kept-until = Aufbewahrt bis

admin-its-version = Ihre Version

admin-recorded-by-analyte = Von Analyte aufgezeichnet

admin-outcome = Ausgang

admin-results-came-with-it = Ergebnisse kamen mit

admin-not-sent-the-earlier-expected-time-is-kept = Nicht gesendet; die früher erwartete Zeit bleibt erhalten

admin-sent-as-empty-cleared-by-the-machine = Leer gesendet (vom Gerät gelöscht)

admin-not-sent = nicht gesendet

admin-expected-ready-time-as-sent = Erwartete Fertigstellungszeit, wie gesendet

admin-message-time-and-zone = Nachrichtenzeit und Zeitzone

admin-event-time-as-sent = Ereigniszeit, wie gesendet

admin-delivery = Übermittlung { $id } · { $state }

admin-analyte-sends-it-to-lab-app-again-unchanged-and = Analyte sendet sie erneut an Labor-App { $destination }, unverändert und mit derselben Übermittlungsnummer. Ihr Status jetzt: { $word }. Hat die Labor-App sie bereits, behandelt sie die Wiederholung als erledigt. Analyte prüft bei Ihrer Bestätigung erneut, ob sie gesendet werden darf.

admin-it-goes-to-the-lab-app-unchanged-with-the = Sie geht unverändert und mit derselben Übermittlungsnummer an die Labor-App, sodass die Labor-App eine Wiederholung als erledigt behandelt.

admin-not-delivered = Nicht übermittelt

admin-no-more-deliveries = Keine weiteren Übermittlungen

admin-this-page-is-past-the-last-delivery = Diese Seite liegt hinter der letzten Übermittlung.

admin-no-deliveries-yet = Noch keine Übermittlungen

admin-results-and-order-messages-analyte-sends-to-a-lab = Ergebnisse und Auftragsnachrichten, die Analyte an eine Labor-App sendet, werden hier aufgeführt.

admin-nothing-waits-for-a-lab-app = Nichts wartet auf eine Labor-App

admin-every-delivery-listed-was-accepted-by-its-lab-app = Jede aufgeführte Übermittlung wurde von ihrer Labor-App angenommen.

admin-show-all-deliveries = Alle Übermittlungen anzeigen

admin-order-message = Auftragsnachricht

admin-needs-review-notice = Hinweis „Prüfung erforderlich“

admin-send-this-delivery-again = Diese Übermittlung erneut senden?

admin-for-example-the-lab-app-team-asked-for-it = Zum Beispiel: Das Team der Labor-App hat erneut darum gebeten

admin-required-up-to-256-characters-recorded-with-the-action = Erforderlich, bis zu 256 Zeichen. Wird mit der Aktion und der Uhrzeit festgehalten.

admin-queued-to-send-again = Zum erneuten Senden eingereiht

admin-the-delivery-is-queued-to-send-again = Die Übermittlung ist zum erneuten Senden eingereiht

admin-lab-app-order-version-sent-machine-order = Labor-App { $destination } · Auftragsversion { $version } gesendet · Geräteauftrag { $id }

admin-unknown = Unbekannt

admin-cancelled = Storniert

admin-no-more-orders = Keine weiteren Aufträge

admin-this-page-is-past-the-last-order = Diese Seite liegt hinter dem letzten Auftrag.

admin-no-orders-yet = Noch keine Aufträge

admin-orders-analyte-sends-from-a-lab-app-to-a = Aufträge, die Analyte aus einer Labor-App an ein Gerät sendet, werden hier aufgeführt.

admin-no-order-with-an-unknown-outcome = Kein Auftrag mit unbekanntem Ausgang

admin-every-order-sent-to-a-machine-has-a-known = Jeder an ein Gerät gesendete Auftrag hat einen bekannten Ausgang.

admin-show-all-orders = Alle Aufträge anzeigen

admin-no-cancelled-order = Kein stornierter Auftrag

admin-no-order-was-cancelled-by-the-lab-app-and = Kein Auftrag wurde von der Labor-App storniert und von ihrem Gerät bestätigt.

admin-review = Prüfen

admin-the-machine-cancelled-part-of-this-order = Das Gerät hat einen Teil dieses Auftrags storniert

admin-action = Aktion

admin-queued = Eingereiht

admin-from = Von

admin-to = An

admin-deleted-after-the-lab-app-last-confirmed-it = Gelöscht { $days } nach der letzten Bestätigung durch die Labor-App

admin-message-from-received = Nachricht von { $machine } · empfangen{ " " }

admin-order-from-lab-app = Auftrag { $order } von Labor-App { $destination }

admin-set-by = Gesetzt von { $actor }

admin-order-lab-app = Auftrag { $order } · Labor-App { $destination }

admin-clean-up-may-delete-it-once-its-keeping-time = { $plain }: Die Bereinigung darf es löschen, sobald seine Aufbewahrungsfrist abgelaufen ist. Der Verlauf der Löschsperre bleibt erhalten.

admin-oldest-message-kept = Älteste aufbewahrte Nachricht

admin-ran = Ausgeführt

admin-messages-checked = Geprüfte Nachrichten

admin-messages-deleted = Gelöschte Nachrichten

admin-orders-deleted = Gelöschte Aufträge

admin-paused = Angehalten

admin-failed = Fehlgeschlagen

admin-purge-failed = Löschen fehlgeschlagen

admin-a-message-no-longer-listed = Eine nicht mehr aufgeführte Nachricht

admin-until-released = bis zur Aufhebung

admin-who-placed-it-is-not-in-the-history-until = Wer sie gesetzt hat, steht nicht im Verlauf · bis zur Aufhebung

admin-release-this-hold = Diese Löschsperre aufheben?

admin-for-example-the-audit-is-finished = Zum Beispiel: Die Prüfung ist abgeschlossen

admin-required-up-to-256-characters-recorded-with-the-time = Erforderlich, bis zu 256 Zeichen. Wird mit der Uhrzeit festgehalten.

admin-tick-the-confirmation-box-and-make-sure-you-are = Aktivieren Sie das Bestätigungsfeld, stellen Sie sicher, dass Sie angemeldet sind, und versuchen Sie es erneut.

admin-authenticated-session-valid-form-and-confirmation-required = angemeldete Sitzung, gültiges Formular und Bestätigung erforderlich

admin-hold-released = Löschsperre aufgehoben

admin-hold-saved = Löschsperre gespeichert

admin-the-hold-is-released = Die Löschsperre ist aufgehoben

admin-the-hold-is-saved = Die Löschsperre ist gespeichert

admin-your-session-and-reason-are-recorded-with-it = Ihre Sitzung und Ihr Grund werden dazu festgehalten.

admin-lab-apps = Labor-Apps: { $why }

admin-of = { $open } von { $machines }

admin-the-lab-apps-settings-could-not-be-read-so = Die Einstellungen der Labor-Apps konnten nicht gelesen werden, daher werden ihre Schlüssel nicht aufgeführt

admin-analyte-version = Analyte-Version

admin-machine-links-open = Offene Geräteverbindungen

admin-admin-pages = Verwaltungsseiten

admin-reachable-from-this-pc-only = Nur von diesem PC erreichbar

admin-program-file = Programmdatei

admin-data-folder = Datenordner

admin-times-on-these-pages = Zeiten auf diesen Seiten

admin-windows-sign-in = Windows-Anmeldung

admin-disk-space-kept-free = Frei gehaltener Speicherplatz

admin-largest-database = Größte Datenbank

admin-largest-message-or-record = Größte Nachricht oder größter Datensatz

admin-wait-for-a-busy-store = Wartezeit bei belegtem Datenspeicher

admin-store-start-limit = Zeitgrenze für den Start des Datenspeichers

admin-shutdown-limit = Zeitgrenze für das Herunterfahren

admin-page-reads = Seitenaufrufe

admin-page-requests = Seitenanfragen

admin-setup-changes = Änderungen an der Einrichtung

admin-checked-every = Geprüft alle

admin-alert-targets = Warnungsziele

admin-none-set-up-alerts-show-on-these-pages-only = Keine eingerichtet; Warnungen erscheinen nur auf diesen Seiten

admin-reminder-every = Erinnerung alle

admin-wait-for-an-answer = Wartezeit auf eine Antwort

admin-tries-per-alert = Versuche je Warnung

admin-in-the-file-admin-token-in-the-data-folder = In der Datei admin-token im Datenordner

admin-read-the-reason-above-and-open-the-item-for = Lesen Sie den Grund oben und öffnen Sie den Eintrag, um seine verknüpften Datensätze zu sehen. Ist unklar, was zu tun ist, geben Sie die Eintragsnummer an den Support weiter.

admin-the-machine-sent-a-result-without-a-test-code = Das Gerät hat ein Ergebnis ohne Testcode gesendet. Prüfen Sie die Einstellungen des Geräts und wo die Definition dieses Geräts den Code liest, und verwenden Sie dann „Erneut verarbeiten“.

admin-the-value-does-not-fit-the-kind-this-machine = Der Wert passt nicht zu der Art, die die Definition dieses Geräts für diesen Testcode erwartet. Prüfen Sie diese Erwartung und die Einstellungen des Geräts, und verwenden Sie dann „Erneut verarbeiten“.

admin-the-message-has-no-sample-id-check-how-the = Die Nachricht hat keine Proben-ID. Prüfen Sie, wie das Gerät Proben-IDs sendet; sobald seine Einstellungen korrigiert sind, verwenden Sie „Erneut verarbeiten“.

admin-check-this-machine-s-definition-and-settings-against-the = Prüfen Sie die Definition und die Einstellungen dieses Geräts anhand des Grundes oben, und verwenden Sie dann „Erneut verarbeiten“.

admin-check-the-machine-s-settings-on-the-setup-page = Prüfen Sie die Einstellungen des Geräts auf der Einrichtungsseite (Gerätemodell und Verbindung) anhand des Grundes oben, und verwenden Sie dann „Erneut verarbeiten“.

admin-compare-this-result-with-the-earlier-results-for-the = Vergleichen Sie dieses Ergebnis mit den früheren Ergebnissen derselben Probe am Gerät. Hat eine Einstellung die Abweichung verursacht, korrigieren Sie sie und verwenden Sie dann „Erneut verarbeiten“.

admin-the-machine-stopped-part-way-through-sending-check-the = Das Gerät hat mitten im Senden aufgehört. Prüfen Sie Kabel oder Netzwerk, lassen Sie das Gerät die Ergebnisse erneut senden und verwenden Sie dann hier „Erneut verarbeiten“.

admin-the-lab-app-already-holds-different-content-for-this = Die Labor-App hat für diese Übermittlung bereits einen anderen Inhalt; Analyte sendet sie nicht von sich aus erneut. Prüfen Sie die Übermittlung in der Labor-App und klären Sie sie dort, dann „Als geklärt markieren“.

admin-the-lab-app-sent-an-older-or-changed-copy = Die Labor-App hat eine ältere oder geänderte Kopie eines Auftrags gesendet, den Analyte bereits hat; Analyte hat seine Kopie behalten. Korrigieren Sie den Auftrag in der Labor-App, dann „Als geklärt markieren“.

admin-the-lab-app-cancelled-tests-the-machine-has-already = Die Labor-App hat Tests storniert, zu denen das Gerät bereits Ergebnisse gemeldet hat; Analyte sendet dem Gerät nichts weiter. Prüfen Sie beides am Gerät und in der Labor-App, dann „Als geklärt markieren“.

admin-analyte-starts-no-other-exchange-with-this-machine-until = Analyte beginnt mit diesem Gerät keinen weiteren Austausch, bis dieser Eintrag geklärt ist. Prüfen Sie am Gerät, ob dieser Austausch abgeschlossen wurde, dann „Als geklärt markieren“.

admin-analyte-did-not-send-the-lab-app-this-order = Analyte hat der Labor-App die Änderung dieses Auftrags nicht gesendet, weil der Auftrag älter ist als das Gerätemodell, das Analyte jetzt in jedem Ereignis nennt. Teilen Sie dem Team der Labor-App den hier angezeigten Stand des Auftrags mit, dann „Als geklärt markieren“.

admin-test-or-unit-not-mapped = Test oder Einheit nicht zugeordnet

admin-could-not-read-message = Nachricht konnte nicht ausgewertet werden

admin-result-could-not-be-matched-to-earlier-results = Ergebnis konnte früheren Ergebnissen nicht zugeordnet werden

admin-message-from-the-machine-was-incomplete = Nachricht des Geräts war unvollständig

admin-lab-app-rejected-a-delivery = Labor-App hat eine Übermittlung abgelehnt

admin-lab-app-reported-a-conflicting-delivery = Labor-App hat eine widersprüchliche Übermittlung gemeldet

admin-order-from-the-lab-app-was-refused = Auftrag aus der Labor-App wurde abgelehnt

admin-cancel-conflicts-with-what-the-machine-reported = Stornierung widerspricht dem, was das Gerät gemeldet hat

admin-order-event-could-not-name-its-machine-model = Auftragsereignis konnte sein Gerätemodell nicht nennen

admin-settle-the-problem-described-above-with-the-lab-app = Klären Sie das oben beschriebene Problem mit dem Team der Labor-App und verwenden Sie dann „Erneut senden“.

admin-the-item-leaves-the-open-list-and-stays-under = Der Eintrag verlässt die Liste der offenen Einträge und bleibt mit seinem Verlauf unter „Geklärt“. Es wird nichts gelöscht und nichts an ein Gerät oder die Labor-App gesendet.

admin-analyte-reads-the-original-message-again-with-the-current = Analyte wertet die Originalnachricht mit den aktuellen Einstellungen erneut aus. Die frühere Auswertung bleibt erhalten und eine neue kommt hinzu; korrigierte Ergebnisse folgen den üblichen Korrekturregeln. An das Gerät wird nichts gesendet.

admin-item-is-marked-resolved = Eintrag { $id } ist als geklärt markiert

admin-it-moved-to-resolved-and-keeps-its-history-nothing = Er wurde nach „Geklärt“ verschoben und behält seinen Verlauf. An ein Gerät oder die Labor-App wurde nichts gesendet.

admin-reading-was-recorded = Auswertung { $version } wurde aufgezeichnet

admin-what-happened-the-earlier-reading-is-kept = Was geschah: { $outcomes }. Die frühere Auswertung bleibt erhalten.

admin-mark-as-resolved = Als geklärt markieren?

admin-for-example-settled-with-the-lab-app-team = Zum Beispiel: mit dem Team der Labor-App geklärt

admin-process-this-message-again = Diese Nachricht erneut verarbeiten?

admin-for-example-machine-definition-corrected = Zum Beispiel: Gerätedefinition korrigiert

admin-required-up-to-256-characters-recorded-in-the-history = Erforderlich, bis zu 256 Zeichen. Wird mit der Uhrzeit im Verlauf festgehalten.

admin-no-eligible-measurement-comparison-is-available = Es ist kein zulässiger Messwertvergleich verfügbar.

admin-the-retained-results-could-not-be-displayed = Die aufbewahrten Ergebnisse konnten nicht angezeigt werden.

admin-when-analyte-cannot-handle-a-message-or-result-on = Wenn Analyte eine Nachricht oder ein Ergebnis nicht selbst verarbeiten kann, bewahrt es sie hier auf, zusammen mit den nächsten Schritten.

admin-when-someone-resolves-this-item-or-processes-it-again = Wenn jemand diesen Eintrag klärt oder erneut verarbeitet, erscheinen hier die Uhrzeit, die Aktion und der Grund.

admin-linked-records-to-of = Verknüpfte Datensätze { $saturating-add } bis { $end } von { $total }.

admin-linked-record-2 = Verknüpfter Datensatz { $number }

admin-arrived-from = Eingegangen von { $machine }

admin-category-as-stored = Kategorie wie gespeichert: { $kind }

admin-linked-record-3 = Verknüpfter Datensatz: { $target }

admin-recorded-at-unix-by = Aufgezeichnet bei Unix { $time }: { $action } durch { $actor }

admin-resolution-as-stored = Klärung wie gespeichert: { $stored }

admin-resolved-today = Heute geklärt

admin-open = Offen

admin-resolved = Geklärt

admin-no-more-items = Keine weiteren Einträge

admin-this-page-is-past-the-last-item = Diese Seite liegt hinter dem letzten Eintrag.

admin-nothing-needs-review = Nichts erfordert eine Prüfung.

admin-see-resolved-items = Geklärte Einträge ansehen

admin-no-resolved-items-yet = Noch keine geklärten Einträge.

admin-items-marked-resolved-or-processed-again-move-here-and = Als geklärt markierte oder erneut verarbeitete Einträge werden hierher verschoben und behalten ihren Verlauf.

admin-see-open-items = Offene Einträge ansehen

admin-resolved-how-it-was-resolved-could-not-be-read = Geklärt; wie er geklärt wurde, konnte nicht gelesen werden

admin-older-entries-are-kept-but-not-shown-here = Ältere Einträge werden aufbewahrt, aber hier nicht angezeigt.

admin-item-to-review = Zu prüfender Eintrag

admin-none-recorded = Keine aufgezeichnet

admin-removed-by-data-keeping = Durch die Datenaufbewahrung entfernt

admin-not-a-stored-message-its-id-is-under-details = Keine gespeicherte Nachricht; ihre ID steht unter „Details für den Support“

admin-show-the-message-as-stored = Nachricht wie gespeichert anzeigen

admin-message-removed-by-data-keeping = Nachricht durch die Datenaufbewahrung entfernt

admin-analyte-stored-the-message = Analyte hat die Nachricht gespeichert

admin-how-it-was-resolved-could-not-be-read-the = Wie er geklärt wurde, konnte nicht gelesen werden; der gespeicherte Datensatz steht unter „Details für den Support“.

admin-source-cursor-offset = Quelle: { $source }, Cursor { $after }, Offset { $offset }

admin-view-recorded-for = Ansicht festgehalten für: { $actor }

admin-the-message-as-stored = Die Nachricht wie gespeichert

admin-the-pieces-as-they-arrived-from-the-machine = Die Teile, wie sie vom Gerät ankamen

admin-show-the-next-part = Nächsten Teil anzeigen

admin-admin-connection-lifetime-exceeded = maximale Dauer der Verwaltungsverbindung überschritten

admin-its-words-could-not-be-read-within-bytes = sein Text konnte nicht innerhalb von { $PLAIN-LIMIT } Byte gelesen werden: { $error }

admin-csrf-session-required = CSRF-Sitzung erforderlich

admin-csrf-token-invalid-or-missing = CSRF-Token ungültig oder fehlend

admin-sign-in-page-failed = Anmeldeseite fehlgeschlagen: { $render }

admin-this-is-not-the-admin-key-check-it-and = Das ist nicht der Verwaltungsschlüssel. Prüfen Sie ihn und versuchen Sie es erneut.

admin-analyte-could-not-read-the-form-enter-the-key = Analyte konnte das Formular nicht lesen. Geben Sie den Schlüssel erneut ein.

admin-windows-sign-in-did-not-work = Die Windows-Anmeldung hat nicht funktioniert

admin-the-form-was-sent-without-a-signed-in-session = Das Formular wurde ohne angemeldete Sitzung gesendet. Melden Sie sich an, öffnen Sie die Seite erneut und wiederholen Sie Ihre Aktion.

admin-your-windows-account-is-not-allowed-to-use-analyte = Ihr Windows-Konto darf Analyte nicht verwenden. Bitten Sie eine Administratorin oder einen Administrator, es der Gruppe der Analyte-Benutzer hinzuzufügen, oder melden Sie sich mit dem Verwaltungsschlüssel an.

admin-the-analyte-app-asked-in-a-way-this-engine = Die Analyte-App hat auf eine Weise angefragt, die dieser Dienst nicht versteht. Melden Sie sich mit dem Verwaltungsschlüssel an und bitten Sie die IT, die App zu aktualisieren.

admin-windows-could-not-say-which-account-is-asking-sign = Windows konnte nicht mitteilen, welches Konto anfragt. Melden Sie sich mit dem Verwaltungsschlüssel an.

admin-windows-sign-in-is-not-available-right-now-try = Die Windows-Anmeldung ist im Moment nicht verfügbar. Versuchen Sie es gleich noch einmal oder melden Sie sich mit dem Verwaltungsschlüssel an.

admin-this-sign-in-has-expired = Diese Anmeldung ist abgelaufen

admin-a-windows-sign-in-works-once-within-20-seconds = Eine Windows-Anmeldung funktioniert einmal, innerhalb von 20 Sekunden. Öffnen Sie die Analyte-App erneut oder melden Sie sich mit dem Verwaltungsschlüssel an.

admin-windows-sign-in-works-in-the-analyte-app = Die Windows-Anmeldung funktioniert in der Analyte-App

admin-in-a-browser-sign-in-with-the-admin-key = Melden Sie sich in einem Browser mit dem Verwaltungsschlüssel an.

admin-the-sign-in-page-could-not-be-shown-give = Die Anmeldeseite konnte nicht angezeigt werden. Geben Sie die Details unten an den Support weiter.

admin-overview = Übersicht

admin-activity = Aktivität

admin-needs-review = Prüfung erforderlich

admin-machines = Geräte

admin-lab-apps-2 = Labor-Apps

admin-setup = Einrichtung

admin-machine-orders = Geräteaufträge

admin-cancelled-by-machine = Vom Gerät storniert

admin-deliveries = Übermittlungen

admin-data-keeping = Datenaufbewahrung

admin-settings = Einstellungen

admin-back-to-overview = Zurück zur Übersicht

admin-back-to-activity = Zurück zu „Aktivität“

admin-back-to-needs-review = Zurück zu „Prüfung erforderlich“

admin-back-to-machines = Zurück zu „Geräte“

admin-back-to-lab-apps = Zurück zu „Labor-Apps“

admin-back-to-setup = Zurück zur Einrichtung

admin-back-to-machine-orders = Zurück zu „Geräteaufträge“

admin-back-to-cancelled-by-machine = Zurück zu „Vom Gerät storniert“

admin-back-to-deliveries = Zurück zu „Übermittlungen“

admin-back-to-data-keeping = Zurück zu „Datenaufbewahrung“

admin-back-to-settings = Zurück zu „Einstellungen“

admin-good = In Ordnung

admin-problem = Problem

admin-note = Hinweis

admin-is-not-allowed-at-this-address = { $method } ist unter dieser Adresse nicht erlaubt

admin-details-for-support-the-page-failed-too = { $title }{ "\u000A" }{ "\u000A" }{ $text }{ "\u000A" }{ "\u000A" }Details für den Support: { $detail }; auch die Seite ist fehlgeschlagen: { $render }

admin-go-to-overview = Zur Übersicht

admin-try-again = Erneut versuchen

admin-page-not-found = Seite nicht gefunden

admin-analyte-has-no-page-at-this-address-check-the = Analyte hat unter dieser Adresse keine Seite. Prüfen Sie die Adresse oder gehen Sie zur Übersicht.

admin-file-not-found = Datei nicht gefunden

admin-analyte-has-no-file-at-this-address = Analyte hat unter dieser Adresse keine Datei.

admin-this-cannot-be-done-here = Das ist hier nicht möglich

admin-analyte-does-not-take-this-kind-of-request-at = Analyte nimmt diese Art von Anfrage unter dieser Adresse nicht an. Es wurde nichts geändert.

admin-this-could-not-be-read = Das konnte nicht gelesen werden

admin-analyte-could-not-read-the-address-or-the-form = Analyte konnte die Adresse oder das gesendete Formular nicht lesen. Es wurde nichts geändert. Gehen Sie zurück und versuchen Sie es erneut.

admin-this-form-has-expired = Dieses Formular ist abgelaufen

admin-go-back-reload-the-page-and-try-again-nothing = Gehen Sie zurück, laden Sie die Seite neu und versuchen Sie es erneut. Es wurde nichts geändert.

admin-too-much-text = Zu viel Text

admin-the-form-holds-more-text-than-analyte-accepts-shorten = Das Formular enthält mehr Text, als Analyte annimmt. Kürzen Sie Ihre Eingabe und versuchen Sie es erneut.

admin-this-took-too-long = Das hat zu lange gedauert

admin-analyte-did-not-finish-in-time-check-the-page = Analyte ist nicht rechtzeitig fertig geworden. Prüfen Sie die Seite, bevor Sie es erneut versuchen: Eine Aktion wurde möglicherweise bereits gespeichert.

admin-analyte-needs-a-restart = Analyte muss neu gestartet werden

admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte = Analyte kann Browsersitzungen nicht mehr prüfen. Starten Sie Analyte neu und melden Sie sich dann erneut an.

admin-you-cannot-sign-in-right-now = Sie können sich im Moment nicht anmelden

admin-too-many-browser-sessions-are-open-wait-a-few = Es sind zu viele Browsersitzungen offen. Warten Sie einige Minuten und versuchen Sie es erneut, oder starten Sie Analyte neu.

admin-analyte-could-not-start-a-browser-session-try-again = Analyte konnte keine Browsersitzung starten. Versuchen Sie es erneut; wenn das wiederholt passiert, geben Sie die Details unten an den Support weiter.

admin-you-cannot-sign-out-right-now = Sie können sich im Moment nicht abmelden

admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte-2 = Analyte kann Browsersitzungen nicht mehr prüfen. Starten Sie Analyte neu; dadurch werden alle abgemeldet.

admin-too-many-live-pages = Zu viele Live-Seiten

admin-analyte-keeps-a-few-pages-live-at-once-and = Analyte hält nur wenige Seiten gleichzeitig live, und alle sind belegt. Die Seiten funktionieren weiterhin; laden Sie eine neu, um Änderungen zu sehen, oder schließen Sie einen Tab.

admin-no-admin-page-at-this-address = keine Verwaltungsseite unter dieser Adresse

admin-static-asset-not-found = statische Datei nicht gefunden

admin-admin-form-exceeds-limit = Verwaltungsformular überschreitet die Grenze

admin-admin-request-deadline-exceeded = Frist der Verwaltungsanfrage überschritten

admin-admin-session-lookup-failed = Nachschlagen der Verwaltungssitzung fehlgeschlagen

admin-admin-session-header-invalid = Header der Verwaltungssitzung ungültig

admin-every-admin-live-stream-is-in-use = alle Live-Datenströme der Verwaltung sind belegt

admin-page-could-not-be-written = die Seite konnte nicht geschrieben werden: { $error }

admin-check-the-item-and-try-again-if-this-keeps = Prüfen Sie den Eintrag und versuchen Sie es erneut; wenn das wiederholt passiert, geben Sie die Details unten an den Support weiter.

admin-process-again-reads-the-item-s-original-message-so = „Erneut verarbeiten“ wertet die Originalnachricht des Eintrags aus und braucht daher genau eine. Sobald der Eintrag geklärt ist, verwenden Sie „Als geklärt markieren“.

admin-admin-deadline-of-ms-exceeded = Verwaltungsfrist { $label } von { $as-millis } ms überschritten

admin-the-page-failed-too = { $alert }; auch die Seite ist fehlgeschlagen: { $render }

admin-open-the-lab-app = Labor-App öffnen

admin-open-its-setup = Einrichtung öffnen

admin-item-not-found = Eintrag nicht gefunden

admin-there-is-no-item-with-this-number = Es gibt keinen Eintrag mit dieser Nummer

admin-go-back-to-needs-review-and-pick-one-from = Gehen Sie zurück zu „Prüfung erforderlich“ und wählen Sie einen aus der Liste.

admin-parked-item-not-found = zurückgehaltener Eintrag nicht gefunden

admin-machine-not-found = Gerät nicht gefunden

admin-no-machine-is-called-this = Kein Gerät hat diesen Namen

admin-go-back-to-machines-and-pick-one-from-the = Gehen Sie zurück zu „Geräte“ und wählen Sie eines aus der Liste.

admin-machine-not-running-and-not-listed = Gerät läuft nicht und ist nicht aufgeführt

admin-this-needs-a-signed-in-browser-session = Dafür ist eine angemeldete Browsersitzung erforderlich

admin-removing-a-machine-changes-settings-so-it-needs-a = Das Entfernen eines Geräts ändert Einstellungen und erfordert daher eine angemeldete Browsersitzung

admin-lab-apps-hold-addresses-and-keys-so-their-pages = Labor-Apps enthalten Adressen und Schlüssel, daher erfordern ihre Seiten eine angemeldete Browsersitzung

admin-setup-changes-settings-so-its-pages-need-a-signed = Die Einrichtung ändert Einstellungen, daher erfordern ihre Seiten eine angemeldete Browsersitzung

admin-analyte-records-who-sends-again-or-changes-a-hold = Analyte hält fest, wer erneut sendet oder eine Löschsperre ändert, daher ist eine angemeldete Browsersitzung erforderlich

admin-the-overview-could-not-be-shown-just-now = Die Übersicht konnte gerade nicht angezeigt werden

admin-reading-not-found = Auswertung nicht gefunden

admin-readings-are-numbered-from-1 = Auswertungen werden ab 1 nummeriert

admin-open-the-message-again-from-its-machine-s-page = Öffnen Sie die Nachricht erneut von der Seite ihres Geräts aus.

admin-this-message-has-no-reading-with-this-number = Diese Nachricht hat keine Auswertung mit dieser Nummer

admin-its-machine-s-page-lists-its-messages-with-their = Die Seite ihres Geräts listet seine Nachrichten mit ihrer letzten Auswertung auf.

admin-sign-in-first = Melden Sie sich zuerst an

admin-sign-in-with-the-admin-key-then-open-the = Melden Sie sich mit dem Verwaltungsschlüssel an und öffnen Sie die Seite dann erneut.

admin-this-form-is-from-an-older-or-another-session = Dieses Formular stammt aus einer älteren oder anderen Sitzung

admin-open-the-item-again-and-repeat-the-action-if = Öffnen Sie den Eintrag erneut und wiederholen Sie die Aktion; wenn die Seite danach fragt, melden Sie sich zuerst an.

admin-that-could-not-be-done = Das konnte nicht ausgeführt werden

admin-already-resolved = Bereits geklärt

admin-this-item-is-already-resolved = Dieser Eintrag ist bereits geklärt

admin-its-history-on-the-item-page-says-when-it = Sein Verlauf auf der Eintragsseite zeigt, wann, wie und warum er geklärt wurde.

admin-cannot-process-this-item-again = Dieser Eintrag kann nicht erneut verarbeitet werden

admin-this-item-is-not-linked-to-exactly-one-stored = Dieser Eintrag ist nicht mit genau einer gespeicherten Nachricht verknüpft

admin-the-original-message-is-no-longer-kept = Die Originalnachricht wird nicht mehr aufbewahrt

admin-data-keeping-removed-the-original-message = Die Datenaufbewahrung hat die Originalnachricht entfernt

admin-it-cannot-be-shown-or-read-again-the-item = Sie kann nicht mehr angezeigt oder ausgewertet werden. Der Eintrag und sein Verlauf bleiben erhalten.

admin-nothing-more-to-show = Nichts weiter anzuzeigen

admin-there-are-no-more-original-bytes-for-this-record = Für diesen Datensatz gibt es keine weiteren Originalbytes

admin-go-back-to-the-item-to-see-its-other = Gehen Sie zurück zum Eintrag, um seine anderen verknüpften Datensätze zu sehen.

admin-analyte-could-not-read-its-settings-just-now = Analyte konnte seine Einstellungen gerade nicht lesen

admin-analyte-could-not-read-its-stored-data-just-now = Analyte konnte seine gespeicherten Daten gerade nicht lesen

admin-analyte-took-too-long = Analyte hat zu lange gebraucht

admin-positive-interpretation-version-required = positive Interpretationsversion erforderlich

admin-interpretation-not-found = Interpretation nicht gefunden

admin-authenticated-session-required = angemeldete Sitzung erforderlich

admin-authenticated-csrf-session-required = angemeldete CSRF-Sitzung erforderlich

admin-parked-item-already-resolved = zurückgehaltener Eintrag bereits geklärt

admin-linked-receipt-purged-by-retention = verknüpfter Empfang durch die Aufbewahrungsregel gelöscht

admin-no-linked-raw-evidence-at-this-cursor = keine verknüpften Rohdaten an dieser Cursorposition

admin-this-page-could-not-be-shown-give-the-details = Diese Seite konnte nicht angezeigt werden. Geben Sie die Details unten an den Support weiter.

admin-item-added = – Eintrag { $item } hinzugefügt: { $title }

admin-item-3 = – Eintrag { $item } { $action }

admin-order-message-2 = – Auftrag { $order }, Nachricht{ " " }

admin-order-3 = – Auftrag { $order }{ " " }

admin-sample-2 = – Probe { $sample }{ " " }

admin-sample-3 = – Probe { $shown }{ " " }

admin-delivered-to = übermittelt an { $lab-app }

admin-sending-to-failed-will-retry = Senden an { $lab-app } fehlgeschlagen, wird erneut versucht

admin-sending-to-was-cut-off-will-retry = Senden an { $lab-app } wurde unterbrochen, wird erneut versucht

admin-rejected-by = abgelehnt von { $lab-app }

admin-sending-to-paused-lab-app-settings-need-repair = Senden an { $lab-app } angehalten: Die Einstellungen der Labor-App müssen korrigiert werden

admin-reports-a-conflict = { $lab-app } meldet einen Konflikt

admin-sending-to-tried-times =
    { $failed ->
        [failed] Senden an { $lab-app } fehlgeschlagen · { $count }-mal versucht, { $first } – { $last }
       *[other] Senden an { $lab-app } { $failed } · { $count }-mal versucht, { $first } – { $last }
    }

admin-next-try-after = { " " }· nächster Versuch nach { $next }

admin-added-machine = Gerät { $named } hinzugefügt

admin-saved-the-settings-of = Einstellungen von { $named } gespeichert

admin-removed-machine-2 = Gerät { $named } entfernt

admin-confirmed-the-settings-of-for-its-reported-software = Einstellungen von { $named } für seine gemeldete Software bestätigt

admin-added-lab-app = Labor-App { $named } hinzugefügt

admin-saved-lab-app = Labor-App { $named } gespeichert

admin-removed-lab-app = Labor-App { $named } entfernt

admin-recorded-3 = { $action } aufgezeichnet

admin-every-try = Jeder Versuch

admin-change = Änderung

admin-stored = Gespeichert

admin-message-stored = Nachricht gespeichert

admin-message-stored-could-not-be-read = Nachricht gespeichert, konnte nicht ausgewertet werden

admin-not-read-yet = Noch nicht ausgewertet

admin-message-stored-not-read-yet = Nachricht gespeichert, noch nicht ausgewertet

admin-no-results-2 = keine Ergebnisse

admin-was-cut-off = wurde unterbrochen

admin-not-recorded-2 = (nicht aufgezeichnet)

admin-asked-analyte-to-restart = Neustart von Analyte angefordert

admin-placed-a-hold-under-data-keeping = Löschsperre unter „Datenaufbewahrung“ gesetzt

admin-released-a-hold-under-data-keeping = Löschsperre unter „Datenaufbewahrung“ aufgehoben

admin-removed-data-past-its-keeping-time = Daten nach Ablauf ihrer Aufbewahrungsfrist entfernt

admin-queued-a-delivery-to-send-again = Übermittlung zum erneuten Senden eingereiht

admin-processed-a-message-again = Nachricht erneut verarbeitet

admin-recovered-an-interrupted-delivery = Unterbrochene Übermittlung wiederhergestellt

admin-restored-the-store-from-a-backup = Datenspeicher aus einer Sicherung wiederhergestellt

admin-settled-an-order-s-outcome = Ausgang eines Auftrags geklärt

admin-retried-an-alert = Warnung erneut versucht

admin-reading-recorded = : Auswertung { $version } aufgezeichnet

admin-by-at = { $what } durch { $actor } um{ " " }

admin-from-arrived = Von { $machine } · eingegangen{ " " }

admin-admin-session = Verwaltungssitzung

admin-it-still-needs-review = ; Prüfung weiterhin erforderlich

admin-recorded-as-a-resend = Als erneute Übertragung festgehalten

admin-rerun-queued = Wiederholungsmessung eingereiht

admin-superseded-by-the-receipt-with-a-tube-id = Ersetzt durch den Empfang mit Röhrchen-ID

admin-no-results-in-it = keine Ergebnisse darin

admin-oldest = { $items } · ältester{ " " }

admin-oldest-not-recorded = { $items } · ältester nicht aufgezeichnet

admin-key-missing = Schlüssel fehlt: { $one }.

admin-keys-missing-and = Schlüssel fehlen: { $join } und { $last }.

admin-set-at = Gesetzt am { $set-at }

admin-set-in-the-service-environment = In der Umgebung des Dienstes gesetzt ({ $variable })

admin-last-delivered = { "Zuletzt übermittelt " }

admin-no-delivery-in-the-kept-records = Keine Übermittlung in den aufbewahrten Datensätzen

admin-nothing-delivered-yet = Noch nichts übermittelt

admin-last-try = { "Letzter Versuch " }

admin-next-try = { "nächster Versuch " }

admin-next-try-due = nächster Versuch fällig

admin-a-try-is-under-way = ein Versuch läuft

admin-no-set-up-machine-sends-here = Kein eingerichtetes Gerät sendet hierher

admin-its-keys-are-set-now-analyte-uses-them-after = Ihre Schlüssel sind jetzt gesetzt; Analyte verwendet sie nach einem Neustart.

admin-set-on-date = Gesetzt am { $day }. { $month }

admin-set-on-date-year = Gesetzt am { $day }. { $month } { $year }

admin-scheduled-on-the-machine = Am Gerät eingeplant

admin-in-progress-on-the-machine = Am Gerät in Bearbeitung

admin-cancelled-by-the-machine = Vom Gerät storniert

admin-not-sent-yet = Noch nicht gesendet

admin-sent-not-yet-accepted = Gesendet, noch nicht angenommen

admin-accepted-by-the-machine = Vom Gerät angenommen

admin-rejected-by-the-machine = Vom Gerät abgelehnt

admin-cancel-requested = Stornierung angefordert

admin-outcome-unknown-check-the-machine = Ausgang unbekannt: Gerät prüfen

admin-delivered = Übermittelt

admin-rejected-by-the-lab-app = Von der Labor-App abgelehnt

admin-paused-lab-app-settings-need-repair = Angehalten: Die Einstellungen der Labor-App müssen korrigiert werden

admin-lab-app-reports-a-conflict = Labor-App meldet einen Konflikt

admin-not-sent-the-lab-app-takes-no-notices = Nicht gesendet: Die Labor-App nimmt keine Hinweise an

admin-will-retry = Wird erneut versucht

admin-serial = seriell { $serial }

admin-port = Port { $port }

admin-kept-since-the-start-export-the-recordings-with-analyte = { $messages } aufbewahrt, { $exchanges } seit dem Start; exportieren Sie die Aufzeichnungen mit analyte captures export

admin-refused-connections-from-the-last-at = { $count } Verbindungen von { $peer } abgelehnt, die letzte um { $at }

admin-refused-a-connection-from-at = Eine Verbindung von { $peer } um { $at } abgelehnt

admin-leaves-a-report-s-status-empty-when-the-report = Lässt den Status eines Berichts leer, wenn der Bericht endgültig ist; Analyte wertet nur einen leeren Berichtsstatus als endgültig.

admin-leaves-a-result-s-status-empty-when-the-result = Lässt den Status eines Ergebnisses leer, wenn das Ergebnis endgültig ist; Analyte wertet nur einen leeren Ergebnisstatus als endgültig.

admin-puts-several-records-in-one-frame-analyte-reads-each = Packt mehrere Datensätze in einen Frame; Analyte wertet jeden Datensatz aus, nachdem die Prüfung des Frames bestanden ist.

admin-marks-its-text-as-utf-8-a-name-its = Kennzeichnet seinen Text als UTF-8, eine Bezeichnung, die seine HL7-Version noch nicht kannte; Analyte liest den Text als UTF-8.

admin-uses-escape-codes-analyte-does-not-translate-analyte-keeps = Verwendet Escape-Codes, die Analyte nicht übersetzt; Analyte bewahrt sie wie geschrieben auf und verwendet sie nie, um eine Probe oder ein Ergebnis zu identifizieren.

admin-sends-frames-of-a-size-other-than-the-standard = Sendet Frames in einer anderen Größe als im Standard; Analyte nimmt Frames bis zur Grenze in den Einstellungen dieses Geräts an.

admin-while-the-machine-sends-analyte-waits-for-each-part = Während das Gerät sendet, wartet Analyte auf jeden Teil so lange, wie die Einstellungen dieses Geräts angeben, nicht so lange wie der Standardwert des Standards.

admin-while-analyte-sends-it-waits-for-the-machine-s = Während Analyte sendet, wartet es auf die Antworten des Geräts so lange, wie die Einstellungen dieses Geräts angeben, nicht so lange wie der Standardwert des Standards.

admin-writes-check-digits-in-lowercase-analyte-accepts-them-only = Schreibt Prüfziffern in Kleinbuchstaben; Analyte nimmt sie nur an, wenn die Prüfung korrekt ist.

admin-of-open = { $open } von { $links } offen

admin-of-delivered = { " " }· { $done } von { $total } übermittelt

admin-delivered-2 = { $total } übermittelt

admin-port-open-since = Port { $port } offen seit { $since }

admin-port-not-open-since = Port { $port } nicht offen seit { $since }

admin-port-of-usb-adapter-open-since = Port des USB-Adapters { $serial } offen seit { $since }

admin-port-of-usb-adapter-not-open-since = Port des USB-Adapters { $serial } nicht offen seit { $since }

admin-machine-connected-since = Gerät verbunden{ $port } seit { $since }

admin-listening-nothing-connected-since = Lauscht{ $port }, nichts verbunden seit { $since }

admin-not-listening-since = Lauscht nicht{ $port } seit { $since }

admin-connected-to-since = Verbunden mit { $machine } seit { $since }

admin-no-connection-to-since = Keine Verbindung zu { $machine } seit { $since }

admin-watching = Überwacht { $folder }

admin-not-watching-since = Überwacht { $folder } nicht seit { $since }

admin-on-port = { " " }an Port { $port }

admin-check-that-the-usb-adapter-or-cable-is-plugged = Prüfen Sie, ob der USB-Adapter oder das Kabel an diesem PC angeschlossen ist, ob { $port } die richtige Schnittstelle ist und ob kein anderes Programm sie verwendet.

admin-check-that-the-usb-adapter-with-serial-is-plugged = Prüfen Sie, ob der USB-Adapter mit der Seriennummer { $serial } an diesem PC angeschlossen ist und ob kein anderes Programm seine Schnittstelle verwendet.

admin-only = Nur { $addresses }

admin-serial-2 = Seriell

admin-usb-adapter = USB-Adapter

admin-network = Netzwerk

admin-watched-folder = Überwachter Ordner

admin-empty-h-12-in-a-documented-upload-is-treated = Ein leeres H.12 in einem dokumentierten Upload wird als Produktion behandelt.

admin-one-repeated-enq-before-any-frame-can-recover-a = Ein wiederholtes ENQ vor dem ersten Frame kann ein verlorenes ACK beim Verbindungsaufbau ausgleichen.

admin-answer-record-format-hl7-barcode-queries = HL7-Barcode-Abfragen im Datensatzformat beantworten

admin-reads-combined-patient-values-and-divides-control-uploads-into = Liest zusammengefasste Patientenwerte und teilt Kontroll-Uploads in ihre ursprünglichen Zeilen auf.

admin-puts-the-upload-category-in-msh-16-analyte-reads = Setzt die Upload-Kategorie in MSH-16. Analyte liest diese Kategorie bei Ergebnis-Uploads.

admin-ends-each-message-line-with-cr-lf-instead-of = Beendet jede Nachrichtenzeile mit CR LF statt CR; Analyte liest die Zeilen entsprechend.

admin-ends-each-message-line-with-lf-instead-of-cr = Beendet jede Nachrichtenzeile mit LF statt CR; Analyte liest die Zeilen entsprechend.

admin-when-both-sides-start-at-the-same-moment-analyte = Wenn beide Seiten im selben Moment beginnen, geht Analyte statt des Geräts zuerst vor.

admin-found-by-its-usb-serial-number-so-it-keeps = Wird an seiner USB-Seriennummer erkannt und funktioniert daher weiter, wenn Windows dem Adapter einen anderen Schnittstellennamen gibt.

admin-last-data-2 = { "letzte Daten " }

admin-last-data-3 = { "Letzte Daten " }

admin-nothing-received-yet-2 = noch nichts empfangen

admin-could-not-be-read = Konnte nicht ausgewertet werden

admin-read-no-results = Ausgewertet · keine Ergebnisse

admin-check-the-network-cable-and-the-machine-s-host = Prüfen Sie das Netzwerkkabel und die Host-Einstellungen des Geräts.

admin-check-that-the-folder-exists-and-that-analyte-s = Prüfen Sie, ob der Ordner existiert und ob das Dienstkonto von Analyte ihn lesen darf.

admin-lab-app-answer-http = Antwort der Labor-App: HTTP { $status }

admin-record = Datensatz { $record }

admin-record-field-2 = Datensatz { $record }, Feld { $field }

admin-the-machine-sent-no-test-code-in = Das Gerät hat in { $at } keinen Testcode gesendet.

admin-the-message-has-no-sample-id-in = Die Nachricht hat in { $at } keine Proben-ID.

admin-the-value-this-machine-s-settings-use-to-tell = Der Wert, mit dem die Einstellungen dieses Geräts Ergebnisse unterscheiden, fehlt in { $at }.

admin-the-value-in-is-of-a-kind-analyte-cannot = Der Wert in { $at } ist von einer Art, die Analyte nicht sicher lesen kann.

admin-the-processing-mode-in-is-missing-or-not-one = Der Verarbeitungsmodus in { $at } fehlt oder ist Analyte nicht bekannt.

admin-the-time-in-could-not-be-read-the-text = Die Zeit in { $at } konnte nicht gelesen werden; der Text wird wie gesendet aufbewahrt.

admin-the-result-s-final-status-comes-from-what-this = Der endgültige Status des Ergebnisses stammt aus dem, was dieses Gerät laut Dokumentation sendet, nicht aus der Nachricht, und der verwendete Labor-App-Vertrag kann das nicht ausdrücken.

admin-part-of-this-message-needs-review-and-this-machine = Ein Teil dieser Nachricht erfordert eine Prüfung, und die Ergebnisse dieses Geräts werden nur alle zusammen oder gar nicht aufgezeichnet, daher wurde keines davon aufgezeichnet.

admin-results-in-this-message-conflict-with-results-this-machine = Ergebnisse in dieser Nachricht widersprechen Ergebnissen, die dieses Gerät früher gesendet hat, daher wurde keines davon aufgezeichnet.

admin-the-connection-timed-out-or-closed-before-the-message = Die Verbindung ist abgelaufen oder wurde geschlossen, bevor die Nachricht vollständig war; der angekommene Teil wird aufbewahrt.

admin-the-message-arrived-while-too-many-messages-were-waiting = Die Nachricht kam an, während zu viele Nachrichten auf das Speichern warteten, daher hat Analyte sie hier aufbewahrt, ohne sie auszuwerten.

admin-the-machine-sent-an-edited-result-whose-content-looks = Das Gerät hat ein bearbeitetes Ergebnis gesendet, dessen Inhalt unverändert aussieht, daher kann Analyte nicht erkennen, was bearbeitet wurde.

admin-the-machine-answered-that-it-could-not-cancel-a = Das Gerät hat geantwortet, dass es einen Test nicht stornieren konnte, den es zuvor selbst als storniert gemeldet hatte.

admin-this-order-was-stored-before-analyte-kept-each-order = Dieser Auftrag wurde gespeichert, bevor Analyte zu jedem Auftrag das Gerätemodell aufbewahrte, daher kann sein Ereignis an die Labor-App das Modell nicht nennen. Der Stand des Auftrags auf dieser Seite bleibt erhalten; teilen Sie der Labor-App diesen Stand mit.

admin-analyte-asked-this-machine-for-its-next-results-and = Analyte hat dieses Gerät nach seinen nächsten Ergebnissen gefragt, und das Gerät betrachtet sie möglicherweise als gesendet, bevor ihre Antwort sicher gespeichert war.

admin-test-code-missing = Testcode fehlt

admin-sample-id-missing = Proben-ID fehlt

admin-value-is-not-the-expected-kind = Wert ist nicht von der erwarteten Art

admin-text-with-formatting-codes = Text mit Formatierungscodes

admin-result-classified-more-than-one-way = Ergebnis mehrfach eingestuft

admin-status-only-update = Reine Statusänderung

admin-result-report-incomplete = Ergebnisbericht unvollständig

admin-result-status-not-final = Ergebnisstatus nicht endgültig

admin-processing-the-message-again-with-the-current-settings-produced = Die erneute Verarbeitung der Nachricht mit den aktuellen Einstellungen hat keine Ergebnisse ergeben.

admin-piece-resent-with-other-content = Teil mit anderem Inhalt erneut gesendet

admin-the-machine-sent-the-same-numbered-piece-of-a = Das Gerät hat denselben nummerierten Teil einer Nachricht zweimal mit unterschiedlichem Inhalt gesendet.

admin-piece-arrived-out-of-turn = Teil außer der Reihe eingegangen

admin-a-piece-of-a-message-arrived-when-no-transfer = Ein Teil einer Nachricht ist eingegangen, als keine Übertragung vom Gerät offen war.

admin-message-not-expected = Nachricht nicht erwartet

admin-a-message-arrived-that-analyte-did-not-expect-or = Eine Nachricht ist eingegangen, die Analyte nicht erwartet hat, oder während zu viele Nachrichten warteten.

admin-processing-interrupted = Verarbeitung unterbrochen

admin-processing-of-this-message-stopped-before-it-was-confirmed = Die Verarbeitung dieser Nachricht wurde beendet, bevor sie als abgeschlossen bestätigt war.

admin-returned-work-differs-from-the-order-sent = Zurückgemeldete Arbeit weicht vom gesendeten Auftrag ab

admin-the-work-the-machine-returned-differs-from-the-order = Die vom Gerät zurückgemeldete Arbeit weicht von dem Auftrag ab, den Analyte gesendet hat.

admin-results-held-back-for-review = Ergebnisse zur Prüfung zurückgehalten

admin-results-conflict-with-earlier-ones = Ergebnisse widersprechen früheren

admin-message-too-large = Nachricht zu groß

admin-the-message-was-larger-than-the-size-limit-for = Die Nachricht war größer als die Größengrenze für dieses Gerät.

admin-the-connection-was-stopped-while-the-message-was-arriving = Die Verbindung wurde beendet, während die Nachricht ankam; der angekommene Teil wird aufbewahrt.

admin-message-not-finished-in-time = Nachricht nicht rechtzeitig abgeschlossen

admin-the-machine-did-not-finish-sending-the-message-in = Das Gerät hat das Senden der Nachricht nicht rechtzeitig abgeschlossen; der angekommene Teil wird aufbewahrt.

admin-message-arrived-while-busy = Nachricht kam während hoher Auslastung an

admin-check-characters-do-not-match = Prüfzeichen stimmen nicht überein

admin-the-check-characters-at-the-end-of-the-message = Die Prüfzeichen am Ende der Nachricht stimmen nicht mit ihrem Inhalt überein.

admin-transfer-cut-off-or-refused = Übertragung abgebrochen oder abgelehnt

admin-the-transfer-from-the-machine-did-not-finish-or = Die Übertragung vom Gerät wurde nicht abgeschlossen oder wurde abgelehnt; was angekommen ist, wird aufbewahrt.

admin-edited-result-kept-for-review = Bearbeitetes Ergebnis zur Prüfung aufbewahrt

admin-transfer-cut-off = Übertragung abgebrochen

admin-the-machine-stopped-sending-before-the-message-was-complete = Das Gerät hat das Senden beendet, bevor die Nachricht vollständig war; die angekommenen Teile werden aufbewahrt.

admin-transfer-cut-off-by-a-stop = Übertragung durch einen Stopp abgebrochen

admin-analyte-stopped-while-the-machine-was-sending-the-pieces = Analyte wurde beendet, während das Gerät sendete; die bereits bestätigten Teile werden aufbewahrt.

admin-lab-app-already-has-different-content = Labor-App hat bereits einen anderen Inhalt

admin-the-lab-app-already-holds-different-content-under-this = Die Labor-App hat unter der ID dieser Übermittlung bereits einen anderen Inhalt.

admin-machine-s-cancel-answers-disagree = Stornierungsantworten des Geräts widersprechen sich

admin-order-event-not-sent = Auftragsereignis nicht gesendet

admin-result-kept-for-review = Ergebnis zur Prüfung aufbewahrt

admin-already-received = bereits empfangen

admin-delivery-stored-for-another-contract = Übermittlung für einen anderen Vertrag gespeichert

admin-unexpected-answer-from-the-lab-app = Unerwartete Antwort der Labor-App

admin-this-pc-s-time-zone = Zeitzone dieses PCs ({ $name })

admin-page-loaded = Seite geladen { $zone }

admin-unix-time-not-a-valid-date = Unix-Zeit { $seconds } (kein gültiges Datum)

admin-this-pc-s-time-zone-2 = Zeitzone dieses PCs

admin-less-than-a-minute-ago = vor weniger als einer Minute

admin-unknown-clock-moved-back = unbekannt (Uhr wurde zurückgestellt)

admin-off-for-other-users-because-the-analyte-users-group = Für andere Benutzer ausgeschaltet, weil die Gruppe der Analyte-Benutzer fehlt; daher können sich nur Administratoren mit Windows anmelden

admin-off-because = Ausgeschaltet, weil { $why }

admin-on = Eingeschaltet

admin-cannot-send-to-lab-app-its-api-key-or = Senden an Labor-App { $destination } nicht möglich: Ihr API-Schlüssel oder Signaturschlüssel fehlt. Geben Sie sie auf der Seite „Labor-Apps“ (oder in der Umgebung des Dienstes) ein und starten Sie dann neu. { admin-kept-and-never-deleted }

admin-cannot-send-to-lab-app-it-is-not-set = Senden an Labor-App { $destination } nicht möglich: Sie ist nicht mehr eingerichtet. { $from }: { $items } { admin-kept-item-agreement } aufbewahrt und nie gelöscht. Fügen Sie Labor-App { $destination } auf der Seite „Labor-Apps“ hinzu und starten Sie dann neu, um sie zu senden.

admin-wait-for-lab-app-they-are-sent-after-the = { $from }: { $items } warten auf Labor-App { $destination }; sie werden nach dem nächsten Neustart gesendet.{ $held }

admin-count-failures =
    { $count ->
        [one] { $number } Fehler
       *[other] { $number } Fehler
    }

admin-count-warnings =
    { $count ->
        [one] { $number } Warnung
       *[other] { $number } Warnungen
    }

admin-count-tests =
    { $count ->
        [one] { $number } Test
       *[other] { $number } Tests
    }

admin-count-notes =
    { $count ->
        [one] { $number } Notiz
       *[other] { $number } Notizen
    }

admin-count-days =
    { $count ->
        [one] { $number } Tag
       *[other] { $number } Tage
    }

admin-count-bytes =
    { $count ->
        [one] { $number } Byte
       *[other] { $number } Byte
    }

admin-count-items-need =
    { $count ->
        [one] { $number } Eintrag erfordert eine Prüfung
       *[other] { $number } Einträge erfordern eine Prüfung
    }

admin-count-orders =
    { $count ->
        [one] { $number } Auftrag
       *[other] { $number } Aufträge
    }

admin-count-things-need =
    { $count ->
        [one] { $number } Sache erfordert Ihre Aufmerksamkeit
       *[other] { $number } Sachen erfordern Ihre Aufmerksamkeit
    }

admin-count-waiting-results =
    { $count ->
        [one] { $number } wartendes Ergebnis
       *[other] { $number } wartende Ergebnisse
    }

admin-count-results =
    { $count ->
        [one] { $number } Ergebnis
       *[other] { $number } Ergebnisse
    }

admin-count-failed-tries =
    { $count ->
        [one] { $number } fehlgeschlagener Versuch
       *[other] { $number } fehlgeschlagene Versuche
    }

admin-count-links =
    { $count ->
        [one] { $number } Verbindung
       *[other] { $number } Verbindungen
    }

admin-count-machines =
    { $count ->
        [one] { $number } Gerät
       *[other] { $number } Geräte
    }

admin-count-lab-apps =
    { $count ->
        [one] { $number } Labor-App
       *[other] { $number } Labor-Apps
    }

admin-count-results-or-order-messages =
    { $count ->
        [one] { $number } Ergebnis oder Auftragsnachricht
       *[other] { $number } Ergebnisse oder Auftragsnachrichten
    }

admin-the-test-code-in-does-not-split-into-the = Der Testcode in { $at } lässt sich nicht in die Teile zerlegen, die die Definition dieses Geräts nennt.

admin-the-text-in-carries-formatting-codes-analyte-keeps-them = Der Text in { $at } enthält Formatierungscodes; Analyte bewahrt sie wie gesendet auf und wandelt sie nicht in einfachen Text um.

admin-the-text-in-uses-an-escape-code-that-this = Der Text in { $at } verwendet einen Escape-Code, für den die Einstellungen dieses Geräts nicht angeben, wie er zu lesen ist.

admin-this-machine-s-quality-control-rules-give-the-result = Die Qualitätskontrollregeln dieses Geräts stufen das Ergebnis in { $at } mehrfach ein.

admin-the-message-in-only-changes-a-result-s-status = Die Nachricht in { $at } ändert nur den Status eines Ergebnisses, und Analyte kann solche Änderungen noch nicht aufzeichnen.

admin-the-result-report-in-lacks-a-field-or-a = Dem Ergebnisbericht in { $at } fehlt ein Feld oder ein Ergebnisstatus, den ein Bericht haben muss.

admin-the-value-in-is-not-a-number-but-this = Der Wert in { $at } ist keine Zahl, aber die Definition dieses Geräts erwartet für diesen Testcode eine Zahl.

admin-the-value-in-is-not-a-code-but-this = Der Wert in { $at } ist kein Code, aber die Definition dieses Geräts erwartet für diesen Testcode einen Code.

admin-the-value-in-is-neither-a-number-nor-a = Der Wert in { $at } ist weder eine Zahl noch ein Code, aber die Definition dieses Geräts erwartet für diesen Testcode eines von beiden.

admin-the-result-status-in-is-not-final-but-this = Der Ergebnisstatus in { $at } ist nicht endgültig, aber dieses Gerät ist so eingerichtet, dass es nur endgültige Ergebnisse sendet.

admin-the-result-was-recorded-as-but-kept-for-review = Das Ergebnis wurde als { $what } aufgezeichnet, aber zur Prüfung aufbewahrt: Es hat keine Proben-ID oder enthält einen Wert, den Analyte nicht sicher lesen kann.

admin-the-lab-app-sent-version-of-an-order-after = Die Labor-App hat Version { $offered } eines Auftrags gesendet, nachdem Analyte bereits Version { $kept } hatte; Analyte hat Version { $kept } behalten.

admin-the-lab-app-sent-other-content-under-version-of = Die Labor-App hat unter Version { $kept } eines Auftrags, den Analyte bereits hatte, einen anderen Inhalt gesendet; Analyte hat seine Kopie behalten.

admin-not-named-in-its-definition = In seiner Definition nicht genannt

admin-only-when-its-program-entry-has-capture-true = Nur wenn sein Programmeintrag capture = true enthält

admin-awos = awos:{ $order }:{ $item }

admin-notes-after = notes-after:{ $after }

admin-not-recorded-older-data-analyte-does-not-guess-them = Nicht aufgezeichnet (ältere Daten); Analyte errät sie nicht

admin-namespace-not-recorded = Namensraum nicht aufgezeichnet

admin-a-cancel-request-was-pending-then = ; zu diesem Zeitpunkt war eine Stornierungsanfrage offen

admin-kept-never-deleted = Aufbewahrt, nie gelöscht

admin-test-code-parts-do-not-match = Teile des Testcodes passen nicht

admin-value-cannot-be-read = Wert kann nicht gelesen werden

admin-escape-code-not-agreed = Escape-Code nicht vereinbart

admin-processing-mode-unknown = Verarbeitungsmodus unbekannt

admin-no-results-in-the-message = Keine Ergebnisse in der Nachricht

admin-time-cannot-be-read = Zeit kann nicht gelesen werden

admin-incomplete-astm-report = Unvollständiger ASTM-Bericht

admin-the-analyzer-did-not-report-normal-completion = Das Analysegerät hat keinen normalen Abschluss gemeldet.

admin-final-status-cannot-be-sent = Endgültiger Status kann nicht gesendet werden

admin-this-machine-sends-no-run-number-that-stays-the = Dieses Gerät sendet keine gleichbleibende Laufnummer, und sein Patientencode ist keine vertrauenswürdige Proben-ID, daher kann Analyte dieses Ergebnis nicht von anderen unterscheiden.

admin-result-identity-repeated = Ergebnisidentität wiederholt

admin-two-results-in-this-message-would-have-the-same = Zwei Ergebnisse in dieser Nachricht hätten dieselbe Identität, und keine Unter-ID unterscheidet sie.

admin-the-device-time-or-sequence-number-that-this-machine = Das Gerät, die Zeit oder die Sequenznummer, mit der die Einstellungen dieses Geräts Ergebnisse unterscheiden, fehlt.

admin-result-identity-not-trusted = Ergebnisidentität nicht vertrauenswürdig

admin-this-machine-s-sequence-numbers-can-start-over-so = Die Sequenznummern dieses Geräts können neu beginnen, daher unterscheiden sie Ergebnisse ohne vertrauenswürdigen Lauf und vertrauenswürdige Revision nicht.

admin-this-machine-s-messages-carry-no-trusted-result-or = Die Nachrichten dieses Geräts enthalten keine vertrauenswürdige Ergebnis- oder Laufidentität und geben nicht an, ob ein Ergebnis ein Patienten- oder ein Qualitätskontrollergebnis ist.

admin-processing-again-produced-no-results = Erneute Verarbeitung hat keine Ergebnisse ergeben

admin-result-differs-from-an-earlier-one = Ergebnis weicht von einem früheren ab

admin-a-result-came-with-other-content-under-the-same = Ein Ergebnis kam mit anderem Inhalt unter derselben Identität wie ein früheres Ergebnis, und nichts kennzeichnet es als Korrektur.

admin-the-message-does-not-give-this-result-an-identity = Die Nachricht gibt diesem Ergebnis keine Identität, daher kann Analyte nicht erkennen, ob es neu, wiederholt oder eine Korrektur ist.

admin-the-delivery-was-stored-for-a-lab-app-contract = Die Übermittlung wurde für eine Version des Labor-App-Vertrags gespeichert, die diese Version von Analyte nicht senden kann.

admin-the-lab-app-answered-that-it-took-the-delivery = Die Labor-App hat geantwortet, dass sie die Übermittlung angenommen hat, aber ihre Antwort folgt nicht dem Labor-App-Vertrag, daher zählt Analyte sie nicht als übermittelt.

admin-the-lab-app-refused-the-delivery-but-its-answer = Die Labor-App hat die Übermittlung abgelehnt, aber ihre Antwort folgt nicht dem Labor-App-Vertrag, daher ist ihr Grund nicht bekannt.

admin-the-lab-app-gave-an-answer-that-the-lab = Die Labor-App hat eine Antwort gegeben, die der Labor-App-Vertrag für eine Übermittlung nicht festlegt.

admin-older-order-version-refused = Ältere Auftragsversion abgelehnt

admin-changed-order-refused = Geänderter Auftrag abgelehnt

admin-error-awos-item-not-retained = AWOS-Position nicht aufbewahrt

admin-error-session-capacity = Kapazität für Verwaltungssitzungen erreicht; starten Sie neu oder warten Sie, bis Sitzungen ablaufen

admin-error-admin-session-lock-poisoned = Sperre der Verwaltungssitzungen ist beschädigt

admin-error-choose-place-or-release = wählen Sie „setzen“ oder „aufheben“

admin-error-choose-receipt-or-order = wählen Sie Empfang oder Auftrag

admin-error-store-mismatch = die aktuelle Konfiguration verweist auf einen anderen Datenspeicher

admin-error-machine-cursor-overflow = Überlauf des Gerätecursors

admin-error-no-software-check = für dieses Gerät gibt es keine Prüfung der Softwareversion

admin-error-no-software-warning = für dieses Gerät gibt es keine aktive Softwarewarnung

admin-error-parked-target-cursor-overflow = Überlauf des Cursors für zurückgehaltene Ziele

admin-waiting-after-restart = „Wartet“ ist nach einem Neustart normal

admin-count-recorded-messages =
    { $count ->
        [one] { $number } Nachricht
       *[other] { $number } Nachrichten
    }

admin-count-failed-exchanges =
    { $count ->
        [one] { $number } fehlgeschlagener Austausch
       *[other] { $number } fehlgeschlagene Austausche
    }

admin-count-machine-problems =
    { $count ->
        [one] { $number } Problem
       *[other] { $number } Probleme
    }

admin-count-machines-waiting =
    { $count ->
        [one] { $number } wartet auf das Gerät
       *[other] { $number } warten auf das Gerät
    }

admin-count-machines-awaiting-setup =
    { $count ->
        [one] { $number } noch nicht eingerichtet
       *[other] { $number } noch nicht eingerichtet
    }

admin-count-machines-start-on-restart =
    { $count ->
        [one] { $number } beginnt nach einem Neustart
       *[other] { $number } beginnen nach einem Neustart
    }

admin-count-machines-stop-on-restart =
    { $count ->
        [one] { $number } entfernt, endet beim Neustart
       *[other] { $number } entfernt, enden beim Neustart
    }

admin-count-new-results =
    { $count ->
        [one] { $number } neues Ergebnis
       *[other] { $number } neue Ergebnisse
    }

admin-count-corrections =
    { $count ->
        [one] { $number } Korrektur
       *[other] { $number } Korrekturen
    }

admin-count-already-received =
    { $count ->
        [one] { $number } Ergebnis bereits empfangen
       *[other] { $number } Ergebnisse bereits empfangen
    }

admin-count-conflicts =
    { $count ->
        [one] { $number } widersprüchliches Ergebnis zur Prüfung aufbewahrt
       *[other] { $number } widersprüchliche Ergebnisse zur Prüfung aufbewahrt
    }

admin-count-unresolved =
    { $count ->
        [one] { $number } Ergebnis ohne Identität zur Prüfung aufbewahrt
       *[other] { $number } Ergebnisse ohne Identität zur Prüfung aufbewahrt
    }

admin-count-waiting-items =
    { $count ->
        [one] { $number } Eintrag wartet
       *[other] { $number } Einträge warten
    }

admin-kept-item-agreement =
    { $count ->
        [one] wird
       *[other] werden
    }

admin-retained-deliveries =
    { $status ->
        [missing-keys] { admin-cannot-send-to-lab-app-its-api-key-or }
        [not-configured] { admin-cannot-send-to-lab-app-it-is-not-set }
        [delivering] { admin-still-being-delivered-to-lab-app }
       *[waiting] { admin-wait-for-lab-app-they-are-sent-after-the }
    }

admin-message-holds-no-results = Die Nachricht enthält keine Ergebnisse.

admin-identity-missing = Ergebnisidentität fehlt

admin-message-cut-off = Nachricht abgebrochen

admin-report-reload = Laden Sie die Seite neu; wenn das wiederholt passiert, geben Sie die Details unten an den Support weiter.

admin-report-reload-after = { $reason } { admin-report-reload }

admin-duration-seconds = { $number } s

admin-duration-minutes = { $number } min

admin-duration-hours = { $number } h

admin-ago = vor { $duration }

admin-clock-this-year = { $day }. { $month } { $time }

admin-date-line = { $weekday }, { $day }. { $month } · { $time }

admin-feed-day-today = Heute · { $weekday }, { $day }. { $month }

admin-feed-day = { $weekday }, { $day }. { $month }

admin-feed-day-year = { $weekday }, { $day }. { $month } { $year }

admin-support-link = Link: { $link }

admin-item-number = Position { $index }

admin-namespace = Namensraum { $namespace }

admin-execution-nothing = Nichts

admin-support-delivery = Übermittlung { $id }

admin-order-recorded = Aufgezeichnet: { $recorded }

admin-hold-reason = Grund: { $reason }

admin-hold-message = Nachricht { $id }

admin-reason-lead = Grund:{ " " }

admin-raw-receipt-offset = die Rohansicht eines Empfangs akzeptiert keinen Frame-Offset

admin-raw-cursor-outside = der Cursor der Rohansicht liegt außerhalb der aufbewahrten Rohdaten

admin-raw-cursor-overflow = Überlauf des Cursors der Rohansicht

admin-raw-corrupt = die gespeicherten Rohdaten sind beschädigt

admin-raw-continued = { $what }, Fortsetzung

admin-removed-machine-name = Gerät { $machines }

admin-removed-machine-names = Geräte { $machines }

admin-recorded-as-new = neu

admin-recorded-as-correction = Korrektur

admin-lab-app-answer-unreadable = Die Antwort der Labor-App konnte nicht gelesen werden

admin-page-could-not-be-shown = Diese Seite konnte nicht angezeigt werden

admin-activity-from = { $rest } – von { $machine }

admin-activity-a-machine = Ein Gerät

admin-warning = Warnung

admin-read-results = Ausgewertet · { $results }

admin-senders-from = Von { $machines }

admin-software-version-not-checked = Softwareversion nicht anhand eines Handbuchs geprüft

admin-error-alert-snapshot-too-large = die Momentaufnahme der aktiven Warnungen überschreitet 4096 Zeilen

admin-error-alert-retry-endpoint-missing = der Endpunkt für erneute Warnungsversuche ist nicht konfiguriert

admin-error-receipt-machine-missing = das Gerät des Empfangs fehlt in der aktuellen Konfiguration

admin-error-receipt-page-lost-cursor = die Empfangsseite hat ihren Cursor verloren

admin-error-unknown-receipt = der Empfang existiert nicht

admin-alert-unknown-number-waits = eine unbekannte Anzahl von Ergebnissen oder Auftragsnachrichten wartet

admin-alert-count-waits =
    { $count ->
        [one] { $number } Ergebnis oder Auftragsnachricht wartet
       *[other] { $number } Ergebnisse oder Auftragsnachrichten warten
    }

admin-alert-cannot-deliver-not-configured = Übermittlung nicht möglich: Kein Gerät und keine Labor-App verwendet das Ziel { $destination }; { $waiting }. Fügen Sie Labor-App { $destination } auf der Seite „Labor-Apps“ hinzu und starten Sie neu, oder stellen Sie ihre Einstellungen wieder her; die Ergebnisse werden aufbewahrt und gesendet, sobald sie konfiguriert ist.

admin-alert-cannot-deliver-keys-missing = Übermittlung nicht möglich: Der API-Schlüssel oder Signaturschlüssel für das Labor-App-Ziel { $destination } fehlt; { $waiting }, angehalten. Geben Sie den API-Schlüssel und den Signaturschlüssel für Labor-App { $destination } auf der Seite „Labor-Apps“ oder in der Umgebung des Dienstes ein und starten Sie dann neu; die Ergebnisse werden aufbewahrt und gesendet, sobald die Schlüssel gesetzt sind.
