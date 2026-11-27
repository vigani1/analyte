### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossaire pour la relecture
### sample = échantillon ; sample ID = ID d’échantillon ; result = résultat ; test = analyse
### rerun = réanalyse ; resend = renvoi ; reprocess = retraitement
### held for review = retenu pour vérification ; Needs review = À vérifier
### machine = automate ; analyzer = analyseur ; lab app = application de laboratoire
### delivery = transmission ; order = demande ; record only = enregistrement seul

cli-lang-help = Langue de l’interface (une étiquette BCP 47). Par défaut : LC_ALL, LANG ou l’anglais

cli-interpretation-recorded-decisions-inspect-parked-items-for-failures = Interprétation { $version } enregistrée ; décisions : { $outcomes }. Consultez les éléments mis de côté pour voir les échecs.

cli-using = Utilisation de { $path } ({ $describe }).

cli-scm-dispatch-was-not-selected = La distribution SCM n’a pas été sélectionnée ({ $shutdown-timeout-secs })

cli-resolution = { $id } { $evidence } resolution={ $as-deref }

cli-item-has-no-eligible-measurement-comparison = L’élément { $id } n’a aucune comparaison de mesures possible.

cli-continue-with-after = Continuez avec --after { $id }

cli-not-set-up-choose-its-definition-on-the-setup = { $id } n’est pas configuré : choisissez sa définition sur la page Configuration

cli-restored-order-sending-paused-pending-reconciliation-previous-store = Restauré ; l’envoi des demandes est en pause en attendant le rapprochement. Stockage précédent : { $kept }

cli-actor-is-required-for-recovery-commands = --actor est obligatoire pour les commandes de récupération

cli-dispatched-in-main = distribué dans main

cli-reprocess-must-use-the-recovery-entry-point = le retraitement doit passer par le point d’entrée de récupération

cli-resolve-must-use-the-recovery-entry-point = la résolution doit passer par le point d’entrée de récupération

cli-no-machines-add-one-on-the-setup-page = aucun automate : ajoutez-en un sur la page Configuration

cli-delivery-queued-with-its-original-id-and-bytes = Transmission mise en file d’attente avec son ID et ses octets d’origine.

cli-backup-verified = Sauvegarde vérifiée.

cli-no-log-file-at = Aucun fichier journal à { $path }.

cli-the-windows-service-writes-it-while-it-runs-on = Le service Windows l’écrit pendant son fonctionnement. Sous Linux, le service écrit dans le journal du système :

cli-more-analyte-activity-before = Suite : analyte activity --before { $next }

cli-raised-for = { $alert } ({ $signal }) · déclenchée { $raised } · pour { $responder }

cli-fix-on = { "  " }Corriger sur : { $page }

cli-command = { "  " }Commande : { $command }

cli-oldest-message-kept = Message conservé le plus ancien : { $oldest }

cli-more-analyte-data-keeping-after = Suite : analyte data-keeping --after { $next }

cli-sample-order = Échantillon { $sample } · demande { $order }

cli-more-analyte-orders-cancelled-after = Suite : analyte orders cancelled --after { $next }

cli-no-earlier-activity-this-is-past-the-oldest-line = Aucune activité antérieure : vous êtes au-delà de la plus ancienne ligne conservée par Analyte.

cli-no-activity-yet = Aucune activité pour l’instant.

cli-no-active-alerts = Aucune alerte active.

cli-not-recorded = non enregistré

cli-how-long-messages-are-kept = Durée de conservation des messages

cli-no-machine-is-set-up = { "  Aucun automate n’est configuré." }

cli-not-run-yet = { "  Pas encore exécutée" }

cli-no-holds-messages-are-deleted-only-as-kept-above = { "  Aucune protection : les messages sont supprimés uniquement selon la durée de conservation ci-dessus." }

cli-no-orders-cancelled-by-a-machine = Aucune demande annulée par un automate.

cli-settings-checked-for-this-version = Paramètres vérifiés pour cette version.

cli-exhausted-alert-events-queued-with-original-identities = Événements d’alerte épuisés remis en file d’attente avec leurs identités d’origine.

cli-retention-hold-updated = Protection contre la purge mise à jour.

cli-no-recordings = { $machine } : aucun enregistrement

cli-bytes-last-changed = { $machine } { $file } { $bytes } octets, dernière modification { $at }{ $else }

cli-copied-to-it-contains-patient-data-de-identify-a = Copié dans { $target }. Il contient des données de patients : anonymisez une copie avant de la partager.

cli-wrote-values-replaced-distinct-the-original-is-unchanged = { $path } écrit : { $replaced } valeurs remplacées ({ $distinct } distinctes). L’original n’est pas modifié.

cli-copy-to = copier dans

cli-the-copy-exceeds-the-recording-file-bound = la copie dépasse la taille maximale d’un fichier d’enregistrement

cli-write-a-reason-it-is-recorded-in-the-history = Indiquez un motif. Il est enregistré dans l’historique.

cli-keep-the-reason-to-256-characters = Limitez le motif à 256 caractères.

cli-order-sending-paused-restored-store-requires-machine-app-reconciliation = ENVOI DES DEMANDES EN PAUSE : le stockage restauré exige un rapprochement entre automate et application.

cli-help-main-cli-config = Fichier du programme pour les commandes d’opérateur. Sans cette option : le fichier indiqué par ANALYTE_CONFIG ; sinon analyte.toml dans le dossier actuel, s’il existe ; sinon le fichier du programme installé (Windows : %ProgramData%\Analyte\config\analyte.toml ; Linux : /etc/analyte/analyte.toml), s’il existe. Un fichier trouvé ainsi est indiqué sur la sortie d’erreur standard

cli-help-main-cli-actor = Identité explicite de l’opérateur local, enregistrée dans le journal d’audit ; elle ne peut pas commencer comme celle d’une connexion aux pages d’administration (`admin-session-`, `windows:`)

cli-help-main-command-retry-alerts = Relance les événements d’alerte épuisés avec leurs identités et octets d’origine

cli-help-main-command-captures = Conversations enregistrées (ADR 0009, ADR 0034) : les lister ou en copier une dans un dossier. Elles contiennent des données de patients ; les deux actions sont auditées

cli-help-main-command-holds = Place ou lève une protection contre la purge sans échéance

cli-help-main-command-admin = La clé de la page d’administration

cli-help-main-command-machines = Liste, ajoute, configure, teste et supprime des automates avec les mêmes contrôles que la page Configuration ; les modifications s’appliquent après un redémarrage

cli-help-main-command-lab-apps = Liste, ajoute, modifie et supprime des applications de laboratoire et enregistre leurs clés avec les mêmes contrôles que la page « Applications de laboratoire » ; les modifications s’appliquent après un redémarrage

cli-help-main-command-orders = Demandes aux automates : lister, annulées par un automate, rapprocher, reprendre après une restauration

cli-help-main-command-check-config = Vérifie un fichier du programme et liste la version des paramètres et le niveau de preuve de chaque automate

cli-help-main-command-run = Exécute le moteur dans cette fenêtre jusqu’à Ctrl+C (le service utilise `service run`)

cli-help-main-command-run-shutdown-timeout-secs = Délai maximal strict du processus après le début de l’arrêt

cli-help-main-command-service = Point d’entrée du service Windows ; démarrez-le avec Start-Service Analyte

cli-help-main-command-status = Lignes techniques d’état pour le support : alertes, files d’attente, dernières données par automate

cli-help-main-command-parked = À vérifier : éléments qu’Analyte a conservés mais n’a pas pu traiter. `review` et `parked` sont la même commande

cli-help-main-command-receipts = Retraite un message stocké avec les paramètres actuels

cli-help-main-command-deliveries = Transmissions aux applications de laboratoire : lister, renvoyer

cli-help-main-command-restore = Restaure le stockage depuis une sauvegarde vérifiée ; l’envoi des demandes reste en pause jusqu’au rapprochement

cli-help-main-command-backup = Écrit une sauvegarde vérifiée du stockage dans un fichier

cli-help-main-command-completions = Affiche un script qui complète les commandes d’analyte dans `shell`. PowerShell : analyte completions powershell | Out-String | Invoke-Expression

cli-help-main-admin-token = Remplace l’identifiant d’administration protégé et l’affiche une seule fois

cli-help-main-parked-compare = Afficher la comparaison protégée des résultats et son instantané de confirmation

cli-help-main-parked-suggestions = Lister les suggestions d’échantillons manquants pour une page d’éléments ouverts

cli-help-main-parked-decide = Confirmer une comparaison de mesures affichée

cli-help-main-parked-list-after = Continuer après le dernier ID d’une page limitée précédente

cli-about = Service fidèle de réception des analyseurs

cli-help-operator-command-resume-after-restore = Lève la pause de restauration une fois les automates et l’application rapprochés

cli-help-operator-command-resume-after-restore-reason = Justification de la décision de l’opérateur

cli-help-operator-command-list = Liste une page limitée d’instantanés du cycle de vie des demandes aux automates

cli-help-operator-command-list-after = Continuer après cet ID local de demande à l’automate

cli-help-operator-command-list-machine = Limiter la page à un seul automate installé

cli-help-operator-command-cancelled = La page « Annulées par l’automate » : éléments de demande qu’un automate a annulés de lui-même, à vérifier avec le laboratoire et l’application de laboratoire. Affiche l’ID d’échantillon de chaque élément tel que l’application de laboratoire l’a fourni, comme la page, jamais les valeurs des résultats ; chaque exécution est enregistrée dans l’audit

cli-help-operator-command-cancelled-after = Continuer après cette position, telle que la page précédente l’a affichée

cli-help-operator-command-cancelled-json = Afficher les éléments en JSON pour les scripts

cli-help-operator-command-reconcile = Résout un résultat incertain de l’automate à l’aide des éléments fournis par l’opérateur

cli-help-operator-command-reconcile-id = ID local de demande à l’automate affiché par orders list

cli-help-operator-command-reconcile-outcome = Issue vérifiée par l’opérateur

cli-help-operator-command-reconcile-reason = Explication obligatoire conservée dans l’audit et dans l’événement de l’application

cli-help-holds-command-receipt = Place ou lève une protection sur une réception

cli-help-holds-command-receipt-id = UUID de la réception sur l’hôte

cli-help-holds-command-receipt-release = Lever une protection existante au lieu d’en placer une

cli-help-holds-command-receipt-reason = Explication obligatoire de l’opérateur

cli-help-holds-command-order = Place ou lève une protection couvrant une demande de l’application et ses réceptions liées

cli-help-holds-command-order-destination = Espace de noms de destination

cli-help-holds-command-order-id = Identité de la demande de l’application

cli-help-captures-command-list = Liste les enregistrements de chaque automate : fichier, taille et dernière modification. Ils contiennent des données de patients

cli-help-captures-command-list-machine = Uniquement les enregistrements de cet automate

cli-help-captures-command-export = Copie un enregistrement dans un dossier, octet par octet, sans rien écraser. Il contient des données de patients : anonymisez une copie avant de la partager

cli-help-captures-command-export-machine = Le nom de l’automate

cli-help-captures-command-export-file = Le nom du fichier d’enregistrement, tel que `captures list` l’affiche

cli-help-captures-command-export-to = Le dossier dans lequel le copier ; il doit exister

cli-help-captures-command-export-reason = Pourquoi il est exporté ; l’audit le conserve

cli-help-captures-command-de-identify = Écrit une copie anonymisée d’un enregistrement dans un dossier : les champs patient sont remplacés par des valeurs synthétiques de même forme et les sommes de contrôle sont recalculées. L’original n’est pas modifié. Vérifiez la copie avant de la partager

cli-help-captures-command-de-identify-to = Le dossier dans lequel écrire la copie ; il doit exister

cli-help-captures-command-de-identify-reason = Pourquoi il est anonymisé ; l’audit le conserve

cli-help-captures-command-de-identify-checksum-case = La casse des lettres de la somme de contrôle utilisée par l’automate (VIDAS) ; nécessaire seulement si l’enregistrement ne permet pas de la voir, et la commande le signale alors

cli-help-command-command-overview = La Vue d’ensemble en termes simples : « Tout va bien », ou chaque point qui demande votre attention avec la page et la commande qui le corrigent. Interroge le moteur en cours d’exécution. Code de sortie : 0 tout va bien, 1 à examiner, 2 un problème bloque les données, 3 inconnu

cli-help-command-command-overview-json = Afficher la réponse en JSON pour les scripts

cli-help-command-command-activity = La page Activité : messages stockés, tentatives de transmission (les échecs consécutifs sur une seule ligne), demandes et modifications, des plus récentes aux plus anciennes. Les lignes indiquent les ID d’échantillon et les nombres, jamais les valeurs des résultats ; chaque exécution est enregistrée dans l’audit

cli-help-command-command-activity-limit = Nombre de lignes à afficher (de 1 à 200)

cli-help-command-command-activity-machine = Uniquement les lignes de cet automate

cli-help-command-command-activity-before = Continuer avant cette position, telle que la page précédente l’a affichée

cli-help-command-command-activity-json = Afficher les lignes en JSON pour les scripts

cli-help-command-command-alerts = Alertes actives, chacune avec son motif et l’endroit où la corriger

cli-help-command-command-data-keeping = La page « Conservation des données » : durée de conservation des messages de chaque automate, dernière purge, message conservé le plus ancien et protections actives

cli-help-command-command-data-keeping-after = Continuer après ce numéro de protection, tel que la page précédente l’a affiché

cli-help-command-command-data-keeping-json = Afficher la page en JSON pour les scripts

cli-help-command-command-doctor = Un diagnostic en lecture seule pour le service informatique : le fichier du programme, le moteur, la connexion de chaque automate, chaque application de laboratoire, l’espace disque et la connexion Windows. Chaque ligne indique OK, WARN ou FAIL et ce qu’il faut faire. Rien n’est modifié. Code de sortie : 0 tout est OK, 1 un avertissement, 2 un échec

cli-help-command-command-doctor-json = Afficher les vérifications en JSON pour les scripts

cli-help-command-command-confirm-software = Confirmer que les paramètres actuels ont été vérifiés pour la version signalée

cli-help-command-command-confirm-software-machine = ID de l’automate figurant sur la page Automates

cli-help-command-command-confirm-software-version = Version exacte du logiciel indiquée dans l’avertissement

cli-help-command-command-logs = Emplacement du journal du moteur et ses dernières lignes. Le journal ne contient aucune donnée de patient : Analyte y écrit des noms, des nombres et des ID, jamais des messages ni des valeurs

cli-help-command-command-logs-tail = Nombre de dernières lignes à afficher (de 1 à 1000)

cli-help-command-alerts-list = Liste toutes les alertes actives

cli-help-command-alerts-list-json = Afficher les alertes en JSON pour les scripts

cli-help-lab-apps-command-list = Liste les applications de laboratoire : adresse, délais, automates qui utilisent chacune et emplacement de chaque clé (jamais sa valeur)

cli-help-lab-apps-command-add = Ajoute une application de laboratoire. Ses deux clés sont ensuite demandées sans être affichées, ou lues sur l’entrée standard, une par ligne : la clé d’API, puis la clé de signature

cli-help-lab-apps-command-add-name = Lettres de A à Z, chiffres, - et _ (64 au maximum) ; ce nom sert aussi à nommer ses clés

cli-help-lab-apps-command-set = Modifie l’adresse ou les délais d’une application de laboratoire ; les valeurs non indiquées restent inchangées

cli-help-lab-apps-command-set-name = Le nom de l’application de laboratoire

cli-help-lab-apps-command-keys = Remplace les clés d’une application de laboratoire, demandées sans être affichées ou lues sur l’entrée standard (la clé d’API, puis la clé de signature, une par ligne). Une réponse vide conserve la clé enregistrée. Les clés ne sont jamais lues depuis les options

cli-help-lab-apps-command-remove = Supprime une application de laboratoire qu’aucun automate n’utilise et que rien n’attend

cli-help-lab-apps-settings-address = Son adresse HTTPS, par exemple `https://lab.example.org`

cli-help-lab-apps-settings-freshness-seconds = Durée pendant laquelle sa liste de demandes reste à jour, en secondes entières

cli-help-lab-apps-settings-request-timeout-seconds = Durée d’attente de sa réponse, en secondes entières

cli-help-lab-apps-settings-review-notices = Indique s’il faut lui envoyer les avis « À vérifier » : `yes` ou `no` (ADR 0038). Une nouvelle application de laboratoire n’en reçoit aucun, sauf si ce réglage vaut yes

cli-help-args-cable-baud = Câble série : le débit en bauds : 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 ou 115200

cli-help-args-cable-data-bits = Câble série : bits de données, de 5 à 8

cli-help-args-cable-parity = Câble série : parité : none, odd ou even

cli-help-args-cable-stop-bits = Câble série : bits d’arrêt, 1 ou 2

cli-help-args-cable-flow-control = Câble série : contrôle de flux : none, hardware (RTS/CTS) ou software (XON/XOFF)

cli-help-args-choice-model = Le modèle d’automate : sa clé telle que `machines models` la liste, ou `installed` pour conserver le modèle actuel

cli-help-args-choice-serial-port = Un port série désigné par son nom, par exemple COM3

cli-help-args-choice-usb-serial = Un adaptateur série USB désigné par son numéro de série, pour le retrouver sur n’importe quelle prise USB

cli-help-args-choice-address = L’adresse IP d’une liaison réseau (celle de ce PC quand l’automate se connecte à Analyte, celle de l’automate quand Analyte se connecte à lui), avec --port

cli-help-args-choice-port = Le numéro de port réseau, avec --address

cli-help-args-choice-folder = Le dossier dans lequel Analyte lit les résultats

cli-help-args-choice-lab-app = L’application de laboratoire à laquelle cet automate envoie ses résultats

cli-help-args-choice-allowed-peer = Pour un automate qui se connecte à Analyte : accepter les connexions uniquement depuis cet appareil, l’adresse IP propre de l’analyseur ; répétez pour en indiquer plusieurs. Tout autre appareil est refusé (ADR 0026)

cli-help-args-choice-any-device = Pour un automate qui se connecte à Analyte : accepter de nouveau les connexions de n’importe quel appareil, en supprimant ses appareils autorisés

cli-help-args-choice-choice = L’un des réglages propres au modèle, tels que l’étape 2 les liste sous Réglages de l’analyseur et que `machines models` les nomme ; répétez pour en indiquer plusieurs. Un réglage non indiqué conserve sa valeur enregistrée, sinon la suggestion du modèle

cli-help-args-choice-record-only = Enregistrement seul (ADR 0034) : maintenir le dialogue avec l’analyseur et enregistrer chaque octet, pour écrire sa définition ; rien n’est envoyé à une application de laboratoire, donc pas de --lab-app. Aussi pour un modèle pas encore prêt. Les enregistrements contiennent des données de patients : exportez-les avec `analyte captures export`

cli-help-args-choice-connect-normally = Quitte l’enregistrement seul : reconnecter normalement l’automate (nécessite un modèle prêt et --lab-app). Les messages enregistrés ne sont jamais transmis

cli-help-args-choice-poll-seconds = Autolyser : secondes entre deux interrogations des rapports, lorsque le modèle ou le profil de capture le demande

cli-help-args-choice-reports-carry-controls = Enregistrement seul, Autolyser : indique si ses rapports contiennent des contrôles : yes ou no

cli-help-args-choice-vidas-acknowledgment = Enregistrement seul, VIDAS : l’accusé de réception du PC VIDAS : literal ou alternate

cli-help-args-choice-analyzer-mode = Enregistrement seul, pour un modèle qui le demande (cobas e 411 : elecsys ou cobas) : le mode indiqué dans les réglages propres de l’analyseur ; `machines models` le liste

cli-help-args-choice-repeated-results = Résultats répétés, pour une connexion normale : `automatic` (recommandé ; un renvoi est conservé une seule fois, une correction devient une nouvelle révision) ou `always_ask` (chaque répétition attend dans À vérifier). Si rien n’est indiqué, le choix enregistré est conservé, sinon automatic (ADR 0035)

cli-help-machines-command-list = Liste les automates des paramètres : modèle, connexion, application de laboratoire, configuré ou non, version des paramètres et, d’après le moteur en cours d’exécution, l’état en direct

cli-help-machines-command-list-json = Affiche du JSON pour les scripts au lieu de lignes

cli-help-machines-command-models = Liste les modèles d’automates du dossier machines : prêts ou non, et comment chacun a été vérifié

cli-help-machines-command-ports = Liste les ports série de ce PC avec leur adaptateur et leur numéro de série USB

cli-help-machines-command-add = Ajoute un automate aux paramètres ; il n’est pas encore configuré. Les réglages du câble série indiqués ici sont conservés pour sa configuration

cli-help-machines-command-add-name = Lettres de A à Z, chiffres, - et _ ; ce nom sert aussi à nommer son fichier de paramètres

cli-help-machines-command-add-same-analyzer = Ce nom a un historique enregistré et il s’agit du même analyseur

cli-help-machines-command-set = Configure un automate : son modèle, sa connexion et son application de laboratoire, vérifiés comme à l’étape Enregistrer. Les choix non indiqués conservent leurs valeurs enregistrées

cli-help-machines-command-set-dry-run = Affiche ce qui serait écrit et n’écrit rien

cli-help-machines-command-test = Exécute le test de connexion en écoute seule de l’étape 3 avec les paramètres enregistrés ou les choix indiqués : rien n’est envoyé et rien n’est stocké

cli-help-machines-command-test-seconds = Durée d’écoute, de 1 à 60 secondes

cli-help-machines-command-test-show-bytes = Affiche aussi les octets reçus (64 Kio au maximum). Ils peuvent contenir des données de patients : ne les enregistrez et ne les partagez jamais

cli-help-machines-command-remove = Supprime un automate des paramètres ; ses résultats stockés et son historique sont conservés

cli-help-machines-command-remove-anyway = Le supprime même si du travail n’est pas terminé

cli-error-reserved = --actor ne peut pas commencer par « { $value } » : Analyte ne l’enregistre que pour les personnes connectées aux pages d’administration

cli-error-text = --actor : { $value }

cli-named-by-config = indiqué par --config

cli-named-by-analyte-config = indiqué par ANALYTE_CONFIG

cli-in-the-current-folder = dans le dossier actuel

cli-the-installed-program-file = le fichier du programme installé

cli-log-path = Journal : { $path }

cli-choice-write-name-value = écrivez NOM=VALEUR, par exemple protocol_mode=alternate

cli-token-missing = le fichier du jeton d’administration est absent ; exécutez analyte admin token --rotate

cli-token-invalid = le fichier du jeton d’administration doit contenir un seul jeton de 256 bits encodé en base64

cli-token-permissions = le fichier du jeton d’administration doit être un fichier ordinaire protégé (mode Unix 0600)

cli-token-access = fichier du jeton d’administration : { $error }

cli-token-random = la source aléatoire du système a échoué pour le jeton d’administration : { $error }

cli-token-io = échec de l’opération « { $operation } » sur le jeton d’administration : { $error }

cli-token-op-inspect = inspecter l’identifiant

cli-token-op-read = lire l’identifiant

cli-token-op-create-staging = créer le fichier temporaire protégé

cli-token-op-write-staging = écrire le fichier temporaire

cli-token-op-flush-staging = vider le tampon du fichier temporaire

cli-token-op-install = installer l’identifiant

cli-token-op-flush-directory = vider le tampon du dossier de l’identifiant

cli-setup-settings-do-not-load = Les paramètres d’Analyte ne se chargent pas : { $error }

cli-setup-store-not-opened = Les données stockées d’Analyte n’ont pas pu être ouvertes : { $error }

cli-setup-ports-failed = la liste des ports série a échoué : { $error }

cli-setup-console-failed = la console n’a pas pu être utilisée : { $error }

cli-setup-no-actor = indiquez qui effectue cette modification avec --actor <name> ; c’est enregistré dans le journal d’audit

cli-setup-not-confirmed = rien n’a été enregistré : la modification n’a pas été confirmée

cli-setup-engine-stopped = les clés ne peuvent être enregistrées que par le moteur Analyte, qui ne fonctionne pas ({ $error })

cli-setup-keys-not-sent = les clés n’ont pas été envoyées : { $error }

cli-setup-engine-no-answer = on ne sait pas si le moteur a enregistré la modification : { $error }

cli-advice-run-as-service-account = Exécutez cette commande avec le compte sous lequel le service Analyte fonctionne, ou définissez les clés sur la page « Applications de laboratoire ».

cli-advice-remove-anyway = Pour le supprimer malgré tout, exécutez de nouveau la commande avec --anyway.

cli-advice-same-analyzer = S’il s’agit du même analyseur, exécutez de nouveau la commande avec --same-analyzer.

cli-advice-check-config = Voyez ce qui ne va pas avec : analyte check-config <file>.

cli-advice-start-engine = Démarrez le moteur avec Start-Service Analyte dans un PowerShell administrateur, puis exécutez de nouveau cette commande.

cli-advice-check-lab-apps-list = Vérifiez avec analyte lab-apps list avant d’exécuter de nouveau la commande.

cli-setup-engine-runs-unknown = On ne sait pas ce que le moteur exécute ({ $why }) ; chaque automate des paramètres est donc considéré comme en cours d’exécution.

cli-setup-ask-actor = Votre nom pour l’enregistrement d’audit

cli-setup-restart-note = { $restart } Sous Windows, exécutez Restart-Service Analyte dans un PowerShell administrateur, ou cliquez sur Redémarrer maintenant sur la page Configuration.

cli-fix-this = À corriger : { $reason }

cli-nothing-was-changed = Rien n’a été modifié.

cli-reason-character-cannot-be-recorded = Le motif contient un caractère qui ne peut pas être enregistré.

cli-captures-patient-data = Ces enregistrements contiennent des données de patients. Gardez-les sur ce PC ; avant d’en partager un, faites-en une copie anonymisée avec captures de-identify et vérifiez-la (docs/develop/adding-a-machine.md, « Capture, de-identify and replay »).

cli-captures-machine-name = un nom d’automate ne contient que des lettres, des chiffres, - et _ : { $name }

cli-captures-file-name = indiquez un enregistrement tel que captures list l’affiche (un nom de fichier se terminant par .rec.jsonl), pas un chemin : { $name }

cli-captures-missing = l’automate { $machine } n’a aucun enregistrement nommé { $file }

cli-captures-no-folder = le dossier { $path } n’existe pas ; créez-le d’abord

cli-captures-exists = { $path } existe déjà ; rien n’a été écrasé

cli-captures-too-many = l’automate { $machine } a plus de 4096 enregistrements ; déplacez d’abord les plus anciens

cli-captures-format = l’enregistrement ne peut pas être lu : { $error }

cli-captures-io = échec de l’opération « { $operation } » sur { $path } : { $error }

cli-captures-op-read = lire

cli-captures-op-open = ouvrir

cli-captures-op-flush = vider le tampon

cli-captures-op-create = créer

cli-captures-op-encode = encoder

cli-captures-op-write = écrire

cli-captures-stopped-early = { " " }(capture arrêtée trop tôt : incomplète)

cli-captures-modified-unknown = inconnue

cli-deidentify-checklist = Vérifiez la copie avant de la partager ; les champs de texte libre ont été remplacés.

cli-deidentify-unsupported = l’anonymisation n’est pas prise en charge pour les enregistrements { $protocol } ; rien n’a été écrit

cli-deidentify-unknown-stack = l’enregistrement indique une pile de protocoles qu’Analyte ne connaît pas ; rien n’a été écrit

cli-deidentify-unparsed = événement { $event } : { $reason } ; Analyte ne peut pas savoir où se trouve chaque champ patient, donc rien n’a été écrit

cli-deidentify-checksum = événement { $event } : { $reason } ; rien n’a été écrit

cli-deidentify-deadline = l’anonymisation a duré plus de { $seconds } secondes ; rien n’a été écrit

cli-deidentify-random = la source aléatoire du système a échoué : { $error }

cli-deidentify-limit = une limite de l’anonymisation a été dépassée ; rien n’a été écrit

cli-deidentify-checksum-not-hex = la somme de contrôle d’origine n’est pas composée de deux chiffres hexadécimaux

cli-deidentify-checksum-case-unknown = la casse des lettres de cette somme de contrôle ne peut pas être déduite de l’enregistrement ; exécutez de nouveau avec --checksum-case upper ou lower, selon les réglages de l’automate

cli-deidentify-checksum-not-sum = la somme de contrôle d’origine n’est pas une somme à trois chiffres modulo 256

cli-deidentify-astm-outside-frames = des octets hors des trames ASTM

cli-deidentify-astm-repeat-differs = une trame ASTM répétée diffère de la trame qu’elle répète

cli-deidentify-astm-header-delimiters = un en-tête ASTM sans quatre délimiteurs distincts

cli-deidentify-astm-record-before-header = un enregistrement ASTM avant son en-tête

cli-deidentify-astm-record-no-delimiter = un type d’enregistrement ASTM sans délimiteur de champ

cli-deidentify-astm-manufacturer-record = un enregistrement fabricant (M), dont seul le fabricant définit les champs

cli-deidentify-astm-unknown-record = un type d’enregistrement ASTM inconnu

cli-deidentify-autolyser-host-outside = des octets de l’hôte hors d’un échange Autolyser

cli-deidentify-autolyser-unasked = des octets de l’automate qu’Analyte n’a pas demandés

cli-deidentify-autolyser-short-record = un enregistrement Autolyser plus court que son code patient

cli-deidentify-field-escape = un champ patient contient une séquence d’échappement qui pourrait former du texte patient

cli-deidentify-hl7-outside-blocks = des octets hors des blocs MLLP

cli-deidentify-hl7-msh-encoding = un MSH sans caractères d’encodage distincts

cli-deidentify-hl7-segment-before-msh = un segment HL7 avant son MSH

cli-deidentify-hl7-segment-no-separator = un nom de segment HL7 sans séparateur de champ

cli-deidentify-hl7-segment-no-rules = un segment HL7 pour lequel Analyte n’a aucune règle de champs patient

cli-deidentify-vidas-foreign-byte = un octet que les paquets VIDAS ne contiennent jamais

cli-deidentify-vidas-outside-packets = des octets hors des paquets VIDAS

cli-program-file-not-found = aucun fichier du programme trouvé ; emplacements recherchés : { $looked }. Indiquez --config <file> ou définissez { $variable }

cli-program-file-unreadable = impossible de vérifier { $path } : { $error }. Le fichier du programme installé n’est lisible que par les administrateurs : exécutez la commande en tant qu’administrateur (un PowerShell élevé sous Windows, sudo sous Linux), ou indiquez --config <file>
