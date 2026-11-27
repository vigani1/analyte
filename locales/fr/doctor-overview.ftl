### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossaire pour la relecture
### sample = échantillon ; sample ID = ID d’échantillon ; result = résultat ; test = analyse
### rerun = réanalyse ; resend = renvoi ; reprocess = retraitement
### held for review = retenu pour vérification ; Needs review = À vérifier
### machine = automate ; analyzer = analyseur ; lab app = application de laboratoire
### delivery = transmission ; order = demande ; record only = enregistrement seul

doctor-overview-free-disk-space = Espace disque libre

doctor-overview-always-kept-free = Toujours laissé libre

doctor-overview-analyte-stops-receiving-below-this = En dessous, Analyte cesse de recevoir

doctor-overview-disk-status = État du disque

doctor-overview-active-alerts = Alertes actives

doctor-overview-alert-relays = Relais d’alerte

doctor-overview-relay-status-for-support = État des relais pour l’assistance

doctor-overview-main-center = main--center

doctor-overview-let-s-connect-your-first-machine = Connectons votre premier automate.

doctor-overview-analyte-reads-results-from-your-lab-s-analyzers-keeps = Analyte lit les résultats des analyseurs de votre laboratoire, en garde une copie et les envoie à votre application de laboratoire. Les demandes suivent le chemin inverse.

doctor-overview-set-up-your-first-machine = Configurer votre premier automate

doctor-overview-the-engine-is-running = Le moteur fonctionne.

doctor-overview-what-to-do = Que faire : { $error }

doctor-overview-fix-on = { "   " }À corriger sur : { $page } ({ $href })

doctor-overview-command = { "   " }Commande : { $command }

doctor-overview-analyte-cannot-say-how-things-are-right-now = Analyte ne peut pas indiquer l’état actuel

doctor-overview-problem = PROBLEM

doctor-overview-attention = ATTENTION

doctor-overview-add-an-admin-table-with-a-port-to-the = ajoutez une table [admin] avec un port dans le fichier du programme et redémarrez le moteur (voir docs/operations/operate.md)

doctor-overview-run-this-from-an-administrator-powershell-if-the-key = exécutez ceci depuis un PowerShell administrateur ; si le fichier de clé manque, exécutez analyte admin token --rotate et redémarrez le moteur

doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start = démarrez le moteur (Start-Service Analyte, ou systemctl start analyte sous Linux), attendez quelques secondes et réessayez ; s’il ne démarre pas, exécutez analyte doctor et analyte logs

doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection = trouvez le programme qui occupe le port d’administration (Get-NetTCPConnection -LocalPort <port>) et arrêtez-le, ou redémarrez le moteur avec Restart-Service Analyte si la clé d’administration a été renouvelée depuis son démarrage

doctor-overview-check-that-this-command-and-the-engine-are-the = vérifiez que cette commande et le moteur sont de la même version d’Analyte, puis exécutez analyte logs

doctor-overview-the-engine-could-not-read-its-state = Le moteur n’a pas pu lire son état.

doctor-overview-sending-to-lab-app-is-failing-waiting = L’envoi vers l’application de laboratoire { $destination } échoue : { $waiting } en attente

doctor-overview-sending-to-lab-app-is-paused-waiting = L’envoi vers l’application de laboratoire { $destination } est en pause : { $waiting } en attente

doctor-overview-analyte-stopped-working-with = Analyte a cessé de fonctionner avec { $machine }

doctor-overview-orders-cancelled-by-a-machine = Demandes annulées par un automate : { $cancelled }

doctor-overview-for = { $UNCHECKED-WARNING } pour { $machine }

doctor-overview-disk-space-is-low-free = L’espace disque est faible : { $free } libres

doctor-overview-check-the-software-version-reported-by = Vérifiez la version logicielle indiquée par { $machine }.

doctor-overview-has-gone-quiet = { $machine } est devenu silencieux

doctor-overview-has-sent-nothing-for = { $machine } n’a rien envoyé depuis { $seconds }

doctor-overview-a-result-has-waited-to-be-sent = Un résultat attend d’être envoyé depuis { $seconds }

doctor-overview-waiting = , { $count } en attente

doctor-overview-cannot-send-to-lab-app = Envoi impossible vers l’application de laboratoire { $destination } : { $why }{ $waiting }

doctor-overview-orders-with-unknown-outcome = Demandes à l’issue inconnue : { $count }

doctor-overview-sending = Envoi

doctor-overview-sending-in-line = envoi

doctor-overview-disk = Disque

doctor-overview-analyte-review-list = analyte review list

doctor-overview-analyte-actor-you-deliveries-list-reason-why = analyte --actor <you> deliveries list --reason "<why>"

doctor-overview-analyte-orders-list = analyte orders list

doctor-overview-analyte-orders-cancelled = analyte orders cancelled

doctor-overview-analyte-data-keeping = analyte data-keeping

doctor-overview-analyte-actor-you-orders-resume-after-restore-reason-why = analyte --actor <you> orders resume-after-restore --reason "<why>"

doctor-overview-analyte-activity = analyte activity

doctor-overview-analyte-machines-list = analyte machines list

doctor-overview-analyte-lab-apps-list = analyte lab-apps list

doctor-overview-disk-almost-full-analyte-refuses-new-data-from-machines = Disque presque plein : Analyte refuse les nouvelles données des automates

doctor-overview-analyte-could-not-save-a-machine-software-version-warning = Analyte n’a pas pu enregistrer un avertissement de version logicielle d’automate

doctor-overview-keys-missing = clés manquantes

doctor-overview-not-set-up-any-more = n’est plus configuré

doctor-overview-items-may-need-review = Des éléments sont peut-être à vérifier

doctor-overview-bytes-free = { $free } octets libres

doctor-overview-bytes = { $reserve } octets

doctor-overview-alert = Alerte

doctor-overview-responder = Intervenant

doctor-overview-raised = Déclenchée

doctor-overview-last-value = Dernière valeur

doctor-overview-pick-the-machine-s-model = Choisissez le modèle de l’automate

doctor-overview-choose-how-it-s-connected-and-which-lab-app = Choisissez comment il est connecté et quelle application de laboratoire reçoit ses résultats

doctor-overview-run-a-listen-only-test-analyte-only-reads-and = Lancez un test en écoute seule. Analyte se contente de lire et n’envoie rien à l’automate

doctor-overview-save-then-restart-analyte-to-use-the-new-settings = Enregistrez, puis redémarrez Analyte pour appliquer les nouveaux paramètres

doctor-overview-nothing-needs-you-right-now = Rien ne requiert votre intervention pour l’instant.

doctor-overview-analyte-keeps-everything-it-has-received-while-you-fix = Analyte conserve tout ce qu’il a reçu pendant que vous corrigez ces points.

doctor-overview-legend-line-legend-line-wait = legend__line legend__line--wait

doctor-overview-legend-line-legend-line-cut = legend__line legend__line--cut

doctor-overview-link-open = Liaison ouverte

doctor-overview-waiting-2 = En attente

doctor-overview-broken = Interrompue

doctor-overview-enough-space = Espace suffisant

doctor-overview-almost-full-receiving-refused = Presque plein : réception refusée

doctor-overview-order-list = Liste des demandes · { $name }

doctor-overview-cannot-deliver = Transmission impossible

doctor-overview-sending-paused = Envoi en pause

doctor-overview-not-delivering = Aucune transmission

doctor-overview-up-to-date = À jour

doctor-overview-api-key = Clé API

doctor-overview-signing-key = clé de signature

doctor-overview-its-keys-cannot-be-checked = Ses clés ne peuvent pas être vérifiées.

doctor-overview-out-of-date = Pas à jour

doctor-overview-not-checked-yet = Pas encore vérifié

doctor-overview-items-waiting-to-send = Éléments en attente d’envoi

doctor-overview-results-and-order-messages = résultats et messages de demande

doctor-overview-oldest-waiting = Plus ancien en attente

doctor-overview-the-check-did-not-finish-within-seconds = la vérification ne s’est pas terminée en { $as-secs } secondes

doctor-overview-is-valid = { $config } est valide

doctor-overview-cannot-be-used = { $config } ne peut pas être utilisé : { $error }

doctor-overview-what-to-do-2 = { "      " }Que faire : { $fix }

doctor-overview-doctor-and = Diagnostic : { $failures } et { $warnings }.

doctor-overview-the-store-did-not-open-within-seconds = le stockage ne s’est pas ouvert en { $as-secs } secondes

doctor-overview-running-version-and-answering = version { $version } en cours d’exécution, répond

doctor-overview-running-version-but-this-command-is-version = version { $version } en cours d’exécution, mais cette commande est en version { $CARGO-PKG-VERSION }

doctor-overview-there-is-no-store-at-yet = il n’y a pas encore de stockage à { $file }

doctor-overview-opened-but-did-not-close-cleanly = ouvert, mais pas fermé proprement : { $error }

doctor-overview-cannot-be-opened = { $data-directory } ne peut pas être ouvert : { $error }

doctor-overview-free-is-always-kept-free = { $free } libres ; { $reserve } sont toujours laissés libres

doctor-overview-free-at-or-below-the-kept-free-analyte-refuses = { $free } libres, soit au plus les { $reserve } toujours laissés libres : Analyte refuse les nouvelles données des automates

doctor-overview-the-free-space-could-not-be-read = l’espace libre n’a pas pu être lu : { $error }

doctor-overview-a-usb-serial-number-names-a-network-or-folder = un numéro de série USB ({ $number }) désigne une liaison réseau ou dossier

doctor-overview-cable = Câble de { $what }

doctor-overview-analyzer-settings = Réglages de l’analyseur { $what }

doctor-overview-report-poll-interval = Intervalle d’interrogation des rapports de { $what }

doctor-overview-mode = Mode de { $what }

doctor-overview-every-byte-is-kept-and-recorded-under-captures = { $recording-only } ; chaque octet est conservé et enregistré dans captures/{ $id }

doctor-overview-devices = Appareils de { $what }

doctor-overview-accepts-connections-only-from = accepte les connexions uniquement de { $allowed }

doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s = exécutez analyte machines set { $id } --allowed-peer <adresse IP de l’analyseur>, ou cochez « N’accepter que cet appareil » à l’étape 4 de sa configuration

doctor-overview-the-check-for-folder-failed = la vérification du dossier { $path } a échoué : { $error }

doctor-overview-folder-is-there = le dossier { $path } existe

doctor-overview-is-not-a-folder = { $path } n’est pas un dossier

doctor-overview-folder-cannot-be-read = le dossier { $path } ne peut pas être lu : { $error }

doctor-overview-the-serial-ports-could-not-be-listed = les ports série n’ont pas pu être listés : { $error }

doctor-overview-serial-port-is-present = le port série { $name } est présent

doctor-overview-serial-port-is-not-present = le port série { $name } est absent

doctor-overview-the-adapter-with-serial-number-is-at = l’adaptateur de numéro de série { $number } est sur { $port }

doctor-overview-no-adapter-with-serial-number-is-plugged-in = aucun adaptateur de numéro de série { $number } n’est branché

doctor-overview-adapters-report-serial-number = { $several } adaptateurs indiquent le numéro de série { $number } : { $join }

doctor-overview-nothing-listens-on-although-the-engine-runs = rien n’écoute sur { $address } alors que le moteur fonctionne

doctor-overview-is-free-for-analyte-to-listen-on = { $address } est libre pour qu’Analyte y écoute

doctor-overview-is-held-by-the-running-engine = { $address } est occupé par le moteur en fonctionnement

doctor-overview-another-program-holds = un autre programme occupe { $address }

doctor-overview-is-held-by-the-engine-or-by-another-program = { $address } est occupé, par le moteur ou par un autre programme

doctor-overview-cannot-be-used-on-this-computer = { $address } ne peut pas être utilisé sur cet ordinateur : { $error }

doctor-overview-the-engine-connects-to-analyte-overview-says-if-the = le moteur se connecte à { $address } ; analyte overview indique si la liaison est coupée

doctor-overview-not-tried-the-engine-may-be-connected-to-and = non essayé : le moteur est peut-être connecté à { $address }, et un automate peut n’accepter qu’une connexion à la fois

doctor-overview-accepts-a-connection = { $address } accepte une connexion

doctor-overview-does-not-accept-a-connection = { $address } n’accepte pas de connexion : { $error }

doctor-overview-did-not-answer-within-seconds = { $address } n’a pas répondu en { $as-secs } secondes

doctor-overview-lab-app = Application de laboratoire { $name }

doctor-overview-when-its-order-list-last-came-could-not-be = la date de sa dernière liste de demandes n’a pas pu être lue : { $error }

doctor-overview-credential-is-missing = l’identifiant { $name } manque

doctor-overview-its-keys-could-not-be-read-from-this-account = ses clés n’ont pas pu être lues depuis ce compte : { $error }

doctor-overview-resolves-to-no-address = { $host } ne correspond à aucune adresse

doctor-overview-does-not-resolve = { $host } ne peut pas être résolu : { $error }

doctor-overview-did-not-resolve-within-seconds = { $host } n’a pas été résolu en { $as-secs } secondes

doctor-overview-answers-over-https-with-a-valid-certificate-http = { $url } répond en HTTPS avec un certificat valide (HTTP { $status })

doctor-overview-gives-no-https-answer = { $url } ne donne aucune réponse HTTPS : { $error }

doctor-overview-last-complete-order-list = dernière liste de demandes complète { $when }

doctor-overview-its-order-list-is-not-up-to-date-last = sa liste de demandes n’est pas à jour (dernière liste complète : { $when })

doctor-overview-create-it-with-new-localgroup-name-then-add-people = créez-le avec New-LocalGroup -Name "{ $GROUP }", puis ajoutez des personnes avec Add-LocalGroupMember -Group "{ $GROUP }" -Member <user>

doctor-overview-the-group-exists = le groupe { $GROUP } existe

doctor-overview-the-group-is-missing-so-only-administrators-can-sign = le groupe { $GROUP } manque, donc seuls les administrateurs peuvent se connecter avec Windows

doctor-overview-is-an-account-but-not-a-local-group = { $GROUP } est un compte, mais pas un groupe local

doctor-overview-the-group-could-not-be-looked-up = le groupe { $GROUP } n’a pas pu être recherché : { $error }

doctor-overview-the-rule-is-there = la règle « { $FIREWALL-RULE } » ({ $FIREWALL-RULE-NAME }) existe

doctor-overview-no-rule-lets-analyzers-connect-to = aucune règle « { $FIREWALL-RULE } » ({ $FIREWALL-RULE-NAME }) ne permet aux analyseurs de se connecter à { $join }

doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended = les règles du pare-feu n’ont pas pu être lues (PowerShell s’est terminé avec { $status })

doctor-overview-the-firewall-rules-could-not-be-read = les règles du pare-feu n’ont pas pu être lues : { $error }

doctor-overview-ok = OK

doctor-overview-warn = WARN

doctor-overview-fail = FAIL

doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run = exécutez de nouveau analyte doctor ; si cela reste lent, exécutez analyte logs

doctor-overview-fix-what-it-names-or-choose-the-right-file = corrigez ce qui est indiqué (ou choisissez le bon fichier avec --config), puis exécutez de nouveau analyte doctor ; les autres vérifications exigent un fichier du programme valide

doctor-overview-doctor-everything-checked-is-ok = Diagnostic : tout ce qui a été vérifié est correct.

doctor-overview-store = Stockage

doctor-overview-listed-but-not-set-up-yet-so-it-does = listé mais pas encore configuré, il ne fonctionne donc pas

doctor-overview-choose-its-model-on-the-setup-page-save-then = choisissez son modèle sur la page Configuration, enregistrez, puis redémarrez le moteur

doctor-overview-run-the-analyte-exe-the-service-runs-or-update = exécutez le analyte.exe qu’utilise le service, ou mettez les deux à la même version

doctor-overview-run-analyte-overview-for-where-and-how-to-fix = exécutez analyte overview pour savoir où et comment corriger chaque point

doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run = exécutez de nouveau analyte doctor ; si l’état reste inconnu, exécutez analyte logs

doctor-overview-start-the-engine-once-start-service-analyte-it-creates = démarrez le moteur une fois (Start-Service Analyte) ; il crée le stockage

doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte = exécutez de nouveau analyte doctor ; si cela se répète, exécutez analyte logs

doctor-overview-run-this-from-an-administrator-powershell-if-it-names = exécutez ceci depuis un PowerShell administrateur ; si une liste d’accès est indiquée, voir docs/operations/install.md

doctor-overview-check-the-machine-s-settings = vérifiez les réglages de l’automate

doctor-overview-machine-software = Logiciel de l’automate

doctor-overview-check-the-store-and-run-analyte-doctor-again = vérifiez le stockage et exécutez de nouveau analyte doctor

doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing = libérez de l’espace disque sur ce lecteur ; Analyte ne supprime rien pour faire de la place

doctor-overview-check-the-data-directory-s-drive-in-file-explorer = vérifiez le lecteur du dossier de données dans l’Explorateur de fichiers

doctor-overview-choose-the-machine-s-address-or-folder-on-the = choisissez l’adresse ou le dossier de l’automate sur la page Configuration

doctor-overview-accepts-connections-from-any-device-on-the-network-which = accepte les connexions de tout appareil du réseau, qui pourrait envoyer des résultats en se faisant passer pour cet automate

doctor-overview-choose-the-machine-s-export-folder-on-the-setup = choisissez le dossier d’export de l’automate sur la page Configuration

doctor-overview-create-the-folder-or-share-it-again-or-choose = créez le dossier ou partagez-le de nouveau, ou choisissez-en un autre sur la page Configuration

doctor-overview-check-the-usb-serial-adapter-s-driver-in-device = vérifiez le pilote de l’adaptateur série USB dans le Gestionnaire de périphériques

doctor-overview-run-analyte-doctor-again = exécutez de nouveau analyte doctor

doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the = branchez l’adaptateur série USB, ou choisissez le bon port sur la page Configuration

doctor-overview-unplug-the-adapter-that-does-not-belong-to-this = débranchez l’adaptateur qui n’appartient pas à cet automate

doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte = exécutez analyte overview : le service de l’automate dans Analyte s’est peut-être arrêté ; redémarrez le moteur avec Restart-Service Analyte

doctor-overview-stop-the-other-program-or-choose-another-port-on = arrêtez l’autre programme, ou choisissez un autre port sur la page Configuration et sur l’automate

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell = activez la page d’administration pour que doctor puisse le déterminer, ou vérifiez avec Get-NetTCPConnection -LocalPort <port>

doctor-overview-choose-an-address-this-computer-has-on-the-setup = choisissez, sur la page Configuration, une adresse que possède cet ordinateur

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2 = activez la page d’administration pour que doctor puisse déterminer si le moteur fonctionne

doctor-overview-check-that-the-machine-is-on-its-network-settings = vérifiez que l’automate est allumé, ses réglages réseau et le câble

doctor-overview-the-running-engine-has-its-keys = le moteur en fonctionnement dispose de ses clés

doctor-overview-its-api-key-and-signing-key-are-there = sa clé API et sa clé de signature sont présentes

doctor-overview-its-keys-were-not-checked = ses clés n’ont pas été vérifiées

doctor-overview-no-order-list-is-kept-for-it = aucune liste de demandes n’est conservée pour elle

doctor-overview-restart-the-engine = redémarrez le moteur

doctor-overview-start-the-engine-and-run-analyte-doctor-again-the = démarrez le moteur et exécutez de nouveau analyte doctor ; le moteur vérifie lui-même les clés

doctor-overview-its-address-has-no-host-name = son adresse n’a pas de nom d’hôte

doctor-overview-fix-its-address-on-the-lab-apps-page = corrigez son adresse sur la page Applications de laboratoire

doctor-overview-check-the-lab-app-s-address-on-the-lab = vérifiez l’adresse de l’application de laboratoire sur la page Applications de laboratoire et le DNS de cet ordinateur

doctor-overview-check-this-computer-s-dns-and-network = vérifiez le DNS et le réseau de cet ordinateur

doctor-overview-check-the-address-the-lab-app-s-server-and = vérifiez l’adresse, le serveur de l’application de laboratoire et son certificat ; un certificat auquel cet ordinateur ne fait pas confiance doit être corrigé sur le serveur

doctor-overview-check-that-the-engine-runs-and-the-lab-app = vérifiez que le moteur fonctionne et que l’application de laboratoire répond ; en attendant, les automates qui sollicitent des demandes ne reçoivent aucune réponse

doctor-overview-computername-is-not-set-so-the-group-cannot-be = COMPUTERNAME n’est pas défini, le groupe ne peut donc pas être recherché

doctor-overview-run-doctor-from-a-normal-powershell-window = exécutez doctor depuis une fenêtre PowerShell normale

doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to = exécutez de nouveau le programme d’installation, ou autorisez le TCP entrant vers ces ports depuis le réseau du laboratoire dans le Pare-feu Windows Defender

doctor-overview-check-the-rule-in-windows-defender-firewall = vérifiez la règle dans le Pare-feu Windows Defender

doctor-overview-reading-the-firewall-rules-took-too-long = la lecture des règles du pare-feu a pris trop de temps

doctor-overview-with-an-unknown-outcome-check-the-machine = { $uncertain } à l’issue inconnue (vérifiez l’automate)

doctor-overview-cancelled-by-a-machine = { $cancelled } annulée(s) par un automate

doctor-overview-analyte-could-not-read-its-store = Analyte n’a pas pu lire son stockage.

doctor-overview-analyte-could-not-read-whether-order-sending-is-paused = Analyte n’a pas pu lire si l’envoi des demandes est en pause après une restauration.

doctor-overview-analyte-could-not-read-the-free-disk-space = Analyte n’a pas pu lire l’espace disque libre.

doctor-overview-analyte-could-not-read-how-many-items-need-review = Analyte n’a pas pu lire combien d’éléments sont à vérifier.

doctor-overview-analyte-could-not-read-when-each-lab-app-last = Analyte n’a pas pu lire quand chaque application de laboratoire a accepté sa dernière transmission.

doctor-overview-analyte-could-not-read-which-lab-apps-keep-failing = Analyte n’a pas pu lire quelles applications de laboratoire continuent de refuser les transmissions.

doctor-overview-analyte-could-not-read-the-active-alerts = Analyte n’a pas pu lire les alertes actives.

doctor-overview-analyte-could-not-read-the-machine-orders = Analyte n’a pas pu lire les demandes aux automates.

doctor-overview-analyte-could-not-read-when-each-machine-last-sent = Analyte n’a pas pu lire quand chaque automate a envoyé des données pour la dernière fois.

doctor-overview-analyte-could-not-read-what-waits-for-each-lab = Analyte n’a pas pu lire ce qui attend pour chaque application de laboratoire.

doctor-overview-analyte-could-not-read-what-waits-for-lab-apps = Analyte n’a pas pu lire ce qui attend pour les applications de laboratoire qu’aucun automate configuré n’utilise.

doctor-overview-analyte-could-not-read-the-alert-relays = Analyte n’a pas pu lire les relais d’alerte.

doctor-overview-analyte-read-everything-but-could-not-write-the-page = Analyte a tout lu, mais n’a pas pu écrire la page.

doctor-overview-no-orders-with-an-unknown-outcome = Aucune demande à l’issue inconnue

doctor-overview-see-machine-orders = Voir les demandes aux automates

doctor-overview-no-orders-cancelled-by-a-machine = Aucune demande annulée par un automate

doctor-overview-review-cancelled-orders = Vérifier les demandes annulées

doctor-overview-a-result-has-waited-too-long-to-be-sent = Un résultat attend depuis trop longtemps d’être envoyé

doctor-overview-machine-has-gone-quiet = Un automate est devenu silencieux

doctor-overview-machine-software-version-changed = La version logicielle de l’automate a changé

doctor-overview-disk-space-is-low = L’espace disque est faible

doctor-overview-order-list-is-out-of-date = La liste des demandes n’est pas à jour

doctor-overview-orders-with-unknown-outcome-2 = Demandes à l’issue inconnue

doctor-overview-items-need-review = Des éléments sont à vérifier

doctor-overview-results-cannot-be-delivered = Les résultats ne peuvent pas être transmis

doctor-overview-error-admin-off = les pages d’administration sont désactivées dans les paramètres (pas de table [admin]), le moteur en fonctionnement ne peut donc pas être interrogé

doctor-overview-error-not-running = rien ne répond à l’adresse d’administration { $value } : le moteur ne fonctionne pas

doctor-overview-error-unreachable = l’adresse d’administration { $address } est injoignable : { $source }

doctor-overview-error-timed-out = le moteur n’a pas répondu en { $as-secs } secondes

doctor-overview-error-key = la clé d’administration ne peut pas être lue avec cet utilisateur ({ $value }) ; exécutez la commande en tant qu’administrateur

doctor-overview-error-key-thread = la lecture de la clé d’administration s’est arrêtée : { $value }

doctor-overview-error-client = la requête n’a pas pu être préparée : { $value }

doctor-overview-error-unproven = autre chose répond à l’adresse d’administration : il n’a pas pu prouver qu’il est ce moteur Analyte, la clé d’administration n’a donc pas été envoyée

doctor-overview-error-silent = le moteur n’a pas répondu : { $value }

doctor-overview-error-status = le moteur a répondu HTTP { $value }

doctor-overview-error-too-large = la réponse du moteur était trop volumineuse

doctor-overview-error-unreadable = la réponse du moteur n’a pas pu être lue : { $value }

doctor-overview-error-request = la requête n’a pas pu être écrite : { $value }

doctor-overview-error-not-sent = le moteur est injoignable : { $value }

doctor-overview-error-timed-out-2 = le moteur n’a pas répondu à la modification en { $as-secs } secondes

doctor-overview-signal-path-section = 02 · Chemin du signal

doctor-overview-signal-path-flow = — automates → Analyte → applications de laboratoire

doctor-overview-disk-alerts-section = 04 · Disque et alertes

doctor-overview-lab-app-key-advice = saisissez ses clés sur la page Applications de laboratoire, puis redémarrez le moteur

doctor-overview-orders-section = 03 · Demandes
