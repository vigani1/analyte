### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossaire pour la relecture
### sample = échantillon ; sample ID = ID d’échantillon ; result = résultat ; test = analyse
### rerun = réanalyse ; resend = renvoi ; reprocess = retraitement
### held for review = retenu pour vérification ; Needs review = À vérifier
### machine = automate ; analyzer = analyseur ; lab app = application de laboratoire
### delivery = transmission ; order = demande ; record only = enregistrement seul

admin-live-at = En direct · { $time }

admin-updated-at = Mis à jour à { $time }

admin-resume-updates = Reprendre les mises à jour

admin-language = Langue

admin-browser-language = Utiliser la langue du navigateur

admin-count-failed = Les résultats encore en attente des automates retirés n’ont pas pu être comptés

admin-activity-lede = Uniquement les ID d’échantillon et les nombres. Les noms des patients et les valeurs des résultats n’apparaissent jamais ici.

admin-orders-lede = Les demandes de l’application de laboratoire qu’Analyte a envoyées à un automate, et ce que l’automate en a fait. Analyte ne renvoie jamais une demande de lui-même.

admin-notes-hidden = Les notes de l’automate sont masquées, car le texte libre peut contenir des données de patients. Chaque fois qu’elles sont affichées, Analyte enregistre d’abord qui les a demandées et pourquoi.

admin-item-lede = Ce que l’automate a accepté et ce qu’il a réellement fait sont affichés séparément. Les heures gardent l’écriture et le fuseau horaire de l’automate ; lorsque l’automate omet une heure, la précédente est conservée. Les messages marqués comme refusés n’ont pas été appliqués.

admin-cancelled-lede = Éléments de demande qu’un automate a annulés de lui-même. Vérifiez chacun avec le laboratoire et prévenez l’application de laboratoire si un échantillon doit être analysé à nouveau.

admin-cancelled-notice = L’automate a annulé ces analyses de lui-même. Cela ne les annule pas dans l’application de laboratoire, n’annule pas les autres analyses de la demande et ne permet pas à Analyte d’envoyer quoi que ce soit d’autre ; l’application de laboratoire n’est pas prévenue automatiquement. Si une demande d’annulation de l’application de laboratoire est en attente, réglez-la avec la réponse de l’application de laboratoire elle-même : Analyte ne compte pas l’annulation de l’automate comme cette réponse.

admin-deliveries-lede = Les résultats et les messages de demande qu’Analyte envoie à une application de laboratoire, et l’état de chacun. Chacun garde le même numéro de transmission à chaque nouvelle tentative, afin que l’application de laboratoire ne l’enregistre jamais deux fois.

admin-keeping-lede = Combien de temps Analyte conserve les messages reçus, et les protections qui conservent des données choisies jusqu’à ce que quelqu’un les lève.

admin-holds-text = Une protection préserve des données de la purge jusqu’à ce que quelqu’un la lève ; elle n’expire jamais. Protéger une demande conserve aussi ses messages. Quand Analyte ne sait pas avec certitude ce qui va ensemble, il conserve davantage.

admin-settings-lede = Les réglages avec lesquels Analyte fonctionne, issus de son fichier du programme et de cet ordinateur. Lecture seule : les automates et les applications de laboratoire se modifient dans Configuration, tout le reste dans le fichier du programme.

admin-last-clean-up = Dernière purge

admin-holds = Protections

admin-hold-not-saved = Protection non enregistrée

admin-add-hold = Ajouter une protection

admin-release = Lever

admin-lede = Messages et résultats qu’Analyte a conservés mais n’a pas pu traiter seul. Les éléments restent ici jusqu’à ce que quelqu’un les résolve ; leur historique est conservé.

admin-sign-in = Se connecter

admin-setup-stopped = La configuration n’a pas pu continuer

admin-feed-undated = Heure non enregistrée

admin-lab-app = Application de laboratoire

admin-nothing-waiting = rien en attente

admin-send-again = Renvoyer

admin-waiting-to-send = En attente d’envoi

admin-recording-only = Enregistrement seul — rien n’est envoyé à une application de laboratoire

admin-open-to-any = Accepte les connexions de n’importe quel appareil

admin-open-to-any-in-line = accepte les connexions de n’importe quel appareil

admin-open-to-any-advice = Tout appareil du réseau du laboratoire qui atteint ce port peut envoyer des résultats en se faisant passer pour cet automate. Modifiez sa configuration et choisissez « N’accepter que cet appareil » après le test de connexion.

admin-refused-advice = Seul l’automate doit se connecter à ce port. S’il s’agit de l’automate lui-même, son adresse a changé : ajoutez la nouvelle adresse dans sa configuration. Sinon, trouvez cet appareil.

admin-any-device = N’importe quel appareil

admin-not-set-up = Pas encore configuré

admin-starts-after-restart = Enregistré : démarre après un redémarrage

admin-stops-at-restart = Retiré : s’arrête au redémarrage

admin-settings-unreadable = Les réglages d’Analyte n’ont pas pu être lus

admin-add-machine = Ajouter un automate

admin-not-chosen = Pas encore choisi

admin-not-running = Pas encore en marche

admin-in-analyte = Dans Analyte

admin-connected-by = Connecté par

admin-accepts-from = Accepte les connexions de

admin-cable-settings = Réglages du câble

admin-analyzer-settings = Réglages de l’analyseur

admin-last-data = Dernières données

admin-sends-results-to = Envoie les résultats à

admin-model = Modèle

admin-connection = Connexion

admin-machine = Automate

admin-open-machine = Ouvrir l’automate

admin-nothing-received-yet = Rien reçu pour l’instant

admin-no-machine-running = Aucun automate en marche

admin-no-machine-set-up = Aucun automate n’est configuré.

admin-poll-title = Demande de résultats non réglée

admin-poll-title-in-line = demande de résultats non réglée

admin-how-read = Comment ce message a été lu

admin-compare-measurements = Comparer les mesures

admin-mark-resolved = Marquer comme résolu

admin-process-again = Retraiter

admin-marked-resolved = Marqué comme résolu

admin-marked-resolved-in-line = marqué comme résolu

admin-processed-again = Retraité

admin-processed-again-in-line = retraité

admin-open-item = Ouvrir l’élément

admin-linked-record = Enregistrement lié

admin-as-arrived = Afficher les fragments tels qu’ils sont arrivés

admin-original-message = Message d’origine

admin-show-original = Afficher le message d’origine

admin-restore-paused = L’envoi des demandes est en pause après une restauration

admin-restore-advice = Demandez au service informatique de rapprocher les demandes avec la ligne de commande ; d’ici là, aucune demande n’est envoyée à un automate.

admin-restart-advice = Le redémarrage coupe toutes les connexions aux automates pendant quelques secondes, et Analyte n’accuse réception de rien tant qu’il est arrêté. Après le redémarrage, reconnectez-vous.

admin-patient-data = Peut contenir des données de patients

admin-patient-data-text = Ne l’incluez pas dans des captures d’écran ni dans des demandes d’assistance. Analyte a enregistré qui a demandé à le voir et pourquoi avant de l’afficher.

admin-support = Détails pour l’assistance

admin-not-recorded = Non enregistré

admin-nothing-yet = Rien pour l’instant

admin-not-applicable = —

admin-slow-read = Analyte n’a pas terminé la lecture en 1,5 seconde.

admin-stopped = Arrêté dans Analyte

admin-stopped-sentence = Analyte a cessé de travailler avec cet automate

admin-removed-advice = Après le prochain redémarrage, Analyte cesse de s’y connecter ; d’ici là, il continue de fonctionner. Ses résultats, ses éléments à vérifier et son historique sont conservés et restent visibles ici.

admin-set-up-advice = configurez-le : choisissez son modèle, son mode de connexion et l’application de laboratoire qui reçoit ses résultats, lancez un test en écoute seule, puis enregistrez et redémarrez.

admin-failing-advice = Vérifiez que l’application de laboratoire fonctionne et que cet ordinateur peut la joindre à son adresse.

admin-keys-apply = Les éléments en attente partent après le redémarrage d’Analyte avec les clés.

admin-keys-advice = ajoutez les clés manquantes sur la page Applications de laboratoire ou dans l’environnement du service. Les éléments en attente partent après le redémarrage d’Analyte avec les clés.

admin-key-needed = Nécessaire avant qu’Analyte puisse envoyer des résultats.

admin-key-unchecked = Le fichier de clés verrouillé n’a pas pu être lu, cette clé ne peut donc pas être vérifiée ; la raison figure dans Détails pour l’assistance.

admin-nothing-changed = Rien n’a été modifié

admin-restart-now = Redémarrer maintenant

admin-first-page = Retour à la première page

admin-line-key = Clé de ligne

admin-add-or-set-one-up-on-the-setup-page = Ajoutez-en un ou configurez-le sur la page Configuration.

admin-analyte-reads-keeps-a-copy-sends = Analyte · lit · garde une copie · envoie

admin-no-lab-app = Aucune application de laboratoire

admin-no-running-machine-sends-to-a-lab-app = Aucun automate en marche n’envoie à une application de laboratoire.

admin-differences-used-for-this-message = Différences utilisées pour ce message

admin-where-this-difference-comes-from = D’où vient cette différence

admin-none-were-used = Aucune n’a été utilisée.

admin-this-message-was-read-by-an-older-version-that = Ce message a été lu par une version plus ancienne qui n’enregistrait pas quelles différences étaient activées ou utilisées ; cela ne veut pas dire qu’aucune ne l’était.

admin-what-to-do = Que faire :

admin-next-page = Page suivante

admin-reason = Raison

admin-cancel = Annuler

admin-reason-optional = Raison (facultative)

admin-recorded-with-the-view = Enregistrée avec la consultation.

admin-paste-it-from-your-lab-app-team-s-message = Collez-la depuis le message de l’équipe de votre application de laboratoire.

admin-replace = Remplacer

admin-back = Retour

admin-next = Suivant

admin-welcome = Bienvenue

admin-analyte = · Analyte

admin-main = Principal

admin-analyte-2 = Analyte

admin-engine-running = Moteur en marche

admin-more = Plus

admin-signed-in-as = Connecté en tant que

admin-signed-in = Connecté

admin-sign-out = Se déconnecter

admin-signed-in-with-the-admin-key = Connecté avec la clé d’administration

admin-newest-at-the-top = Les plus récents en haut

admin-live = En direct

admin-pause-updates = Mettre en pause les mises à jour

admin-every-try-of-one-delivery-newest-first-one-line = Chaque tentative d’une transmission, la plus récente en premier, une ligne chacune

admin-show-all-activity = Afficher toute l’activité

admin-all-machines = Tous les automates

admin-show = Afficher

admin-type = Type

admin-today = Aujourd’hui ·

admin-show-earlier-activity = Afficher l’activité antérieure

admin-read-from-what-analyte-stored-what-is-kept-is = Lu à partir de ce qu’Analyte a stocké ; ce qui est conservé se règle dans

admin-this-reading = Cette lecture

admin-differences-switched-on-for-this-machine = Différences activées pour cet automate

admin-none-were-switched-on = Aucune n’était activée.

admin-switched-on-does-not-mean-used-the-next-card = Activée ne veut pas dire utilisée : la carte suivante indique lesquelles ont été utilisées pour ce message.

admin-their-declarations = Leurs déclarations

admin-not-recorded-by-the-version-that-read-this-message = Non enregistré par la version qui a lu ce message.

admin-result-statuses = Statuts des résultats

admin-no-results = Aucun résultat

admin-this-reading-produced-no-results = Cette lecture n’a produit aucun résultat.

admin-none-sent = Aucun envoyé

admin-the-references-above-point-to-the-manual-sections-and = Les références ci-dessus renvoient aux sections du manuel et aux enregistrements à l’origine de chaque différence. Les octets d’origine restent dans le stockage protégé ; pour les voir, utilisez Afficher le message d’origine sur un élément dans À vérifier.

admin-analyte-is-listening-it-reads-connected-once-the-machine = Analyte est à l’écoute ; l’état passe à Connecté dès que l’automate se connecte. Si cela reste ainsi, vérifiez les réglages d’hôte de l’automate.

admin-check-the-machine-s-software-version = Vérifiez la version logicielle de l’automate

admin-first-seen = Vu pour la première fois

admin-settings-checked-for-this-version = Réglages vérifiés pour cette version

admin-accepted-differences-from-the-standard = Différences acceptées par rapport à la norme

admin-none-its-settings-switch-on-no-difference-from-the = Aucune : ses réglages n’activent aucune différence par rapport à la norme.

admin-this-model-does-a-few-things-its-own-way = Ce modèle fait certaines choses à sa manière. Ses réglages activent ces différences ; la lecture de chaque message indique lesquelles ont été utilisées.

admin-where-these-differences-come-from = D’où viennent ces différences

admin-recent-messages-received = Messages reçus récemment

admin-messages-this-machine-sends-are-listed-here-newest-first = Les messages envoyés par cet automate sont listés ici, les plus récents en premier.

admin-how-it-was-read = Comment il a été lu

admin-no-order-was-cancelled-by-a-machine = Aucune demande n’a été annulée par un automate

admin-order-items-a-machine-cancels-on-its-own-are = Les éléments de demande qu’un automate annule de lui-même sont listés ici.

admin-check-each-order-below-with-the-lab-and-the = Vérifiez chaque demande ci-dessous avec le laboratoire et l’application de laboratoire

admin-sample = Échantillon

admin-order = Demande

admin-details = Détails

admin-send-it-even-if-a-newer-correction-of-this = L’envoyer même s’il existe une correction plus récente de ce résultat

admin-this-item = Cet élément

admin-other-items-of-this-order = Autres éléments de cette demande :

admin-the-lab-app-s-copy-of-this-order = La copie de cette demande dans l’application de laboratoire

admin-messages-from-the-machine-about-this-item = Messages de l’automate concernant cet élément

admin-no-message-about-this-item = Aucun message concernant cet élément

admin-messages-the-machine-sends-about-this-item-are-listed = Les messages que l’automate envoie concernant cet élément sont listés ici, les plus anciens en premier.

admin-notes-the-machine-sent-as-sent = Notes envoyées par l’automate, telles qu’envoyées

admin-to-show-the-original-message = pour afficher le message d’origine.

admin-keeping-times = Durées de conservation

admin-no-clean-up-has-run-yet = Aucune purge n’a encore été effectuée.

admin-no-holds = Aucune protection

admin-nothing-is-held-now-clean-up-follows-the-keeping = Rien n’est protégé actuellement ; la purge suit les durées de conservation.

admin-hold-a = Protéger un élément de type

admin-message = Message

admin-message-number-or-lab-app-order-number = Numéro de message ou numéro de demande de l’application de laboratoire

admin-lab-app-orders-only = Application de laboratoire (demandes uniquement)

admin-recorded-with-the-hold-and-the-time = Enregistrée avec la protection et l’heure.

admin-i-confirm-this-hold = Je confirme cette protection

admin-save-hold = Enregistrer la protection

admin-sign-in-to-add-or-release-a-hold = Connectez-vous pour ajouter ou lever une protection

admin-analyte-records-who-places-or-releases-a-hold-so = Analyte enregistre qui pose ou lève une protection ; il faut donc une session de navigateur connectée.

admin-engine = Moteur

admin-running = En marche

admin-store-limits = Limites du stockage

admin-time-limits = Limites de temps

admin-alerts = Alertes

admin-keys = Clés

admin-where-each-is-found-never-its-value = Où chacune se trouve, jamais sa valeur

admin-arrived = Arrivé

admin-item = Élément

admin-history = Historique

admin-item-2 = · Élément

admin-why-it-s-here = Pourquoi il est ici

admin-everything-linked-to-it = Tout ce qui y est lié

admin-the-original-bytes-are-hidden-because-they-may-contain = Les octets d’origine sont masqués, car ils peuvent contenir des données de patients. Chaque fois qu’ils sont affichés, Analyte enregistre d’abord qui les a demandés et pourquoi.

admin-tube-id = ID de tube

admin-tube-id-missing = ID de tube manquant

admin-analyzer-status = Statut de l’analyseur

admin-source-fields-and-receipt-id = Champs source et ID de réception

admin-this-looks-like-the-same-result-now-with-tube = Il semble s’agir du même résultat, désormais avec l’ID de tube

admin-resolve-the-held-item = . Résoudre l’élément retenu ?

admin-choose-resend-to-record-a-duplicate-without-sending-another = Choisissez renvoi pour enregistrer un doublon sans envoyer d’autre résultat. Choisissez réanalyse pour envoyer une nouvelle mesure à l’application de laboratoire.

admin-choose-correction-to-replace = Choisissez correction quand la valeur retenue remplace la précédente : l’application de laboratoire la reçoit comme révision suivante du même résultat.

admin-held-measurement = Mesure retenue

admin-later-measurement = Mesure ultérieure

admin-earlier-measurement = Mesure antérieure

admin-resolve-held-item = Résoudre l’élément retenu

admin-this-is-a-resend = Il s’agit d’un renvoi

admin-this-is-a-rerun = Il s’agit d’une réanalyse

admin-this-is-a-correction = Il s’agit d’une correction

admin-replace-the-earlier-value = Remplacer la valeur précédente dans l’application de laboratoire ?

admin-correction-dialog-text = L’application de laboratoire reçoit le résultat retenu comme révision suivante du résultat précédent et remplace la valeur précédente. Le résultat précédent et les deux messages restent dans Analyte tels que reçus. Votre nom, l’heure et le motif sont enregistrés.

admin-earlier-value = Valeur précédente

admin-new-value = Nouvelle valeur

admin-send-as-correction = Envoyer comme correction

admin-help-what-each-choice-means = Ce que signifie chaque choix

admin-help-three-cases = Un résultat peut revenir pour trois raisons :

admin-help-case-resend = Renvoi

admin-help-case-resend-text = L’automate a envoyé de nouveau le même résultat.

admin-help-case-rerun = Réanalyse

admin-help-case-rerun-text = Le même échantillon et la même analyse ont été mesurés de nouveau.

admin-help-case-correction = Correction

admin-help-case-correction-text = L’automate a modifié un résultat qu’il avait envoyé.

admin-help-correction-note = Une correction que l’automate signale comme telle part d’elle-même vers l’application de laboratoire, comme nouvelle révision du résultat, sauf si l’automate est réglé sur « Toujours me demander ». Si l’automate a modifié un résultat sans le signaler, celui-ci attend ici. Vérifiez sur l’automate quelle valeur est la bonne avant de choisir ; choisissez « Il s’agit d’une correction » seulement si la nouvelle valeur remplace la précédente.

admin-help-buttons = Ce que fait chaque bouton :

admin-help-resend-does = Enregistre le résultat retenu comme le même résultat que le précédent. Rien de nouveau n’est envoyé à l’application de laboratoire.

admin-help-rerun-does = Envoie une fois le résultat retenu à l’application de laboratoire, comme nouveau résultat. Le résultat précédent reste ; l’application de laboratoire décide quelle valeur rendre.

admin-help-correction-does = Envoie une fois le résultat retenu à l’application de laboratoire, comme révision suivante du résultat précédent, afin que l’application de laboratoire remplace la valeur précédente. Analyte vous demande d’abord de confirmer. Les deux messages restent tels que reçus.

admin-help-resolve-held-does = Proposé quand un résultat retenu sans ID d’échantillon correspond à un message ultérieur du même automate qui en a un. Le message ultérieur fait foi, et rien de plus n’est envoyé.

admin-help-process-again-does = Relit le message d’origine avec les réglages actuels et enregistre la lecture comme nouvelle version ; la lecture précédente est conservée. Si le message est maintenant lu correctement, l’élément est réglé et ses résultats partent vers l’application de laboratoire comme d’habitude. Sinon, l’élément reste ouvert.

admin-help-send-again-does = Remet en file d’attente la même transmission pour l’application de laboratoire, inchangée et avec le même ID. L’élément reste ouvert jusqu’à ce que quelqu’un le marque comme résolu.

admin-help-mark-resolved-does = Enregistre que quelqu’un a vérifié l’élément. Rien n’est envoyé à un automate ni à l’application de laboratoire. L’élément passe dans Résolus et reste dans l’historique.

admin-help-audited = Chaque choix demande une raison et enregistre qui l’a fait et quand. Le message d’origine n’est jamais modifié.

admin-help-left-alone = Un élément sur lequel personne ne décide reste ici. Analyte ne l’envoie ni ne le supprime jamais de lui-même.

admin-help-notices = Si les avis À vérifier sont activés pour une application de laboratoire, celle-ci affiche les éléments qui attendent ici et reçoit un avis de clôture dès que chacun est réglé. La décision se prend toujours ici seulement.

admin-left-every-byte-exactly-in-hex-right-the-same = À gauche : chaque octet exactement, en hexadécimal. À droite : les mêmes octets sous forme de texte, les octets qui ne sont pas de simples lettres ou signes étant affichés sous forme de points.

admin-sign-in-to-analyte = Se connecter à Analyte

admin-use-the-admin-key-of-analyte-on-this-pc = Utilisez la clé d’administration d’Analyte sur cet ordinateur.

admin-admin-key = Clé d’administration

admin-sign-in-with-windows = Se connecter avec Windows

admin-the-key-is-in-the-file = La clé se trouve dans le fichier

admin-admin-token = admin-token

admin-in-analyte-s-data-folder-on-this-pc-ask = dans le dossier de données d’Analyte sur cet ordinateur. Demandez-la au service informatique si vous ne l’avez pas.

admin-engine-running-it-keeps-running-when-you-sign-out = Moteur en marche · il continue de fonctionner quand vous vous déconnectez

admin-restart-to-apply = Redémarrer pour appliquer

admin-lab-app-2 = Application de laboratoire { $destination }

admin-removed-machine = Automate retiré { $machines }

admin-removed-machines = Automates retirés { $machines }

admin-of-them-need-someone-to-act-on-the-deliveries = { " " }Pour { $held } d’entre eux, une intervention est nécessaire sur la page Transmissions (en pause, refusés ou en conflit).

admin-kept-and-never-deleted = { $from } : { $items } { admin-kept-item-agreement }.{ $held }

admin-still-being-delivered-to-lab-app = { $from } : { $items } en cours de transmission à l’application de laboratoire { $destination }.{ $held }

admin-and-possibly-more = { " et peut-être d’autres" }

admin-messages-from-machines-deliveries-to-lab-apps-orders-and = Les messages des automates, les transmissions aux applications de laboratoire, les demandes et les modifications sont listés ici dès qu’Analyte les a stockés.

admin-all = Tout

admin-results = Résultats

admin-orders = Demandes

admin-problems = Problèmes

admin-changes = Modifications

admin-no-earlier-activity = Aucune activité antérieure

admin-this-page-is-past-the-oldest-line-analyte-keeps = Cette page va au-delà de la ligne la plus ancienne conservée par Analyte.

admin-back-to-the-newest = Retour aux plus récents

admin-no-tries-kept = Aucune tentative conservée

admin-analyte-keeps-no-try-of-this-delivery = Analyte ne conserve aucune tentative de cette transmission.

admin-nothing-for-this-filter = Rien pour ce filtre

admin-no-stored-activity-matches-this-machine-and-type = Aucune activité stockée ne correspond à cet automate et à ce type.

admin-no-activity-yet = Aucune activité pour l’instant

admin-expected-the-position-of-a-line-as-at-source = position d’une ligne attendue, sous la forme at.source.key

admin-reading = lecture { $version }

admin-message-2 = Message : { $id }

admin-reading-2 = Lecture : { $version }

admin-read-by = Lu par : { $driver }

admin-settings-version = Version des réglages : { $digest }

admin-incomplete-record-reason-as-stored = Enregistrement incomplet, raison telle que stockée : { $reason }

admin-reason-as-stored = Raison telle que stockée : { $reason }

admin-record-field = Enregistrement { $record }, champ { $field }

admin-coordinates-spans = Coordonnées : { $coordinates } ; étendues : { $spans }

admin-marked-final-because-the-manual-says-so-not-because = Marqué comme définitif parce que le manuel l’indique, et non parce que l’automate a envoyé un statut définitif

admin-reading-3 = Lecture

admin-read-by-2 = Lu par

admin-settings-label = Libellé des réglages

admin-settings-version-2 = Version des réglages

admin-not-sent-to-the-lab-app-the-record-of = Non envoyé à l’application de laboratoire : l’enregistrement des différences utilisées est incomplet

admin-this-message-could-not-be-read = Ce message n’a pas pu être lu

admin-earlier-reading = Lecture précédente

admin-next-reading = Lecture suivante

admin-as-received = Tel que reçu

admin-read-as = Lu comme

admin-standard = Norme

admin-where-in-the-message = Où dans le message

admin-it-waits-on-the-needs-review-page-instead-of = Il attend sur la page À vérifier au lieu d’être transmis à l’application de laboratoire.

admin-production-mode-supplied-by-the-machine-definition = Mode production fourni par la définition de l’automate

admin-the-received-astm-header-had-an-empty-processing-id = L’en-tête ASTM reçu avait un ID de traitement (processing ID) vide.

admin-accepted-on-purpose-values-are-kept-exactly-as-received = Accepté volontairement : les valeurs sont conservées exactement telles que reçues

admin-the-newest-messages-are-listed-older-ones-stay-stored = Les { $RECENT } messages les plus récents sont listés ; les plus anciens restent stockés.

admin-machine-2 = Automate : { $id }

admin-message-received = Message { $id } reçu { $received-at }

admin-is-removed-from-analyte-s-settings = { $id } est retiré des réglages d’Analyte

admin-machines-set-up-or-removed-since-analyte-started-may = Les automates configurés ou retirés depuis le démarrage d’Analyte peuvent manquer ici.

admin-no-machine-is-set-up-yet = Aucun automate n’est encore configuré

admin-add-one-then-set-it-up-step-by-step = Ajoutez-en un, puis configurez-le étape par étape : son modèle, sa connexion, un test en écoute seule et l’enregistrement.

admin-no-more-machines = Aucun autre automate

admin-this-page-is-past-the-end-of-the-list = Cette page va au-delà de la fin de la liste.

admin-whether-this-machine-is-still-set-up-is-not = On ne sait pas pour l’instant si cet automate est encore configuré.

admin-removed-from-analyte-s-settings = Retiré des réglages d’Analyte

admin-shown-after-analyte-restarts = Affiché après le redémarrage d’Analyte

admin-set-it-up = Le configurer

admin-repeated-results = Résultats répétés

admin-handled-automatically-a-resend-is-kept-once-a-correction = Traités automatiquement : un renvoi est conservé une seule fois, une correction devient une nouvelle révision

admin-always-ask-me-every-repeat-waits-in-needs-review = Toujours me demander : chaque répétition attend dans À vérifier

admin-recorded = Enregistré

admin-stopped-2 = Arrêté

admin-test-connection = Tester la connexion

admin-edit-setup = Modifier la configuration

admin-no-reading-stored = aucune lecture stockée

admin-in-needs-review = Dans À vérifier

admin-machine-removed = Automate retiré

admin-since = Depuis

admin-sends-to = Envoie à

admin-time = Heure

admin-status-sent-by-the-machine = Statut envoyé par l’automate

admin-shown-as = Affiché comme

admin-this-machine-has-no-software-warning-you-can-confirm = Cet automate n’a aucun avertissement logiciel que vous puissiez confirmer.

admin-the-lab-app-s-copy-says = La copie de l’application de laboratoire indique : { $state }

admin-lab-app-order-version-sent-machine-order-item = Application de laboratoire { $destination } · version de demande { $version } envoyée · demande à l’automate { $id }, élément { $index }

admin-order-item = Demande { $number }, élément { $index }

admin-order-2 = Demande { $number }

admin-message-receipt-reading = Message { $message } · réception { $receipt }, lecture { $version }

admin-receipt-reading = Réception { $receipt }, lecture { $interpretation }

admin-recorded-2 = Enregistré { $shown }{ $else }

admin-the-same-version-that-was-sent = { $version } : la même version que celle envoyée

admin-a-different-version-from-the-one-sent-so-its = { $version } : une version différente de celle envoyée, ses analyses ne sont donc pas celles que l’automate a reçues

admin-the-lab-app-s-copy-cannot-be-read-so = La copie de l’application de laboratoire ne peut pas être lue, sa version n’est donc pas affichée. Détails pour l’assistance : { $error }

admin-from-the-machine-hidden = { $notes } de l’automate, masquées

admin-message-received-2 = Message reçu { $now }

admin-cancelled-by-the-machine-recorded = { "Annulé par l’automate · enregistré " }

admin-the-lab-app-s-copy-cannot-be-read = La copie de l’application de laboratoire ne peut pas être lue.

admin-no-copy-of-this-order-from-the-lab-app = Aucune copie de cette demande provenant de l’application de laboratoire n’est conservée.

admin-not-known = inconnu

admin-this-form-is-not-one-analyte-sent-for-this = Ce formulaire n’est pas l’un de ceux qu’Analyte a envoyés pour cette page. Rouvrez la demande et refaites ce que vous avez fait.

admin-show-the-notes = Afficher les notes

admin-order-status = Statut de la demande

admin-order-version-sent = Version de demande envoyée

admin-tests-sent = Analyses envoyées

admin-last-change = Dernière modification

admin-status = Statut

admin-what-the-machine-reports = Ce que l’automate signale

admin-tests = Analyses

admin-not-recorded-older-data = Non enregistré (données plus anciennes)

admin-order-number-on-the-machine = Numéro de demande sur l’automate

admin-last-note = Dernière note

admin-cancelled-in-the-lab-app = Annulé dans l’application de laboratoire

admin-kept-until = Conservé jusqu’au

admin-its-version = Sa version

admin-recorded-by-analyte = Enregistré par Analyte

admin-outcome = Résultat de l’opération

admin-results-came-with-it = Des résultats l’accompagnaient

admin-not-sent-the-earlier-expected-time-is-kept = Non envoyé ; l’heure prévue précédente est conservée

admin-sent-as-empty-cleared-by-the-machine = Envoyé vide (effacé par l’automate)

admin-not-sent = non envoyé

admin-expected-ready-time-as-sent = Heure prévue de disponibilité, telle qu’envoyée

admin-message-time-and-zone = Heure et fuseau du message

admin-event-time-as-sent = Heure de l’événement, telle qu’envoyée

admin-delivery = Transmission { $id } · { $state }

admin-analyte-sends-it-to-lab-app-again-unchanged-and = Analyte la renvoie à l’application de laboratoire { $destination }, inchangée et avec le même numéro de transmission. Son statut actuel : { $word }. Si l’application de laboratoire l’a déjà, elle traite la répétition comme déjà faite. Analyte vérifie à nouveau qu’elle peut être envoyée lorsque vous confirmez.

admin-it-goes-to-the-lab-app-unchanged-with-the = Elle part vers l’application de laboratoire inchangée, avec le même numéro de transmission, afin que l’application de laboratoire traite une répétition comme déjà faite.

admin-not-delivered = Non transmis

admin-no-more-deliveries = Aucune autre transmission

admin-this-page-is-past-the-last-delivery = Cette page va au-delà de la dernière transmission.

admin-no-deliveries-yet = Aucune transmission pour l’instant

admin-results-and-order-messages-analyte-sends-to-a-lab = Les résultats et les messages de demande qu’Analyte envoie à une application de laboratoire sont listés ici.

admin-nothing-waits-for-a-lab-app = Rien n’attend une application de laboratoire

admin-every-delivery-listed-was-accepted-by-its-lab-app = Chaque transmission listée a été acceptée par son application de laboratoire.

admin-show-all-deliveries = Afficher toutes les transmissions

admin-order-message = Message de demande

admin-needs-review-notice = Avis À vérifier

admin-send-this-delivery-again = Renvoyer cette transmission ?

admin-for-example-the-lab-app-team-asked-for-it = Par exemple : l’équipe de l’application de laboratoire l’a redemandée

admin-required-up-to-256-characters-recorded-with-the-action = Obligatoire, 256 caractères au maximum. Enregistrée avec l’action et l’heure.

admin-queued-to-send-again = Mise en file d’attente pour renvoi

admin-the-delivery-is-queued-to-send-again = La transmission est mise en file d’attente pour être renvoyée

admin-lab-app-order-version-sent-machine-order = Application de laboratoire { $destination } · version de demande { $version } envoyée · demande à l’automate { $id }

admin-unknown = Inconnu

admin-cancelled = Annulé

admin-cancelled-in-line = annulé

admin-no-more-orders = Aucune autre demande

admin-this-page-is-past-the-last-order = Cette page va au-delà de la dernière demande.

admin-no-orders-yet = Aucune demande pour l’instant

admin-orders-analyte-sends-from-a-lab-app-to-a = Les demandes qu’Analyte envoie d’une application de laboratoire à un automate sont listées ici.

admin-no-order-with-an-unknown-outcome = Aucune demande à l’issue inconnue

admin-every-order-sent-to-a-machine-has-a-known = Chaque demande envoyée à un automate a une issue connue.

admin-show-all-orders = Afficher toutes les demandes

admin-no-cancelled-order = Aucune demande annulée

admin-no-order-was-cancelled-by-the-lab-app-and = Aucune demande n’a été annulée par l’application de laboratoire puis confirmée par son automate.

admin-review = Vérifier

admin-the-machine-cancelled-part-of-this-order = L’automate a annulé une partie de cette demande

admin-action = Action

admin-queued = En file d’attente

admin-from = De

admin-to = À

admin-deleted-after-the-lab-app-last-confirmed-it = Supprimé { $days } après la dernière confirmation par l’application de laboratoire

admin-message-from-received = Message de { $machine } · reçu{ " " }

admin-order-from-lab-app = Demande { $order } de l’application de laboratoire { $destination }

admin-set-by = Posée par { $actor }

admin-order-lab-app = Demande { $order } · application de laboratoire { $destination }

admin-clean-up-may-delete-it-once-its-keeping-time = { $plain } : la purge peut le supprimer une fois sa durée de conservation écoulée. L’historique de la protection est conservé.

admin-oldest-message-kept = Message le plus ancien conservé

admin-ran = Effectuée

admin-messages-checked = Messages examinés

admin-messages-deleted = Messages supprimés

admin-orders-deleted = Demandes supprimées

admin-paused = En pause

admin-failed = Échec

admin-purge-failed = Échec de la purge

admin-a-message-no-longer-listed = Un message qui n’est plus listé

admin-until-released = jusqu’à sa levée

admin-who-placed-it-is-not-in-the-history-until = L’auteur n’est pas dans l’historique · jusqu’à sa levée

admin-release-this-hold = Lever cette protection ?

admin-for-example-the-audit-is-finished = Par exemple : l’audit est terminé

admin-required-up-to-256-characters-recorded-with-the-time = Obligatoire, 256 caractères au maximum. Enregistrée avec l’heure.

admin-tick-the-confirmation-box-and-make-sure-you-are = Cochez la case de confirmation et assurez-vous d’être connecté, puis réessayez.

admin-authenticated-session-valid-form-and-confirmation-required = session authentifiée, formulaire valide et confirmation requis

admin-hold-released = Protection levée

admin-hold-saved = Protection enregistrée

admin-the-hold-is-released = La protection est levée

admin-the-hold-is-saved = La protection est enregistrée

admin-your-session-and-reason-are-recorded-with-it = Votre session et votre raison sont enregistrées avec elle.

admin-lab-apps = Applications de laboratoire : { $why }

admin-of = { $open } sur { $machines }

admin-the-lab-apps-settings-could-not-be-read-so = Les réglages des applications de laboratoire n’ont pas pu être lus, leurs clés ne sont donc pas listées

admin-analyte-version = Version d’Analyte

admin-machine-links-open = Liaisons d’automates ouvertes

admin-admin-pages = Pages d’administration

admin-reachable-from-this-pc-only = Accessibles depuis cet ordinateur uniquement

admin-program-file = Fichier du programme

admin-data-folder = Dossier de données

admin-times-on-these-pages = Heures sur ces pages

admin-windows-sign-in = Connexion Windows

admin-disk-space-kept-free = Espace disque maintenu libre

admin-largest-database = Base de données la plus grande

admin-largest-message-or-record = Message ou enregistrement le plus grand

admin-wait-for-a-busy-store = Attente d’un stockage occupé

admin-store-start-limit = Délai de démarrage du stockage

admin-shutdown-limit = Délai d’arrêt

admin-page-reads = Lectures de pages

admin-page-requests = Requêtes de pages

admin-setup-changes = Modifications de configuration

admin-checked-every = Vérifié toutes les

admin-alert-targets = Destinataires des alertes

admin-none-set-up-alerts-show-on-these-pages-only = Aucun configuré ; les alertes s’affichent uniquement sur ces pages

admin-reminder-every = Rappel toutes les

admin-wait-for-an-answer = Attente d’une réponse

admin-tries-per-alert = Tentatives par alerte

admin-in-the-file-admin-token-in-the-data-folder = Dans le fichier admin-token du dossier de données

admin-read-the-reason-above-and-open-the-item-for = Lisez la raison ci-dessus et ouvrez l’élément pour voir ses enregistrements liés. Si la marche à suivre n’est pas claire, donnez le numéro de l’élément à l’assistance.

admin-the-machine-sent-a-result-without-a-test-code = L’automate a envoyé un résultat sans code d’analyse. Vérifiez les réglages de l’automate et l’endroit où la définition de cet automate lit le code, puis utilisez Retraiter.

admin-the-value-does-not-fit-the-kind-this-machine = La valeur ne correspond pas au type que la définition de cet automate attend pour ce code d’analyse. Vérifiez cette attente et les réglages de l’automate, puis utilisez Retraiter.

admin-the-message-has-no-sample-id-check-how-the = Le message n’a pas d’ID d’échantillon. Vérifiez comment l’automate envoie les ID d’échantillon ; une fois ses réglages corrigés, utilisez Retraiter.

admin-check-this-machine-s-definition-and-settings-against-the = Comparez la définition et les réglages de cet automate à la raison ci-dessus, puis utilisez Retraiter.

admin-check-the-machine-s-settings-on-the-setup-page = Comparez les réglages de l’automate sur la page Configuration (modèle et connexion de l’automate) à la raison ci-dessus, puis utilisez Retraiter.

admin-compare-this-result-with-the-earlier-results-for-the = Comparez ce résultat aux résultats antérieurs du même échantillon sur l’automate. Si un réglage a causé l’écart, corrigez-le, puis utilisez Retraiter.

admin-the-machine-stopped-part-way-through-sending-check-the = L’automate s’est arrêté en cours d’envoi. Vérifiez le câble ou le réseau et faites renvoyer les résultats par l’automate, puis utilisez Retraiter ici.

admin-the-lab-app-already-holds-different-content-for-this = L’application de laboratoire détient déjà un contenu différent pour cette transmission ; Analyte ne la renvoie pas de lui-même. Vérifiez la transmission dans l’application de laboratoire et réglez-la là-bas, puis utilisez Marquer comme résolu.

admin-the-lab-app-sent-an-older-or-changed-copy = L’application de laboratoire a envoyé une copie plus ancienne ou modifiée d’une demande qu’Analyte possède déjà ; Analyte a gardé sa copie. Corrigez la demande dans l’application de laboratoire, puis utilisez Marquer comme résolu.

admin-the-lab-app-cancelled-tests-the-machine-has-already = L’application de laboratoire a annulé des analyses pour lesquelles l’automate a déjà rendu des résultats ; Analyte n’envoie plus rien à l’automate. Vérifiez les deux, sur l’automate et dans l’application de laboratoire, puis utilisez Marquer comme résolu.

admin-analyte-starts-no-other-exchange-with-this-machine-until = Analyte ne commence aucun autre échange avec cet automate tant que cet élément n’est pas résolu. Vérifiez sur l’automate si celui-ci s’est terminé, puis utilisez Marquer comme résolu.

admin-analyte-did-not-send-the-lab-app-this-order = Analyte n’a pas envoyé à l’application de laboratoire la modification de cette demande, car la demande est plus ancienne que le modèle d’automate qu’Analyte nomme désormais dans chaque événement. Communiquez à l’équipe de l’application de laboratoire l’état de la demande affiché ici, puis utilisez Marquer comme résolu.

admin-test-or-unit-not-mapped = Analyse ou unité sans correspondance

admin-test-or-unit-not-mapped-in-line = analyse ou unité sans correspondance

admin-could-not-read-message = Impossible de lire le message

admin-could-not-read-message-in-line = impossible de lire le message

admin-result-could-not-be-matched-to-earlier-results = Le résultat n’a pas pu être rapproché des résultats antérieurs

admin-result-could-not-be-matched-to-earlier-results-in-line = le résultat n’a pas pu être rapproché des résultats antérieurs

admin-message-from-the-machine-was-incomplete = Le message de l’automate était incomplet

admin-message-from-the-machine-was-incomplete-in-line = le message de l’automate était incomplet

admin-lab-app-rejected-a-delivery = L’application de laboratoire a refusé une transmission

admin-lab-app-rejected-a-delivery-in-line = l’application de laboratoire a refusé une transmission

admin-lab-app-reported-a-conflicting-delivery = L’application de laboratoire a signalé une transmission en conflit

admin-lab-app-reported-a-conflicting-delivery-in-line = l’application de laboratoire a signalé une transmission en conflit

admin-order-from-the-lab-app-was-refused = Une demande de l’application de laboratoire a été refusée

admin-order-from-the-lab-app-was-refused-in-line = une demande de l’application de laboratoire a été refusée

admin-cancel-conflicts-with-what-the-machine-reported = L’annulation est en conflit avec ce que l’automate a signalé

admin-cancel-conflicts-with-what-the-machine-reported-in-line = l’annulation est en conflit avec ce que l’automate a signalé

admin-order-event-could-not-name-its-machine-model = L’événement de demande n’a pas pu nommer son modèle d’automate

admin-order-event-could-not-name-its-machine-model-in-line = l’événement de demande n’a pas pu nommer son modèle d’automate

admin-settle-the-problem-described-above-with-the-lab-app = Réglez le problème décrit ci-dessus avec l’équipe de l’application de laboratoire, puis utilisez Renvoyer.

admin-the-item-leaves-the-open-list-and-stays-under = L’élément quitte la liste des éléments ouverts et reste dans Résolus avec son historique. Rien n’est supprimé, et rien n’est envoyé à un automate ni à l’application de laboratoire.

admin-analyte-reads-the-original-message-again-with-the-current = Analyte relit le message d’origine avec les réglages actuels. La lecture précédente est conservée et une nouvelle est ajoutée ; les résultats corrigés suivent les règles de correction habituelles. Rien n’est envoyé à l’automate.

admin-item-is-marked-resolved = L’élément { $id } est marqué comme résolu

admin-it-moved-to-resolved-and-keeps-its-history-nothing = Il est passé dans Résolus et garde son historique. Rien n’a été envoyé à un automate ni à l’application de laboratoire.

admin-reading-was-recorded = La lecture { $version } a été enregistrée

admin-what-happened-the-earlier-reading-is-kept = Ce qui s’est passé : { $outcomes }. La lecture précédente est conservée.

admin-mark-as-resolved = Marquer comme résolu ?

admin-for-example-settled-with-the-lab-app-team = Par exemple : réglé avec l’équipe de l’application de laboratoire

admin-process-this-message-again = Retraiter ce message ?

admin-for-example-machine-definition-corrected = Par exemple : définition de l’automate corrigée

admin-required-up-to-256-characters-recorded-in-the-history = Obligatoire, 256 caractères au maximum. Enregistrée dans l’historique avec l’heure.

admin-no-eligible-measurement-comparison-is-available = Aucune comparaison de mesures admissible n’est disponible.

admin-the-retained-results-could-not-be-displayed = Les résultats retenus n’ont pas pu être affichés.

admin-when-analyte-cannot-handle-a-message-or-result-on = Quand Analyte ne peut pas traiter seul un message ou un résultat, il le conserve ici avec la marche à suivre.

admin-when-someone-resolves-this-item-or-processes-it-again = Quand quelqu’un résout cet élément ou le retraite, l’heure, l’action et la raison apparaissent ici.

admin-linked-records-to-of = Enregistrements liés { $saturating-add } à { $end } sur { $total }.

admin-linked-record-2 = Enregistrement lié { $number }

admin-arrived-from = Arrivé de { $machine }

admin-category-as-stored = Catégorie telle que stockée : { $kind }

admin-linked-record-3 = Enregistrement lié : { $target }

admin-recorded-at-unix-by = Enregistré à Unix { $time } : { $action } par { $actor }

admin-resolution-as-stored = Résolution telle que stockée : { $stored }

admin-resolved-today = Résolus aujourd’hui

admin-open = Ouverts

admin-resolved = Résolus

admin-resolved-in-line = résolus

admin-no-more-items = Aucun autre élément

admin-this-page-is-past-the-last-item = Cette page va au-delà du dernier élément.

admin-nothing-needs-review = Rien n’est à vérifier.

admin-see-resolved-items = Voir les éléments résolus

admin-no-resolved-items-yet = Aucun élément résolu pour l’instant.

admin-items-marked-resolved-or-processed-again-move-here-and = Les éléments marqués comme résolus ou retraités arrivent ici et gardent leur historique.

admin-see-open-items = Voir les éléments ouverts

admin-resolved-how-it-was-resolved-could-not-be-read = Résolu ; la façon dont il a été résolu n’a pas pu être lue

admin-older-entries-are-kept-but-not-shown-here = Les entrées plus anciennes sont conservées mais ne sont pas affichées ici.

admin-item-to-review = Élément à vérifier

admin-none-recorded = Aucun enregistré

admin-removed-by-data-keeping = Supprimé par la conservation des données

admin-not-a-stored-message-its-id-is-under-details = Pas un message stocké ; son ID figure dans Détails pour l’assistance

admin-show-the-message-as-stored = Afficher le message tel que stocké

admin-message-removed-by-data-keeping = Message supprimé par la conservation des données

admin-analyte-stored-the-message = Analyte a stocké le message

admin-how-it-was-resolved-could-not-be-read-the = La façon dont il a été résolu n’a pas pu être lue ; l’enregistrement stocké figure dans Détails pour l’assistance.

admin-source-cursor-offset = Source : { $source }, curseur { $after }, décalage { $offset }

admin-view-recorded-for = Consultation enregistrée pour : { $actor }

admin-the-message-as-stored = Le message tel que stocké

admin-the-pieces-as-they-arrived-from-the-machine = Les fragments tels qu’ils sont arrivés de l’automate

admin-show-the-next-part = Afficher la partie suivante

admin-admin-connection-lifetime-exceeded = durée de vie de la connexion d’administration dépassée

admin-its-words-could-not-be-read-within-bytes = son texte n’a pas pu être lu dans la limite de { $PLAIN-LIMIT } octets : { $error }

admin-csrf-session-required = session CSRF requise

admin-csrf-token-invalid-or-missing = jeton CSRF invalide ou manquant

admin-sign-in-page-failed = échec de la page de connexion : { $render }

admin-this-is-not-the-admin-key-check-it-and = Ce n’est pas la clé d’administration. Vérifiez-la et réessayez.

admin-analyte-could-not-read-the-form-enter-the-key = Analyte n’a pas pu lire le formulaire. Saisissez à nouveau la clé.

admin-windows-sign-in-did-not-work = La connexion Windows n’a pas fonctionné

admin-the-form-was-sent-without-a-signed-in-session = Le formulaire a été envoyé sans session connectée. Connectez-vous, puis rouvrez la page et refaites ce que vous avez fait.

admin-your-windows-account-is-not-allowed-to-use-analyte = Votre compte Windows n’est pas autorisé à utiliser Analyte. Demandez à un administrateur de l’ajouter au groupe des utilisateurs d’Analyte, ou connectez-vous avec la clé d’administration.

admin-the-analyte-app-asked-in-a-way-this-engine = L’application Analyte a fait une demande sous une forme que ce moteur ne comprend pas. Connectez-vous avec la clé d’administration et demandez au service informatique de mettre à jour l’application.

admin-windows-could-not-say-which-account-is-asking-sign = Windows n’a pas pu indiquer quel compte fait la demande. Connectez-vous avec la clé d’administration.

admin-windows-sign-in-is-not-available-right-now-try = La connexion Windows n’est pas disponible pour le moment. Réessayez dans un instant, ou connectez-vous avec la clé d’administration.

admin-this-sign-in-has-expired = Cette connexion a expiré

admin-a-windows-sign-in-works-once-within-20-seconds = Une connexion Windows ne fonctionne qu’une fois, dans les 20 secondes. Rouvrez l’application Analyte, ou connectez-vous avec la clé d’administration.

admin-windows-sign-in-works-in-the-analyte-app = La connexion Windows fonctionne dans l’application Analyte

admin-in-a-browser-sign-in-with-the-admin-key = Dans un navigateur, connectez-vous avec la clé d’administration.

admin-the-sign-in-page-could-not-be-shown-give = La page de connexion n’a pas pu être affichée. Transmettez les détails ci-dessous à l’assistance.

admin-overview = Vue d’ensemble

admin-activity = Activité

admin-needs-review = À vérifier

admin-machines = Automates

admin-lab-apps-2 = Applications de laboratoire

admin-setup = Configuration

admin-machine-orders = Demandes aux automates

admin-cancelled-by-machine = Annulé par l’automate

admin-deliveries = Transmissions

admin-data-keeping = Conservation des données

admin-settings = Paramètres

admin-back-to-overview = Retour à la vue d’ensemble

admin-back-to-activity = Retour à l’activité

admin-back-to-needs-review = Retour à « À vérifier »

admin-back-to-machines = Retour aux automates

admin-back-to-lab-apps = Retour aux applications de laboratoire

admin-back-to-setup = Retour à la configuration

admin-back-to-machine-orders = Retour aux demandes aux automates

admin-back-to-cancelled-by-machine = Retour à « Annulé par l’automate »

admin-back-to-deliveries = Retour aux transmissions

admin-back-to-data-keeping = Retour à la conservation des données

admin-back-to-settings = Retour aux paramètres

admin-good = Bon

admin-problem = Problème

admin-note = Remarque

admin-is-not-allowed-at-this-address = { $method } n’est pas autorisé à cette adresse

admin-details-for-support-the-page-failed-too = { $title }{ "\u000A" }{ "\u000A" }{ $text }{ "\u000A" }{ "\u000A" }Détails pour le support : { $detail } ; la page a aussi échoué : { $render }

admin-go-to-overview = Aller à la vue d’ensemble

admin-try-again = Réessayer

admin-page-not-found = Page introuvable

admin-analyte-has-no-page-at-this-address-check-the = Analyte n’a aucune page à cette adresse. Vérifiez l’adresse ou allez à la vue d’ensemble.

admin-file-not-found = Fichier introuvable

admin-analyte-has-no-file-at-this-address = Analyte n’a aucun fichier à cette adresse.

admin-this-cannot-be-done-here = Cette action n’est pas possible ici

admin-analyte-does-not-take-this-kind-of-request-at = Analyte n’accepte pas ce type de requête à cette adresse. Rien n’a été modifié.

admin-this-could-not-be-read = Lecture impossible

admin-analyte-could-not-read-the-address-or-the-form = Analyte n’a pas pu lire l’adresse ou le formulaire envoyé. Rien n’a été modifié. Revenez en arrière et réessayez.

admin-this-form-has-expired = Ce formulaire a expiré

admin-go-back-reload-the-page-and-try-again-nothing = Revenez en arrière, rechargez la page et réessayez. Rien n’a été modifié.

admin-too-much-text = Trop de texte

admin-the-form-holds-more-text-than-analyte-accepts-shorten = Le formulaire contient plus de texte qu’Analyte n’en accepte. Raccourcissez ce que vous avez saisi et réessayez.

admin-this-took-too-long = Cette opération a pris trop de temps

admin-analyte-did-not-finish-in-time-check-the-page = Analyte n’a pas terminé à temps. Vérifiez la page avant de réessayer : une action a peut-être déjà été enregistrée.

admin-analyte-needs-a-restart = Analyte doit être redémarré

admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte = Analyte ne peut plus vérifier les sessions du navigateur. Redémarrez Analyte, puis reconnectez-vous.

admin-you-cannot-sign-in-right-now = Vous ne pouvez pas vous connecter pour le moment

admin-too-many-browser-sessions-are-open-wait-a-few = Trop de sessions de navigateur sont ouvertes. Attendez quelques minutes et réessayez, ou redémarrez Analyte.

admin-analyte-could-not-start-a-browser-session-try-again = Analyte n’a pas pu ouvrir de session de navigateur. Réessayez ; si le problème persiste, transmettez les détails ci-dessous au support.

admin-you-cannot-sign-out-right-now = Vous ne pouvez pas vous déconnecter pour le moment

admin-analyte-cannot-check-browser-sessions-any-more-restart-analyte-2 = Analyte ne peut plus vérifier les sessions du navigateur. Redémarrez Analyte ; cela déconnecte tout le monde.

admin-too-many-live-pages = Trop de pages en direct

admin-analyte-keeps-a-few-pages-live-at-once-and = Analyte garde quelques pages en direct à la fois, et toutes sont utilisées. Les pages fonctionnent toujours ; rechargez-en une pour voir ce qui a changé, ou fermez un onglet.

admin-no-admin-page-at-this-address = aucune page d’administration à cette adresse

admin-static-asset-not-found = ressource statique introuvable

admin-admin-form-exceeds-limit = le formulaire d’administration dépasse la limite

admin-admin-request-deadline-exceeded = délai de la requête d’administration dépassé

admin-admin-session-lookup-failed = échec de la recherche de la session d’administration

admin-admin-session-header-invalid = en-tête de session d’administration non valide

admin-every-admin-live-stream-is-in-use = tous les flux en direct d’administration sont utilisés

admin-page-could-not-be-written = la page n’a pas pu être écrite : { $error }

admin-check-the-item-and-try-again-if-this-keeps = Vérifiez l’élément et réessayez ; si le problème persiste, transmettez les détails ci-dessous au support.

admin-process-again-reads-the-item-s-original-message-so = Le retraitement relit le message d’origine de l’élément, il en faut donc exactement un. Une fois l’élément réglé, utilisez « Marquer comme résolu ».

admin-admin-deadline-of-ms-exceeded = délai d’administration { $label } de { $as-millis } ms dépassé

admin-the-page-failed-too = { $alert } ; la page a aussi échoué : { $render }

admin-open-the-lab-app = Ouvrir l’application de laboratoire

admin-open-its-setup = Ouvrir sa configuration

admin-item-not-found = Élément introuvable

admin-there-is-no-item-with-this-number = Aucun élément ne porte ce numéro

admin-go-back-to-needs-review-and-pick-one-from = Revenez à « À vérifier » et choisissez-en un dans la liste.

admin-parked-item-not-found = élément mis de côté introuvable

admin-machine-not-found = Automate introuvable

admin-no-machine-is-called-this = Aucun automate ne porte ce nom

admin-go-back-to-machines-and-pick-one-from-the = Revenez aux automates et choisissez-en un dans la liste.

admin-machine-not-running-and-not-listed = automate non démarré et absent de la liste

admin-this-needs-a-signed-in-browser-session = Cette action nécessite une session de navigateur connectée

admin-removing-a-machine-changes-settings-so-it-needs-a = Retirer un automate modifie les paramètres, cela nécessite donc une session de navigateur connectée

admin-lab-apps-hold-addresses-and-keys-so-their-pages = Les applications de laboratoire contiennent des adresses et des clés, leurs pages nécessitent donc une session de navigateur connectée

admin-setup-changes-settings-so-its-pages-need-a-signed = La configuration modifie les paramètres, ses pages nécessitent donc une session de navigateur connectée

admin-analyte-records-who-sends-again-or-changes-a-hold = Analyte enregistre qui renvoie ou modifie une protection, cette action nécessite donc une session de navigateur connectée

admin-the-overview-could-not-be-shown-just-now = La vue d’ensemble n’a pas pu être affichée pour le moment

admin-reading-not-found = Lecture introuvable

admin-readings-are-numbered-from-1 = Les lectures sont numérotées à partir de 1

admin-open-the-message-again-from-its-machine-s-page = Rouvrez le message depuis la page de son automate.

admin-this-message-has-no-reading-with-this-number = Ce message n’a aucune lecture portant ce numéro

admin-its-machine-s-page-lists-its-messages-with-their = La page de son automate liste ses messages avec leur dernière lecture.

admin-sign-in-first = Connectez-vous d’abord

admin-sign-in-with-the-admin-key-then-open-the = Connectez-vous avec la clé d’administration, puis rouvrez la page.

admin-this-form-is-from-an-older-or-another-session = Ce formulaire provient d’une session plus ancienne ou d’une autre session

admin-open-the-item-again-and-repeat-the-action-if = Rouvrez l’élément et refaites l’action ; si la page le demande, connectez-vous d’abord.

admin-that-could-not-be-done = Cette action n’a pas pu être effectuée

admin-already-resolved = Déjà résolu

admin-this-item-is-already-resolved = Cet élément est déjà résolu

admin-its-history-on-the-item-page-says-when-it = Son historique, sur la page de l’élément, indique quand, comment et pourquoi il a été résolu.

admin-cannot-process-this-item-again = Impossible de retraiter cet élément

admin-this-item-is-not-linked-to-exactly-one-stored = Cet élément n’est pas lié à exactement un message stocké

admin-the-original-message-is-no-longer-kept = Le message d’origine n’est plus conservé

admin-data-keeping-removed-the-original-message = La conservation des données a supprimé le message d’origine

admin-it-cannot-be-shown-or-read-again-the-item = Il ne peut plus être affiché ni relu. L’élément et son historique restent.

admin-nothing-more-to-show = Rien de plus à afficher

admin-there-are-no-more-original-bytes-for-this-record = Il n’y a plus d’octets d’origine pour cet enregistrement

admin-go-back-to-the-item-to-see-its-other = Revenez à l’élément pour voir ses autres enregistrements liés.

admin-analyte-could-not-read-its-settings-just-now = Analyte n’a pas pu lire ses paramètres pour le moment

admin-analyte-could-not-read-its-stored-data-just-now = Analyte n’a pas pu lire ses données stockées pour le moment

admin-analyte-took-too-long = Analyte a pris trop de temps

admin-positive-interpretation-version-required = une version d’interprétation positive est requise

admin-interpretation-not-found = interprétation introuvable

admin-authenticated-session-required = session authentifiée requise

admin-authenticated-csrf-session-required = session CSRF authentifiée requise

admin-parked-item-already-resolved = élément mis de côté déjà résolu

admin-linked-receipt-purged-by-retention = réception liée supprimée par la purge de conservation

admin-no-linked-raw-evidence-at-this-cursor = aucune donnée brute liée à ce curseur

admin-this-page-could-not-be-shown-give-the-details = Cette page n’a pas pu être affichée. Transmettez les détails ci-dessous au support.

admin-item-added = – élément { $item } ajouté : { $title }

admin-item-3 = – élément { $item } { $action }

admin-order-message-2 = – message de la demande { $order }{ " " }

admin-order-3 = – demande { $order }{ " " }

admin-sample-2 = – échantillon { $sample }{ " " }

admin-sample-3 = – échantillon { $shown }{ " " }

admin-delivered-to = transmis à { $lab-app }

admin-sending-to-failed-will-retry = échec de l’envoi à { $lab-app }, nouvelle tentative prévue

admin-sending-to-was-cut-off-will-retry = envoi à { $lab-app } interrompu, nouvelle tentative prévue

admin-rejected-by = refusé par { $lab-app }

admin-sending-to-paused-lab-app-settings-need-repair = envoi à { $lab-app } en pause : les paramètres de l’application de laboratoire doivent être corrigés

admin-reports-a-conflict = { $lab-app } signale un conflit

admin-sending-to-tried-times =
    { $outcome ->
        [cut-off] envoi à { $lab-app } : interrompu · { $count } tentatives, { $first } – { $last }
       *[failed] envoi à { $lab-app } : échec · { $count } tentatives, { $first } – { $last }
    }

admin-next-try-after = { " " }· prochaine tentative après { $next }

admin-added-machine = a ajouté l’automate { $named }

admin-saved-the-settings-of = a enregistré les paramètres de { $named }

admin-removed-machine-2 = a retiré l’automate { $named }

admin-confirmed-the-settings-of-for-its-reported-software = a confirmé les paramètres de { $named } pour le logiciel qu’il signale

admin-added-lab-app = a ajouté l’application de laboratoire { $named }

admin-saved-lab-app = a enregistré l’application de laboratoire { $named }

admin-removed-lab-app = a retiré l’application de laboratoire { $named }

admin-recorded-3 = a enregistré { $action }

admin-every-try = Toutes les tentatives

admin-change = Modification

admin-stored = Stocké

admin-message-stored = message stocké

admin-message-stored-could-not-be-read = message stocké, lecture impossible

admin-not-read-yet = Pas encore lu

admin-message-stored-not-read-yet = message stocké, pas encore lu

admin-no-results-2 = aucun résultat

admin-not-recorded-2 = (non enregistré)

admin-asked-analyte-to-restart = a demandé le redémarrage d’Analyte

admin-placed-a-hold-under-data-keeping = a posé une protection dans « Conservation des données »

admin-released-a-hold-under-data-keeping = a levé une protection dans « Conservation des données »

admin-removed-data-past-its-keeping-time = a supprimé des données dont la durée de conservation était dépassée

admin-queued-a-delivery-to-send-again = a mis une transmission en file d’attente pour renvoi

admin-processed-a-message-again = a retraité un message

admin-recovered-an-interrupted-delivery = a repris une transmission interrompue

admin-restored-the-store-from-a-backup = a restauré le stockage à partir d’une sauvegarde

admin-settled-an-order-s-outcome = a réglé l’issue d’une demande

admin-retried-an-alert = a relancé une alerte

admin-reading-recorded =  : lecture { $version } enregistrée

admin-by-at = { $what } par { $actor } ·{ " " }

admin-from-arrived = De { $machine } · arrivé{ " " }

admin-admin-session = session d’administration

admin-it-still-needs-review =  ; une vérification reste nécessaire

admin-recorded-as-a-resend = Enregistré comme renvoi

admin-recorded-as-a-resend-in-line = enregistré comme renvoi

admin-rerun-queued = Réanalyse en file d’attente

admin-rerun-queued-in-line = réanalyse en file d’attente

admin-correction-queued = Correction en file d’attente

admin-correction-queued-in-line = correction en file d’attente

admin-superseded-by-the-receipt-with-a-tube-id = Remplacé par la réception avec un ID de tube

admin-superseded-by-the-receipt-with-a-tube-id-in-line = remplacé par la réception avec un ID de tube

admin-no-results-in-it = aucun résultat dedans

admin-oldest = { $items } · le plus ancien{ " " }

admin-oldest-not-recorded = { $items } · le plus ancien non enregistré

admin-key-missing = Clé manquante : { $one }.

admin-keys-missing-and = Clés manquantes : { $join } et { $last }.

admin-set-at = Défini dans { $set-at }

admin-set-in-the-service-environment = Défini dans l’environnement du service ({ $variable })

admin-last-delivered = { "Dernière transmission " }

admin-no-delivery-in-the-kept-records = Aucune transmission dans les enregistrements conservés

admin-nothing-delivered-yet = Rien n’a encore été transmis

admin-last-try = { "Dernière tentative " }

admin-next-try = { "prochaine tentative " }

admin-next-try-due = prochaine tentative imminente

admin-a-try-is-under-way = une tentative est en cours

admin-no-set-up-machine-sends-here = Aucun automate configuré n’envoie ici

admin-its-keys-are-set-now-analyte-uses-them-after = Ses clés sont maintenant définies ; Analyte les utilisera après son redémarrage.

admin-set-on-date = Défini le { $day } { $month }

admin-set-on-date-year = Défini le { $day } { $month } { $year }

admin-scheduled-on-the-machine = Planifiée sur l’automate

admin-in-progress-on-the-machine = En cours sur l’automate

admin-cancelled-by-the-machine = Annulée par l’automate

admin-not-sent-yet = Pas encore envoyée

admin-not-sent-yet-in-line = pas encore envoyée

admin-sent-not-yet-accepted = Envoyée, pas encore acceptée

admin-sent-not-yet-accepted-in-line = envoyée, pas encore acceptée

admin-accepted-by-the-machine = Acceptée par l’automate

admin-accepted-by-the-machine-in-line = acceptée par l’automate

admin-rejected-by-the-machine = Refusée par l’automate

admin-rejected-by-the-machine-in-line = refusée par l’automate

admin-cancel-requested = Annulation demandée

admin-cancel-requested-in-line = annulation demandée

admin-outcome-unknown-check-the-machine = Issue inconnue : vérifiez l’automate

admin-outcome-unknown-check-the-machine-in-line = issue inconnue : vérifiez l’automate

admin-delivered = Transmis

admin-rejected-by-the-lab-app = Refusé par l’application de laboratoire

admin-paused-lab-app-settings-need-repair = En pause : les paramètres de l’application de laboratoire doivent être corrigés

admin-lab-app-reports-a-conflict = L’application de laboratoire signale un conflit

admin-not-sent-the-lab-app-takes-no-notices = Non envoyé : l’application de laboratoire n’accepte pas les avis

admin-will-retry = Nouvelle tentative prévue

admin-serial = n° de série { $serial }

admin-port = port { $port }

admin-kept-since-the-start-export-the-recordings-with-analyte = { $messages } conservés, { $exchanges } depuis le démarrage ; exportez les enregistrements avec analyte captures export

admin-refused-connections-from-the-last-at = { $count } connexions refusées depuis { $peer }, la dernière à { $at }

admin-refused-a-connection-from-at = Connexion refusée depuis { $peer } à { $at }

admin-leaves-a-report-s-status-empty-when-the-report = Laisse vide le statut d’un compte rendu lorsque celui-ci est définitif ; Analyte ne considère comme définitif qu’un statut de compte rendu vide.

admin-leaves-a-result-s-status-empty-when-the-result = Laisse vide le statut d’un résultat lorsque celui-ci est définitif ; Analyte ne considère comme définitif qu’un statut de résultat vide.

admin-puts-several-records-in-one-frame-analyte-reads-each = Place plusieurs enregistrements dans une même trame ; Analyte lit chaque enregistrement une fois le contrôle de la trame réussi.

admin-marks-its-text-as-utf-8-a-name-its = Marque son texte comme UTF-8, un nom que sa version HL7 ne connaissait pas encore ; Analyte lit le texte en UTF-8.

admin-uses-escape-codes-analyte-does-not-translate-analyte-keeps = Utilise des codes d’échappement qu’Analyte ne traduit pas ; Analyte les conserve tels quels et ne s’en sert jamais pour identifier un échantillon ou un résultat.

admin-sends-frames-of-a-size-other-than-the-standard = Envoie des trames d’une taille différente de celle de la norme ; Analyte accepte les trames jusqu’à la limite fixée dans les paramètres de cet automate.

admin-while-the-machine-sends-analyte-waits-for-each-part = Pendant l’envoi par l’automate, Analyte attend chaque partie aussi longtemps que l’indiquent les paramètres de cet automate, et non la valeur par défaut de la norme.

admin-while-analyte-sends-it-waits-for-the-machine-s = Pendant qu’Analyte envoie, il attend les réponses de l’automate aussi longtemps que l’indiquent les paramètres de cet automate, et non la valeur par défaut de la norme.

admin-writes-check-digits-in-lowercase-analyte-accepts-them-only = Écrit les caractères de contrôle en minuscules ; Analyte ne les accepte que si le contrôle est correct.

admin-of-open = { $open } sur { $links } ouvertes

admin-of-delivered = { " " }· { $done } sur { $total } transmis

admin-delivered-2 = { $total } transmis

admin-port-open-since = Port { $port } ouvert depuis { $since }

admin-port-not-open-since = Port { $port } non ouvert depuis { $since }

admin-port-of-usb-adapter-open-since = Port de l’adaptateur USB { $serial } ouvert depuis { $since }

admin-port-of-usb-adapter-not-open-since = Port de l’adaptateur USB { $serial } non ouvert depuis { $since }

admin-machine-connected-since = Automate connecté{ $port } depuis { $since }

admin-listening-nothing-connected-since = En écoute{ $port }, rien de connecté depuis { $since }

admin-not-listening-since = Pas en écoute{ $port } depuis { $since }

admin-connected-to-since = Connecté à { $machine } depuis { $since }

admin-no-connection-to-since = Aucune connexion à { $machine } depuis { $since }

admin-watching = Surveillance de { $folder }

admin-not-watching-since = { $folder } non surveillé depuis { $since }

admin-on-port = { " " }sur le port { $port }

admin-check-that-the-usb-adapter-or-cable-is-plugged = Vérifiez que l’adaptateur ou le câble USB est branché sur ce PC, que { $port } est le bon port et qu’aucun autre programme ne l’utilise.

admin-check-that-the-usb-adapter-with-serial-is-plugged = Vérifiez que l’adaptateur USB portant le numéro de série { $serial } est branché sur ce PC et qu’aucun autre programme n’utilise son port.

admin-only = Uniquement { $addresses }

admin-serial-2 = Port série

admin-usb-adapter = Adaptateur USB

admin-network = Réseau

admin-watched-folder = Dossier surveillé

admin-empty-h-12-in-a-documented-upload-is-treated = Un champ H.12 vide dans un envoi documenté est traité comme production.

admin-one-repeated-enq-before-any-frame-can-recover-a = Un seul ENQ répété avant toute trame peut rattraper un ACK d’établissement perdu.

admin-answer-record-format-hl7-barcode-queries = Répondre aux requêtes HL7 de code-barres au format enregistrement

admin-reads-combined-patient-values-and-divides-control-uploads-into = Lit les valeurs patient combinées et divise les envois de contrôle en leurs lignes d’origine.

admin-puts-the-upload-category-in-msh-16-analyte-reads = Place la catégorie d’envoi dans MSH-16. Analyte lit cette catégorie dans les envois de résultats.

admin-ends-each-message-line-with-cr-lf-instead-of = Termine chaque ligne de message par CR LF au lieu de CR ; Analyte lit les lignes ainsi.

admin-ends-each-message-line-with-lf-instead-of-cr = Termine chaque ligne de message par LF au lieu de CR ; Analyte lit les lignes ainsi.

admin-when-both-sides-start-at-the-same-moment-analyte = Lorsque les deux côtés commencent au même moment, Analyte passe en premier à la place de l’automate.

admin-found-by-its-usb-serial-number-so-it-keeps = Retrouvé par son numéro de série USB, il continue donc de fonctionner si Windows donne un autre nom de port à l’adaptateur.

admin-last-data-2 = { "dernières données " }

admin-last-data-3 = { "Dernières données " }

admin-nothing-received-yet-2 = rien reçu pour l’instant

admin-could-not-be-read = Lecture impossible

admin-read-no-results = Lu · aucun résultat

admin-check-the-network-cable-and-the-machine-s-host = Vérifiez le câble réseau et les paramètres d’hôte de l’automate.

admin-check-that-the-folder-exists-and-that-analyte-s = Vérifiez que le dossier existe et que le compte de service d’Analyte peut le lire.

admin-lab-app-answer-http = Réponse de l’application de laboratoire : HTTP { $status }

admin-record = enregistrement { $record }

admin-record-field-2 = enregistrement { $record }, champ { $field }

admin-the-machine-sent-no-test-code-in = L’automate n’a envoyé aucun code d’analyse dans { $at }.

admin-the-message-has-no-sample-id-in = Le message ne contient aucun ID d’échantillon dans { $at }.

admin-the-value-this-machine-s-settings-use-to-tell = La valeur que les paramètres de cet automate utilisent pour distinguer les résultats est absente dans { $at }.

admin-the-value-in-is-of-a-kind-analyte-cannot = La valeur dans { $at } est d’un type qu’Analyte ne peut pas lire de façon sûre.

admin-the-processing-mode-in-is-missing-or-not-one = Le mode de traitement dans { $at } est absent ou inconnu d’Analyte.

admin-the-time-in-could-not-be-read-the-text = L’heure dans { $at } n’a pas pu être lue ; le texte tel qu’envoyé est conservé.

admin-the-result-s-final-status-comes-from-what-this = Le statut définitif du résultat provient de ce que cet automate est documenté comme envoyant, et non du message, et le contrat de l’application de laboratoire utilisé ne permet pas de l’indiquer.

admin-part-of-this-message-needs-review-and-this-machine = Une partie de ce message doit être vérifiée, et les résultats de cet automate sont enregistrés tous ensemble ou pas du tout ; aucun n’a donc été enregistré.

admin-results-in-this-message-conflict-with-results-this-machine = Des résultats de ce message sont en conflit avec des résultats envoyés plus tôt par cet automate ; aucun n’a donc été enregistré.

admin-the-connection-timed-out-or-closed-before-the-message = La connexion a expiré ou s’est fermée avant que le message soit complet ; la partie reçue est conservée.

admin-the-message-arrived-while-too-many-messages-were-waiting = Le message est arrivé alors que trop de messages attendaient d’être stockés ; Analyte l’a donc conservé ici sans le lire.

admin-the-machine-sent-an-edited-result-whose-content-looks = L’automate a envoyé un résultat modifié dont le contenu semble inchangé ; Analyte ne peut donc pas savoir ce qui a été modifié.

admin-the-machine-answered-that-it-could-not-cancel-a = L’automate a répondu qu’il ne pouvait pas annuler une analyse qu’il avait auparavant signalée comme annulée de lui-même.

admin-this-order-was-stored-before-analyte-kept-each-order = Cette demande a été stockée avant qu’Analyte ne conserve le modèle d’automate de chaque demande ; son événement vers l’application de laboratoire ne peut donc pas nommer le modèle. L’état de la demande sur cette page est conservé ; communiquez cet état à l’application de laboratoire.

admin-analyte-asked-this-machine-for-its-next-results-and = Analyte a demandé ses résultats suivants à cet automate, et l’automate les considère peut-être comme envoyés avant que leur réponse ait été stockée de façon sûre.

admin-test-code-missing = Code d’analyse manquant

admin-test-code-missing-in-line = code d’analyse manquant

admin-sample-id-missing = ID d’échantillon manquant

admin-sample-id-missing-in-line = ID d’échantillon manquant

admin-value-is-not-the-expected-kind = La valeur n’est pas du type attendu

admin-value-is-not-the-expected-kind-in-line = la valeur n’est pas du type attendu

admin-text-with-formatting-codes = Texte avec codes de mise en forme

admin-text-with-formatting-codes-in-line = texte avec codes de mise en forme

admin-result-classified-more-than-one-way = Résultat classé de plusieurs façons

admin-result-classified-more-than-one-way-in-line = résultat classé de plusieurs façons

admin-status-only-update = Mise à jour du statut seul

admin-status-only-update-in-line = mise à jour du statut seul

admin-result-report-incomplete = Compte rendu de résultat incomplet

admin-result-report-incomplete-in-line = compte rendu de résultat incomplet

admin-result-status-not-final = Statut du résultat non définitif

admin-result-status-not-final-in-line = statut du résultat non définitif

admin-processing-the-message-again-with-the-current-settings-produced = Le retraitement du message avec les paramètres actuels n’a produit aucun résultat.

admin-piece-resent-with-other-content = Partie renvoyée avec un autre contenu

admin-piece-resent-with-other-content-in-line = partie renvoyée avec un autre contenu

admin-the-machine-sent-the-same-numbered-piece-of-a = L’automate a envoyé deux fois la même partie numérotée d’un message, avec un contenu différent.

admin-piece-arrived-out-of-turn = Partie arrivée hors séquence

admin-piece-arrived-out-of-turn-in-line = partie arrivée hors séquence

admin-a-piece-of-a-message-arrived-when-no-transfer = Une partie de message est arrivée alors qu’aucun transfert depuis l’automate n’était ouvert.

admin-message-not-expected = Message inattendu

admin-message-not-expected-in-line = message inattendu

admin-a-message-arrived-that-analyte-did-not-expect-or = Un message est arrivé alors qu’Analyte ne l’attendait pas, ou alors que trop de messages étaient en attente.

admin-processing-interrupted = Traitement interrompu

admin-processing-interrupted-in-line = traitement interrompu

admin-processing-of-this-message-stopped-before-it-was-confirmed = Le traitement de ce message s’est arrêté avant d’être confirmé comme terminé.

admin-returned-work-differs-from-the-order-sent = Le travail renvoyé diffère de la demande envoyée

admin-returned-work-differs-from-the-order-sent-in-line = le travail renvoyé diffère de la demande envoyée

admin-the-work-the-machine-returned-differs-from-the-order = Le travail renvoyé par l’automate diffère de la demande qu’Analyte lui a envoyée.

admin-results-held-back-for-review = Résultats retenus pour vérification

admin-results-held-back-for-review-in-line = résultats retenus pour vérification

admin-results-conflict-with-earlier-ones = Résultats en conflit avec des résultats antérieurs

admin-results-conflict-with-earlier-ones-in-line = résultats en conflit avec des résultats antérieurs

admin-message-too-large = Message trop volumineux

admin-message-too-large-in-line = message trop volumineux

admin-the-message-was-larger-than-the-size-limit-for = Le message dépassait la taille limite fixée pour cet automate.

admin-the-connection-was-stopped-while-the-message-was-arriving = La connexion a été arrêtée pendant l’arrivée du message ; la partie reçue est conservée.

admin-message-not-finished-in-time = Message non terminé à temps

admin-message-not-finished-in-time-in-line = message non terminé à temps

admin-the-machine-did-not-finish-sending-the-message-in = L’automate n’a pas terminé l’envoi du message à temps ; la partie reçue est conservée.

admin-message-arrived-while-busy = Message arrivé pendant une surcharge

admin-message-arrived-while-busy-in-line = message arrivé pendant une surcharge

admin-check-characters-do-not-match = Les caractères de contrôle ne correspondent pas

admin-check-characters-do-not-match-in-line = les caractères de contrôle ne correspondent pas

admin-the-check-characters-at-the-end-of-the-message = Les caractères de contrôle à la fin du message ne correspondent pas à son contenu.

admin-transfer-cut-off-or-refused = Transfert interrompu ou refusé

admin-transfer-cut-off-or-refused-in-line = transfert interrompu ou refusé

admin-the-transfer-from-the-machine-did-not-finish-or = Le transfert depuis l’automate ne s’est pas terminé ou a été refusé ; ce qui est arrivé est conservé.

admin-edited-result-kept-for-review = Résultat modifié conservé pour vérification

admin-edited-result-kept-for-review-in-line = résultat modifié conservé pour vérification

admin-transfer-cut-off = Transfert interrompu

admin-transfer-cut-off-in-line = transfert interrompu

admin-the-machine-stopped-sending-before-the-message-was-complete = L’automate a cessé d’envoyer avant que le message soit complet ; les parties reçues sont conservées.

admin-transfer-cut-off-by-a-stop = Transfert interrompu par un arrêt

admin-transfer-cut-off-by-a-stop-in-line = transfert interrompu par un arrêt

admin-analyte-stopped-while-the-machine-was-sending-the-pieces = Analyte s’est arrêté pendant l’envoi par l’automate ; les parties qu’il avait confirmées sont conservées.

admin-lab-app-already-has-different-content = L’application de laboratoire a déjà un contenu différent

admin-lab-app-already-has-different-content-in-line = l’application de laboratoire a déjà un contenu différent

admin-the-lab-app-already-holds-different-content-under-this = L’application de laboratoire détient déjà un contenu différent sous l’ID de cette transmission.

admin-machine-s-cancel-answers-disagree = Les réponses d’annulation de l’automate se contredisent

admin-machine-s-cancel-answers-disagree-in-line = les réponses d’annulation de l’automate se contredisent

admin-order-event-not-sent = Événement de demande non envoyé

admin-order-event-not-sent-in-line = événement de demande non envoyé

admin-result-kept-for-review = Résultat conservé pour vérification

admin-result-kept-for-review-in-line = résultat conservé pour vérification

admin-already-received = déjà reçu

admin-delivery-stored-for-another-contract = Transmission stockée pour un autre contrat

admin-delivery-stored-for-another-contract-in-line = transmission stockée pour un autre contrat

admin-unexpected-answer-from-the-lab-app = Réponse inattendue de l’application de laboratoire

admin-unexpected-answer-from-the-lab-app-in-line = réponse inattendue de l’application de laboratoire

admin-this-pc-s-time-zone = Fuseau horaire de ce PC ({ $name })

admin-page-loaded = Page chargée { $zone }

admin-unix-time-not-a-valid-date = Heure Unix { $seconds } (date non valide)

admin-this-pc-s-time-zone-2 = Fuseau horaire de ce PC

admin-less-than-a-minute-ago = il y a moins d’une minute

admin-unknown-clock-moved-back = inconnu (l’horloge a reculé)

admin-off-for-other-users-because-the-analyte-users-group = Désactivé pour les autres utilisateurs, car le groupe des utilisateurs d’Analyte est absent ; seuls les administrateurs peuvent donc se connecter avec Windows

admin-off-because = Désactivé, car { $why }

admin-on = Activé

admin-cannot-send-to-lab-app-its-api-key-or = Impossible d’envoyer à l’application de laboratoire { $destination } : sa clé API ou sa clé de signature est manquante. Saisissez-les sur la page des applications de laboratoire (ou dans l’environnement du service), puis redémarrez. { admin-kept-and-never-deleted }

admin-cannot-send-to-lab-app-it-is-not-set = Impossible d’envoyer à l’application de laboratoire { $destination } : elle n’est plus configurée. { $from } : { $items } { admin-kept-item-agreement }. Ajoutez l’application de laboratoire { $destination } sur la page des applications de laboratoire, puis redémarrez, pour les envoyer.

admin-wait-for-lab-app-they-are-sent-after-the = { $from } : { $items } en attente de l’application de laboratoire { $destination } ; ils seront envoyés après le prochain redémarrage.{ $held }

admin-count-failures =
    { $count ->
        [one] { $number } échec
        [many] { $number } d’échecs
       *[other] { $number } échecs
    }

admin-count-warnings =
    { $count ->
        [one] { $number } avertissement
        [many] { $number } d’avertissements
       *[other] { $number } avertissements
    }

admin-count-tests =
    { $count ->
        [one] { $number } analyse
        [many] { $number } d’analyses
       *[other] { $number } analyses
    }

admin-count-notes =
    { $count ->
        [one] { $number } remarque
        [many] { $number } de remarques
       *[other] { $number } remarques
    }

admin-count-days =
    { $count ->
        [one] { $number } jour
        [many] { $number } de jours
       *[other] { $number } jours
    }

admin-count-bytes =
    { $count ->
        [one] { $number } octet
        [many] { $number } d’octets
       *[other] { $number } octets
    }

admin-count-items-need =
    { $count ->
        [one] { $number } élément à vérifier
        [many] { $number } d’éléments à vérifier
       *[other] { $number } éléments à vérifier
    }

admin-count-orders =
    { $count ->
        [one] { $number } demande
        [many] { $number } de demandes
       *[other] { $number } demandes
    }

admin-count-things-need =
    { $count ->
        [one] { $number } point demande votre attention
        [many] { $number } de points demandent votre attention
       *[other] { $number } points demandent votre attention
    }

admin-count-waiting-results =
    { $count ->
        [one] { $number } résultat en attente
        [many] { $number } de résultats en attente
       *[other] { $number } résultats en attente
    }

admin-count-results =
    { $count ->
        [one] { $number } résultat
        [many] { $number } de résultats
       *[other] { $number } résultats
    }

admin-count-failed-tries =
    { $count ->
        [one] { $number } tentative échouée
        [many] { $number } de tentatives échouées
       *[other] { $number } tentatives échouées
    }

admin-count-links =
    { $count ->
        [one] { $number } liaison
        [many] { $number } de liaisons
       *[other] { $number } liaisons
    }

admin-count-machines =
    { $count ->
        [one] { $number } automate
        [many] { $number } d’automates
       *[other] { $number } automates
    }

admin-count-lab-apps =
    { $count ->
        [one] { $number } application de laboratoire
        [many] { $number } d’applications de laboratoire
       *[other] { $number } applications de laboratoire
    }

admin-count-results-or-order-messages =
    { $count ->
        [one] { $number } résultat ou message de demande
        [many] { $number } de résultats ou messages de demande
       *[other] { $number } résultats ou messages de demande
    }

admin-the-test-code-in-does-not-split-into-the = Le code d’analyse dans { $at } ne se décompose pas selon les parties que nomme la définition de cet automate.

admin-the-text-in-carries-formatting-codes-analyte-keeps-them = Le texte dans { $at } contient des codes de mise en forme ; Analyte les conserve tels qu’envoyés et ne les convertit pas en texte brut.

admin-the-text-in-uses-an-escape-code-that-this = Le texte dans { $at } utilise un code d’échappement que les paramètres de cet automate ne savent pas interpréter.

admin-this-machine-s-quality-control-rules-give-the-result = Les règles de contrôle qualité de cet automate attribuent plusieurs classements au résultat dans { $at }.

admin-the-message-in-only-changes-a-result-s-status = Le message dans { $at } modifie seulement le statut d’un résultat, et Analyte ne peut pas encore enregistrer ce type de mise à jour.

admin-the-result-report-in-lacks-a-field-or-a = Il manque au compte rendu de résultat dans { $at } un champ, ou un statut de résultat, qu’un compte rendu doit avoir.

admin-the-value-in-is-not-a-number-but-this = La valeur dans { $at } n’est pas un nombre, alors que la définition de cet automate attend un nombre pour ce code d’analyse.

admin-the-value-in-is-not-a-code-but-this = La valeur dans { $at } n’est pas un code, alors que la définition de cet automate attend un code pour ce code d’analyse.

admin-the-value-in-is-neither-a-number-nor-a = La valeur dans { $at } n’est ni un nombre ni un code, alors que la définition de cet automate attend l’un des deux pour ce code d’analyse.

admin-the-result-status-in-is-not-final-but-this = Le statut du résultat dans { $at } n’est pas définitif, alors que cet automate est configuré pour n’envoyer que des résultats définitifs.

admin-the-result-was-recorded-as-but-kept-for-review = Le résultat a été enregistré comme { $what }, mais conservé pour vérification : il n’a pas d’ID d’échantillon, ou il contient une valeur qu’Analyte ne peut pas lire de façon sûre.

admin-the-lab-app-sent-version-of-an-order-after = L’application de laboratoire a envoyé la version { $offered } d’une demande alors qu’Analyte avait déjà la version { $kept } ; Analyte a conservé la version { $kept }.

admin-the-lab-app-sent-other-content-under-version-of = L’application de laboratoire a envoyé un autre contenu sous la version { $kept } d’une demande qu’Analyte avait déjà ; Analyte a conservé sa copie.

admin-not-named-in-its-definition = Non nommé dans sa définition

admin-only-when-its-program-entry-has-capture-true = Uniquement si son entrée dans le fichier du programme a capture = true

admin-awos = awos:{ $order }:{ $item }

admin-notes-after = notes-after:{ $after }

admin-not-recorded-older-data-analyte-does-not-guess-them = Non enregistré (données plus anciennes) ; Analyte ne les devine pas

admin-namespace-not-recorded = Espace de noms non enregistré

admin-a-cancel-request-was-pending-then =  ; une demande d’annulation était alors en attente

admin-kept-never-deleted = Conservé, jamais supprimé

admin-test-code-parts-do-not-match = Les parties du code d’analyse ne correspondent pas

admin-test-code-parts-do-not-match-in-line = les parties du code d’analyse ne correspondent pas

admin-value-cannot-be-read = Valeur illisible

admin-value-cannot-be-read-in-line = valeur illisible

admin-escape-code-not-agreed = Code d’échappement non convenu

admin-escape-code-not-agreed-in-line = code d’échappement non convenu

admin-processing-mode-unknown = Mode de traitement inconnu

admin-processing-mode-unknown-in-line = mode de traitement inconnu

admin-no-results-in-the-message = Aucun résultat dans le message

admin-no-results-in-the-message-in-line = aucun résultat dans le message

admin-time-cannot-be-read = Heure illisible

admin-time-cannot-be-read-in-line = heure illisible

admin-incomplete-astm-report = Compte rendu ASTM incomplet

admin-incomplete-astm-report-in-line = compte rendu ASTM incomplet

admin-the-analyzer-did-not-report-normal-completion = L’analyseur n’a pas signalé de fin normale.

admin-final-status-cannot-be-sent = Le statut définitif ne peut pas être envoyé

admin-final-status-cannot-be-sent-in-line = le statut définitif ne peut pas être envoyé

admin-this-machine-sends-no-run-number-that-stays-the = Cet automate n’envoie aucun numéro de passage qui reste identique, et son code patient n’est pas un ID d’échantillon fiable ; Analyte ne peut donc pas distinguer ce résultat des autres.

admin-result-identity-repeated = Identité de résultat répétée

admin-result-identity-repeated-in-line = identité de résultat répétée

admin-two-results-in-this-message-would-have-the-same = Deux résultats de ce message auraient la même identité, et aucun sous-ID ne les distingue.

admin-the-device-time-or-sequence-number-that-this-machine = L’appareil, l’heure ou le numéro de séquence que les paramètres de cet automate utilisent pour distinguer les résultats est absent.

admin-result-identity-not-trusted = Identité de résultat non fiable

admin-result-identity-not-trusted-in-line = identité de résultat non fiable

admin-this-machine-s-sequence-numbers-can-start-over-so = Les numéros de séquence de cet automate peuvent recommencer à zéro ; ils ne peuvent donc pas distinguer les résultats sans un passage et une révision fiables.

admin-this-machine-s-messages-carry-no-trusted-result-or = Les messages de cet automate ne contiennent aucune identité fiable de résultat ou de passage, et n’indiquent pas si un résultat est un résultat patient ou un résultat de contrôle qualité.

admin-processing-again-produced-no-results = Le retraitement n’a produit aucun résultat

admin-processing-again-produced-no-results-in-line = le retraitement n’a produit aucun résultat

admin-result-differs-from-an-earlier-one = Le résultat diffère d’un résultat antérieur

admin-result-differs-from-an-earlier-one-in-line = le résultat diffère d’un résultat antérieur

admin-a-result-came-with-other-content-under-the-same = Un résultat est arrivé avec un autre contenu sous la même identité qu’un résultat antérieur, et rien ne l’indique comme une correction.

admin-the-message-does-not-give-this-result-an-identity = Le message ne donne pas d’identité à ce résultat ; Analyte ne peut donc pas savoir s’il est nouveau, répété ou s’il s’agit d’une correction.

admin-the-delivery-was-stored-for-a-lab-app-contract = La transmission a été stockée pour une version du contrat de l’application de laboratoire que cette version d’Analyte ne peut pas envoyer.

admin-the-lab-app-answered-that-it-took-the-delivery = L’application de laboratoire a répondu qu’elle avait accepté la transmission, mais sa réponse ne respecte pas le contrat de l’application de laboratoire ; Analyte ne la considère donc pas comme transmise.

admin-the-lab-app-refused-the-delivery-but-its-answer = L’application de laboratoire a refusé la transmission, mais sa réponse ne respecte pas le contrat de l’application de laboratoire ; son motif est donc inconnu.

admin-the-lab-app-gave-an-answer-that-the-lab = L’application de laboratoire a donné une réponse que le contrat de l’application de laboratoire ne définit pas pour une transmission.

admin-older-order-version-refused = Version plus ancienne de la demande refusée

admin-older-order-version-refused-in-line = version plus ancienne de la demande refusée

admin-changed-order-refused = Demande modifiée refusée

admin-changed-order-refused-in-line = demande modifiée refusée

admin-error-awos-item-not-retained = élément AWOS non conservé

admin-error-session-capacity = nombre maximal de sessions d’administration atteint ; redémarrez ou attendez leur expiration

admin-error-admin-session-lock-poisoned = verrou de session d’administration empoisonné

admin-error-choose-place-or-release = choisissez poser ou lever

admin-error-choose-receipt-or-order = choisissez réception ou demande

admin-error-store-mismatch = la configuration actuelle pointe vers un autre stockage

admin-error-machine-cursor-overflow = dépassement du curseur de l’automate

admin-error-no-software-check = cet automate n’a pas de vérification de version du logiciel

admin-error-no-software-warning = cet automate n’a pas d’avertissement logiciel actif

admin-error-parked-target-cursor-overflow = dépassement du curseur de la cible mise de côté

admin-waiting-after-restart = « En attente » est normal après un redémarrage

admin-count-recorded-messages =
    { $count ->
        [one] { $number } message
        [many] { $number } de messages
       *[other] { $number } messages
    }

admin-count-failed-exchanges =
    { $count ->
        [one] { $number } échange échoué
        [many] { $number } d’échanges échoués
       *[other] { $number } échanges échoués
    }

admin-count-machine-problems =
    { $count ->
        [one] { $number } problème
        [many] { $number } de problèmes
       *[other] { $number } problèmes
    }

admin-count-machines-waiting =
    { $count ->
        [one] { $number } en attente de l’automate
        [many] { $number } en attente de l’automate
       *[other] { $number } en attente de l’automate
    }

admin-count-machines-awaiting-setup =
    { $count ->
        [one] { $number } pas encore configuré
        [many] { $number } pas encore configurés
       *[other] { $number } pas encore configurés
    }

admin-count-machines-start-on-restart =
    { $count ->
        [one] { $number } démarre après un redémarrage
        [many] { $number } démarrent après un redémarrage
       *[other] { $number } démarrent après un redémarrage
    }

admin-count-machines-stop-on-restart =
    { $count ->
        [one] { $number } retiré, s’arrête au redémarrage
        [many] { $number } retirés, s’arrêtent au redémarrage
       *[other] { $number } retirés, s’arrêtent au redémarrage
    }

admin-count-new-results =
    { $count ->
        [one] { $number } nouveau résultat
        [many] { $number } de nouveaux résultats
       *[other] { $number } nouveaux résultats
    }

admin-count-corrections =
    { $count ->
        [one] { $number } correction
        [many] { $number } de corrections
       *[other] { $number } corrections
    }

admin-count-already-received =
    { $count ->
        [one] { $number } résultat déjà reçu
        [many] { $number } de résultats déjà reçus
       *[other] { $number } résultats déjà reçus
    }

admin-count-conflicts =
    { $count ->
        [one] { $number } résultat en conflit conservé pour vérification
        [many] { $number } de résultats en conflit conservés pour vérification
       *[other] { $number } résultats en conflit conservés pour vérification
    }

admin-count-unresolved =
    { $count ->
        [one] { $number } résultat sans identité conservé pour vérification
        [many] { $number } de résultats sans identité conservés pour vérification
       *[other] { $number } résultats sans identité conservés pour vérification
    }

admin-count-waiting-items =
    { $count ->
        [one] { $number } élément en attente
        [many] { $number } d’éléments en attente
       *[other] { $number } éléments en attente
    }

admin-kept-item-agreement =
    { $count ->
        [one] est conservé et jamais supprimé
        [many] sont conservés et jamais supprimés
       *[other] sont conservés et jamais supprimés
    }

admin-retained-deliveries =
    { $status ->
        [missing-keys] { admin-cannot-send-to-lab-app-its-api-key-or }
        [not-configured] { admin-cannot-send-to-lab-app-it-is-not-set }
        [delivering] { admin-still-being-delivered-to-lab-app }
       *[waiting] { admin-wait-for-lab-app-they-are-sent-after-the }
    }

admin-message-holds-no-results = Le message ne contient aucun résultat.

admin-identity-missing = Identité de résultat manquante

admin-identity-missing-in-line = identité de résultat manquante

admin-message-cut-off = Message interrompu

admin-message-cut-off-in-line = message interrompu

admin-report-reload = Rechargez la page ; si le problème persiste, transmettez les détails ci-dessous au support.

admin-report-reload-after = { $reason } { admin-report-reload }

admin-duration-seconds = { $number } s

admin-duration-minutes = { $number } min

admin-duration-hours = { $number } h

admin-ago = il y a { $duration }

admin-clock-this-year = { $day } { $month } { $time }

admin-date-line = { $weekday } { $day } { $month } · { $time }

admin-feed-day-today = Aujourd’hui · { $weekday } { $day } { $month }

admin-feed-day = { $weekday } { $day } { $month }

admin-feed-day-year = { $weekday } { $day } { $month } { $year }

admin-support-link = Liaison : { $link }

admin-item-number = Élément { $index }

admin-namespace = Espace de noms { $namespace }

admin-execution-nothing = Rien

admin-support-delivery = Transmission { $id }

admin-order-recorded = Enregistrée : { $recorded }

admin-hold-reason = Motif : { $reason }

admin-hold-message = Message { $id }

admin-reason-lead = Motif :{ " " }

admin-raw-receipt-offset = la vue brute d’une réception n’accepte pas de décalage de trame

admin-raw-cursor-outside = le curseur de la vue brute est hors des données conservées

admin-raw-cursor-overflow = dépassement du curseur de la vue brute

admin-raw-corrupt = les données stockées sont corrompues

admin-raw-continued = { $what }, suite

admin-removed-machine-name = Automate { $machines }

admin-removed-machine-names = Automates { $machines }

admin-recorded-as-new = nouveau

admin-recorded-as-correction = une correction

admin-lab-app-answer-unreadable = La réponse de l’application de laboratoire n’a pas pu être lue

admin-lab-app-answer-unreadable-in-line = la réponse de l’application de laboratoire n’a pas pu être lue

admin-page-could-not-be-shown = Cette page n’a pas pu être affichée

admin-activity-from = { $rest } – provenant de { $machine }

admin-activity-a-machine = Un automate

admin-warning = Avertissement

admin-read-results = Lu · { $results }

admin-senders-from = De { $machines }

admin-software-version-not-checked = Version du logiciel non vérifiée par rapport à un manuel

admin-error-alert-snapshot-too-large = l’instantané des alertes actives dépasse 4096 lignes

admin-error-alert-retry-endpoint-missing = le point de terminaison de relance des alertes n’est pas configuré

admin-error-receipt-machine-missing = l’automate de la réception est absent de la configuration actuelle

admin-error-receipt-page-lost-cursor = la page de réception a perdu son curseur

admin-error-unknown-receipt = la réception n’existe pas

admin-alert-unknown-number-waits = un nombre inconnu de résultats ou de messages de demande attendent

admin-alert-count-waits =
    { $count ->
        [one] { $number } résultat ou message de demande attend
        [many] { $number } de résultats ou messages de demande attendent
       *[other] { $number } résultats ou messages de demande attendent
    }

admin-alert-cannot-deliver-not-configured = Transmission impossible : aucun automate ni aucune application de laboratoire n’utilise la destination { $destination } ; { $waiting }. Ajoutez l’application de laboratoire { $destination } sur la page des applications de laboratoire et redémarrez, ou restaurez ses paramètres ; les résultats sont conservés et seront envoyés dès qu’elle sera configurée.

admin-alert-cannot-deliver-keys-missing = Transmission impossible : la clé API ou la clé de signature de la destination d’application de laboratoire { $destination } est manquante ; { $waiting }, en pause. Saisissez la clé API et la clé de signature de l’application de laboratoire { $destination } sur la page des applications de laboratoire ou dans l’environnement du service, puis redémarrez ; les résultats sont conservés et seront envoyés dès que les clés seront définies.
