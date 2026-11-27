### Review status: machine-translated, needs native review (2026-11-27)

cli-lang-help = Gjuha e ndërfaqes (etiketë BCP 47). Si parazgjedhje përdoret LC_ALL, LANG ose anglishtja

cli-interpretation-recorded-decisions-inspect-parked-items-for-failures = Interpretimi { $version } i regjistruar; vendimet: { $outcomes }. Inspektoni sendet e parkuara për dështime.

cli-using = Duke përdorur { $path } ({ $describe }).

cli-scm-dispatch-was-not-selected = Dërgimi SCM nuk u zgjodh ({ $shutdown-timeout-secs })

cli-resolution = { $id } { $evidence } resolution={ $as-deref }

cli-item-has-no-eligible-measurement-comparison = Artikulli { $id } nuk ka asnjë krahasim të përshtatshëm të matjes.

cli-continue-with-after = Vazhdo me --after { $id }

cli-not-set-up-choose-its-definition-on-the-setup = { $id } nuk është konfiguruar: zgjidhni përkufizimin e tij në faqen e konfigurimit

cli-restored-order-sending-paused-pending-reconciliation-previous-store = Restauruar; urdhri i dërgimit është ndërprerë në pritje të pajtimit. Depoja e të dhënave i mëparshëm: { $kept }

cli-actor-is-required-for-recovery-commands = --actor kërkohet për komandat e rikuperimit

cli-dispatched-in-main = nisur nga main

cli-reprocess-must-use-the-recovery-entry-point = ripërpunimi duhet të përdorë pikën hyrëse të rikuperimit

cli-resolve-must-use-the-recovery-entry-point = Zgjidhja duhet të përdorë pikën hyrëse të rikuperimit

cli-no-machines-add-one-on-the-setup-page = pa pajisja: shtoni një në faqen e konfigurimit

cli-delivery-queued-with-its-original-id-and-bytes = Dorëzimi është në radhë me ID origjinal dhe bajt.

cli-backup-verified = Rezervimi u verifikua.

cli-no-log-file-at = Nuk ka skedar log në { $path }.

cli-the-windows-service-writes-it-while-it-runs-on = Shërbimi Windows e shkruan atë ndërsa funksionon. Në Linux shërbimi regjistrohet në ditarin e sistemit:

cli-more-analyte-activity-before = Më shumë: analyte activity --before { $next }

cli-raised-for = { $alert } ({ $signal }) · ngritur { $raised } · për { $responder }

cli-fix-on = { "  " }Rifikojeni në: { $page }

cli-command = { "  " }Komanda: { $command }

cli-oldest-message-kept = Mesazhi më i vjetër i ruajtur: { $oldest }

cli-more-analyte-data-keeping-after = Më shumë: analyte data-keeping --after { $next }

cli-sample-order = Shembull { $sample } · porosi { $order }

cli-more-analyte-orders-cancelled-after = Më shumë: analyte orders cancelled --after { $next }

cli-no-earlier-activity-this-is-past-the-oldest-line = Asnjë aktivitet i mëparshëm: kjo ka kaluar linja më e vjetër që mban Analyte.

cli-no-activity-yet = Asnjë aktivitet ende.

cli-no-active-alerts = Nuk ka sinjalizime aktive.

cli-not-recorded = nuk është regjistruar

cli-how-long-messages-are-kept = Sa kohë mbahen mesazhet

cli-no-machine-is-set-up = { "  No machine is set up." }

cli-not-run-yet = { "  Not run yet" }

cli-no-holds-messages-are-deleted-only-as-kept-above = { "  No holds: messages are deleted only as kept above." }

cli-no-orders-cancelled-by-a-machine = Asnjë porosi e anuluar nga një pajisje.

cli-settings-checked-for-this-version = Cilësimet u kontrolluan për këtë version.

cli-exhausted-alert-events-queued-with-original-identities = Ngjarjet e rraskapitura të alarmit në radhë me identitete origjinale.

cli-retention-hold-updated = Mbajtja e ruajtjes u përditësua.

cli-no-recordings = { $machine }: nuk ka regjistrime

cli-bytes-last-changed = { $machine } { $file } { $bytes } bajt, ndryshimi i fundit { $at }{ $else }

cli-copied-to-it-contains-patient-data-de-identify-a = Kopjuar në { $target }. Ai përmban të dhëna për pacientin: çidentifikoni një kopje përpara se ta ndani.

cli-wrote-values-replaced-distinct-the-original-is-unchanged = Shkruani { $path }: Vlerat { $replaced } u zëvendësuan ({ $distinct } të dallueshme). Origjinali është i pandryshuar.

cli-copy-to = kopjoni në

cli-the-copy-exceeds-the-recording-file-bound = kopja tejkalon skedarin e regjistrimit të lidhur

cli-write-a-reason-it-is-recorded-in-the-history = Shkruani një arsye. Është regjistruar në histori.

cli-keep-the-reason-to-256-characters = Mbajeni arsyen në 256 karaktere.

cli-order-sending-paused-restored-store-requires-machine-app-reconciliation = ORDER SENDING PAUSED: depoja e të dhënave i restauruar kërkon pajtimin e pajisjes/aplikacionit.

cli-help-main-cli-config = Skedari i programit për komandat e operatorit. Pa të: skedari i emërtuar nga ANALYTE_CONFIG; tjetër analyte.toml në dosjen aktuale, nëse ka një të tillë; përndryshe skedari i programit të instaluar (Windows: %ProgramData%\Analyte\config\analyte.toml; Linux: /etc/analyte/analyte.toml), nëse ka një të tillë. Një skedar i gjetur në këtë mënyrë emërtohet në gabim standard

cli-help-main-cli-actor = Identiteti i qartë i operatorit lokal, i regjistruar në gjurmën e auditimit; mund të mos fillojë si identifikimi i një faqe administratori (`admin-session-`, `windows:`)

cli-help-main-command-retry-alerts = Riprovon ngjarjet e alarmeve të shterura me identitetet dhe bajtet e tyre origjinale

cli-help-main-command-captures = Bisedat e regjistruara (ADR 0009, ADR 0034): renditini ato ose kopjoni një në një dosje. Ato përmbajnë të dhëna për pacientët; të dyja janë të audituara

cli-help-main-command-holds = Vendos ose lëshon një mbajtje të pacaktuar

cli-help-main-command-admin = Çelësi i faqes së administratorit

cli-help-main-command-machines = Liston, shton, konfiguron, teston dhe heq pajisjet përmes kontrolleve të vetë faqes Setup; ndryshimet zbatohen pas një rifillimi

cli-help-main-command-lab-apps = Liston, shton, ndryshon dhe heq aplikacionet e laboratorit dhe ruan çelësat e tyre përmes kontrolleve të vetë faqes së aplikacioneve të laboratorit; ndryshimet zbatohen pas një rifillimi

cli-help-main-command-orders = Porositë e pajisjeve: lista, anuluar nga një pajisje, rakordimi, rifillimi pas një rivendosjeje

cli-help-main-command-check-config = Kontrollon një skedar programi dhe liston versionin e cilësimeve të çdo pajisjeje dhe nivelin e provave

cli-help-main-command-run = Drejton motorin në këtë dritare deri në Ctrl+C (shërbimi përdor `service run`)

cli-help-main-command-run-shutdown-timeout-secs = Afati i procesit të vështirë pas fillimit të mbylljes

cli-help-main-command-service = Pika hyrëse për shërbimin Windows; filloni me Start-Service Analyte

cli-help-main-command-status = Linjat teknike shëndetësore për mbështetje: sinjalizime, radhë, të dhënat e fundit për pajisje

cli-help-main-command-parked = Kërkon shqyrtim: çështjet që Analyte i ruajti, por nuk i përpunoi dot. `review` dhe `parked` janë e njëjta komandë

cli-help-main-command-receipts = Ripërpunon një mesazh të ruajtur me cilësimet aktuale

cli-help-main-command-deliveries = Dërgesat për aplikacionet e laboratorit: shfaq listën ose ridërgo

cli-help-main-command-restore = Rivendos depon e të dhënave nga një kopje rezervë e verifikuar; dërgimi i porosisë qëndron në pauzë derisa të pajtohet

cli-help-main-command-backup = Shkruan një kopje rezervë të verifikuar të depos së të dhënave në një skedar

cli-help-main-command-completions = Printon një skript që plotëson komandat e analyte në `shell`. PowerShell: analyte completions powershell | Out-String | Invoke-Expression

cli-help-main-admin-token = Zëvendëson kredencialin e mbrojtur të administratorit dhe e printon atë një herë

cli-help-main-parked-compare = Shfaq krahasimin e rezultateve të mbrojtura dhe fotografinë e konfirmimit të tij

cli-help-main-parked-suggestions = Listoni sugjerimet e mostrës që mungojnë për një faqe artikujsh të hapur

cli-help-main-parked-decide = Konfirmoni krahasimin e shfaqur të matjeve

cli-help-main-parked-list-after = Vazhdo pas ID të fundit të një faqeje të mëparshme të kufizuar

cli-about = Shërbim për marrjen e të dhënave nga analizatorët

cli-help-operator-command-resume-after-restore = Lëshon pauzën e rivendosjes pasi pajisjet dhe aplikacioni të jenë rakorduar

cli-help-operator-command-resume-after-restore-reason = Dëshmi për vendimin e operatorit

cli-help-operator-command-list = Liston një faqe të kufizuar të fotografive të ciklit jetësor të rendit të pajisjes

cli-help-operator-command-list-after = Vazhdo pas kësaj porosie lokale me pajisje ID

cli-help-operator-command-list-machine = Kufizoni faqen në një pajisje të instaluar

cli-help-operator-command-cancelled = Faqja «Anuluar nga pajisja»: analizat e porositura që pajisja i anuloi vetë. Kontrollojini me laboratorin dhe aplikacionin e laboratorit. Për çdo analizë shfaqet ID-ja e mostrës nga kopja e aplikacionit, njësoj si në faqe. Vlerat e rezultateve nuk shfaqen kurrë. Çdo ekzekutim regjistrohet në regjistrin e auditimit

cli-help-operator-command-cancelled-after = Vazhdoni pas këtij pozicioni, siç e printoi faqja e mëparshme

cli-help-operator-command-cancelled-json = Printoni artikujt si JSON për skriptet

cli-help-operator-command-reconcile = Zgjidh një rezultat të pasigurt të pajisjes duke përdorur dëshminë e operatorit

cli-help-operator-command-reconcile-id = Porosia lokale e pajisjes ID e printuar sipas listës së porosive

cli-help-operator-command-reconcile-outcome = Rezultati i verifikuar nga operatori

cli-help-operator-command-reconcile-reason = Shpjegimi i kërkuar ruhet në auditimin dhe ngjarjen e aplikacionit

cli-help-holds-command-receipt = Vendos ose lëshon një mbajtje në një faturë

cli-help-holds-command-receipt-id = Marrja e hostit UUID

cli-help-holds-command-receipt-release = Lëshoni një mbajtje ekzistuese në vend që të vendosni një të tillë

cli-help-holds-command-receipt-reason = Shpjegimi i kërkuar i operatorit

cli-help-holds-command-order = Vendos ose lëshon një pritje që mbulon një porosi aplikacioni dhe faturat e lidhura me të

cli-help-holds-command-order-destination = Hapësira e emrit të destinacionit

cli-help-holds-command-order-id = Identiteti i porosisë së aplikacionit

cli-help-captures-command-list = Liston regjistrimet e secilës pajisje: skedarin, madhësinë dhe ndryshimin e fundit. Ato përmbajnë të dhëna për pacientët

cli-help-captures-command-list-machine = Vetëm regjistrimet e kësaj pajisjeje

cli-help-captures-command-export = Kopjon një regjistrim në një dosje, bajt për bajt, pa mbishkrim. Ai përmban të dhëna për pacientin: çidentifikoni një kopje përpara se ta ndani

cli-help-captures-command-export-machine = Emri i pajisjes

cli-help-captures-command-export-file = Emri i skedarit të regjistrimit, siç e tregon `captures list`

cli-help-captures-command-export-to = Dosja në të cilën do ta kopjoni; duhet të ekzistojë

cli-help-captures-command-export-reason = Pse eksportohet; auditi e mban atë

cli-help-captures-command-de-identify = Shkruan një kopje të çidentifikuar të një regjistrimi në një dosje: fushat e pacientit bëhen vlera sintetike të së njëjtës formë dhe shumat e kontrollit rillogariten. Origjinali nuk është ndryshuar. Rishikoni kopjen përpara se ta ndani

cli-help-captures-command-de-identify-to = Dosja në të cilën do të shkruani kopjen; duhet të ekzistojë

cli-help-captures-command-de-identify-reason = Pse është çidentifikuar; auditi e mban atë

cli-help-captures-command-de-identify-checksum-case = Rasti i shkronjës së kontrollit që përdor pajisja (VIDAS); nevojiten vetëm kur regjistrimi nuk mund ta shfaqë atë, dhe atëherë komanda e thotë këtë

cli-help-command-command-overview = Përmbledhja me fjalë të thjeshta. Shfaq «Gjithçka në rregull» ose çështjet që kërkojnë vëmendje, me faqen dhe komandën për zgjidhjen e secilës. Merr të dhëna nga motori në punë. Kodi i daljes: 0 gjithçka në rregull, 1 kërkon vëmendje, 2 një problem ka ndalur rrjedhën e të dhënave, 3 gjendja nuk dihet

cli-help-command-command-overview-json = Shfaq përgjigjen në formatin JSON për skriptet

cli-help-command-command-activity = Faqja «Aktiviteti»: mesazhet e ruajtura, përpjekjet e dërgimit (dështimet radhazi në një rresht), porositë dhe ndryshimet, nga më të rejat. Rreshtat shfaqin ID-të e mostrave dhe numrat përkatës. Vlerat e rezultateve nuk shfaqen kurrë. Çdo ekzekutim regjistrohet në regjistrin e auditimit

cli-help-command-command-activity-limit = Sa rreshta për të printuar (1 deri në 200)

cli-help-command-command-activity-machine = Vetëm linjat e kësaj pajisjeje

cli-help-command-command-activity-before = Vazhdoni përpara këtij pozicioni, siç e printoi faqja e mëparshme

cli-help-command-command-activity-json = Printoni rreshtat si JSON për skriptet

cli-help-command-command-alerts = Sinjalizimet aktive, secili me arsyen e tij dhe ku është fiksuar

cli-help-command-command-data-keeping = Faqja e ruajtjes së të dhënave: sa kohë ruhen mesazhet e çdo pajisjeje, pastrimi i fundit, mesazhi më i vjetër dhe ruhet aktivi

cli-help-command-command-data-keeping-after = Vazhdoni pas këtij numri mbajeni, siç e printoi faqja e mëparshme

cli-help-command-command-data-keeping-json = Printoni faqen si JSON për skriptet

cli-help-command-command-doctor = Një kontroll shëndetësor vetëm për lexim për IT: skedari i programit, motori, lidhja e çdo pajisjeje, çdo aplikacion i laboratorit, hapësira në disk dhe identifikimi në Windows. Çdo rresht thotë OK, WARN ose FAIL dhe çfarë të bëni. Nuk ndryshon asgjë. Kodi i daljes: 0 të gjitha OK, 1 një paralajmërim, 2 një dështim

cli-help-command-command-doctor-json = Printoni çeqet si JSON për skriptet

cli-help-command-command-confirm-software = Konfirmoni që cilësimet aktuale janë kontrolluar për versionin e raportuar

cli-help-command-command-confirm-software-machine = Pajisja ID nga faqja Pajisjet

cli-help-command-command-confirm-software-version = Versioni i saktë i softuerit tregohet në paralajmërim

cli-help-command-command-logs = Ku është regjistri i motorit dhe rreshtat e tij të fundit. Regjistri nuk mban të dhëna për pacientin: Analyte regjistron emrat, numërimet dhe ID-të, kurrë mesazhet ose vlerat

cli-help-command-command-logs-tail = Sa nga rreshtat e fundit për të printuar (1 deri në 1000)

cli-help-command-alerts-list = Liston çdo alarm aktiv

cli-help-command-alerts-list-json = Printoni sinjalizimet si JSON për skriptet

cli-help-lab-apps-command-list = Liston aplikacionet e laboratorit: adresën, kohën, pajisjet që përdorin secilin dhe ku gjendet secili çelës (asnjëherë vlera e tij)

cli-help-lab-apps-command-add = Shton një aplikacion të laboratorit. Dy çelësat kërkohen pa u shfaqur në ekran ose lexohen nga hyrja standarde, nga një për rresht: së pari çelësi API, pastaj çelësi i nënshkrimit

cli-help-lab-apps-command-add-name = Shkronjat A deri në Z, shifrat, - dhe _ (më së shumti 64); emërton edhe çelësat e tij

cli-help-lab-apps-command-set = Ndryshon adresën ose cilësimet e kohës për një aplikacion të laboratorit. Vlerat që nuk jepen mbeten siç janë

cli-help-lab-apps-command-set-name = Emri i aplikacionit të laboratorit

cli-help-lab-apps-command-keys = Zëvendëson çelësat e një aplikacioni të laboratorit. I kërkon pa i shfaqur ose i lexon nga hyrja standarde, nga një për rresht (çelësi API, pastaj çelësi i nënshkrimit). Një përgjigje bosh mban çelësin ekzistues. Çelësat nuk merren kurrë nga opsionet e komandës

cli-help-lab-apps-command-remove = Heq një aplikacion të laboratorit që asnjë pajisje nuk e përdor dhe që nuk ka punë në pritje

cli-help-lab-apps-settings-address = Adresa e tij HTTPS, siç është `https://lab.example.org`

cli-help-lab-apps-settings-freshness-seconds = Sa kohë llogaritet lista e porosive të saj aktuale, në sekonda të tëra

cli-help-lab-apps-settings-request-timeout-seconds = Sa të presim përgjigjen e saj, në sekonda të tëra

cli-help-lab-apps-settings-review-notices = Dërgimi i njoftimeve për shqyrtim: `yes` ose `no` (ADR 0038). Një aplikacion i ri i laboratorit i merr vetëm nëse këtu zgjidhet po

cli-help-args-cable-baud = Kablloja seriale, shpejtësia në baud: 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 ose 115200

cli-help-args-cable-data-bits = Kabllo serike: bit të dhënash, 5 deri në 8

cli-help-args-cable-parity = Kablloja seriale, pariteti: none (pa paritet), odd (tek) ose even (çift)

cli-help-args-cable-stop-bits = Kabllo serike: bit ndalimi, 1 ose 2

cli-help-args-cable-flow-control = Kablloja seriale, kontrolli i rrjedhës: none (pa kontroll), hardware (RTS/CTS) ose software (XON/XOFF)

cli-help-args-choice-model = Modeli i pajisjes: çelësi i tij siç e liston `machines models`, ose `installed` për të mbajtur atë aktual

cli-help-args-choice-serial-port = Një port serik me emër, si COM3

cli-help-args-choice-usb-serial = Një përshtatës serial USB sipas numrit të tij serial, kështu që gjendet në çdo prizë USB

cli-help-args-choice-address = Adresa IP e një lidhjeje rrjeti (kjo PC është kur pajisja lidhet me Analyte, e pajisjes kur lidhet Analyte me të), me --port

cli-help-args-choice-port = Numri i portës së rrjetit, me --address

cli-help-args-choice-folder = Dosja Analyte lexon rezultatet nga

cli-help-args-choice-lab-app = Aplikacioni i laboratorit ku kjo pajisje i dërgon rezultatet

cli-help-args-choice-allowed-peer = Për një pajisje që lidhet me Analyte: pranoni lidhjet vetëm nga kjo pajisje, adresën IP të vetë analizatorit; përsëriteni për disa. Çdo pajisje tjetër refuzohet (ADR 0026)

cli-help-args-choice-any-device = Për një pajisje që lidhet me Analyte: pranoni përsëri lidhjet nga çdo pajisje, duke hequr pajisjet e saj të lejuara

cli-help-args-choice-choice = Një nga cilësimet e vetë modelit, pasi hapi 2 i rendit ato nën cilësimet e analizatorit dhe `machines models` i emërton; përsëriteni për disa. Një cilësim që nuk jepet ruan vlerën e tij të ruajtur, përndryshe sugjerimi i modelit

cli-help-args-choice-record-only = Vetëm regjistrim (ADR 0034): mbajeni analizatorin në komunikim dhe regjistroni çdo bajt për të hartuar përkufizimin e tij. Aplikacionit të laboratorit nuk i dërgohet asgjë, ndaj mos përdorni --lab-app. Kjo vlen edhe për një model që ende nuk është gati. Regjistrimet përmbajnë të dhëna të pacientëve. Eksportojini me `analyte captures export`

cli-help-args-choice-connect-normally = Lë vetëm regjistrim: lidhni përsëri pajisjen normalisht (ka nevojë për një model të gatshëm dhe --lab-app). Mesazhet e regjistruara nuk dërgohen kurrë

cli-help-args-choice-poll-seconds = Autolyser: sekondat ndërmjet kërkesave për raporte, kur këtë e kërkon modeli ose profili i regjistrimit

cli-help-args-choice-reports-carry-controls = Vetëm regjistrim, Autolyser: a përmbajnë raportet kontrolle? Përdorni yes ose no

cli-help-args-choice-vidas-acknowledgment = Vetëm regjistrim, VIDAS: mënyra e konfirmimit në VIDAS PC, literal ose alternate

cli-help-args-choice-analyzer-mode = Regjistro vetëm, për një model që e kërkon (cobas e 411: elecsys ose cobas): modaliteti që tregojnë cilësimet e vetë analizatorit; `machines models` e rendit atë

cli-help-args-choice-repeated-results = Rezultatet e përsëritura në një lidhje normale: `automatic` (rekomandohet, një rezultat i ridërguar ruhet vetëm një herë dhe një korrigjim krijon version të ri) ose `always_ask` (çdo përsëritje mbetet në pritje te «Kërkon shqyrtim»). Nëse nuk e caktoni, ruhet zgjedhja e mëparshme. Në mungesë të saj, përdoret automatic (ADR 0035)

cli-help-machines-command-list = Liston pajisjet në cilësimet: modelin, lidhjen, aplikacionin e laboratorit, nëse është i konfiguruar, versioni i cilësimeve dhe, nga motori në punë, gjendja e drejtpërdrejtë

cli-help-machines-command-list-json = Printon JSON për skriptet në vend të rreshtave

cli-help-machines-command-models = Liston modelet e pajisjeve në dosjen e pajisjeve: gati ose jo, dhe si u kontrollua secila

cli-help-machines-command-ports = Liston portet serike të këtij PC me përshtatësin e tyre dhe numrin serial USB

cli-help-machines-command-add = Shton një pajisje në cilësimet; nuk është ngritur ende. Cilësimet e kabllove serike të dhëna këtu ruhen për konfigurimin e tij

cli-help-machines-command-add-name = Shkronjat A deri në Z, shifrat, - dhe _; ai gjithashtu emërton skedarin e tij të cilësimeve

cli-help-machines-command-add-same-analyzer = Emri ka ruajtur historinë dhe ky është i njëjti analizator

cli-help-machines-command-set = Konfiguron modelin e pajisjes, lidhjen dhe aplikacionin e laboratorit. I kontrollon si në hapin «Ruaj». Zgjedhjet që nuk jepen mbeten siç janë ruajtur

cli-help-machines-command-set-dry-run = Printon atë që do të shkruhej dhe nuk shkruan asgjë

cli-help-machines-command-test = Kryen testin e lidhjes vetëm për dëgjim të hapit 3 me cilësimet e ruajtura ose zgjedhjet e dhëna: asgjë nuk dërgohet dhe asgjë nuk ruhet

cli-help-machines-command-test-seconds = Sa kohë për të dëgjuar, 1 deri në 60 sekonda

cli-help-machines-command-test-show-bytes = Gjithashtu printon bajtet e marra (maksimumi 64 KiB). Ato mund të përmbajnë të dhëna për pacientin: kurrë mos i ruani ose mos i shpërndani ato

cli-help-machines-command-remove = Heq një pajisje nga cilësimet; rezultatet e tij të ruajtura dhe historia qëndrojnë

cli-help-machines-command-remove-anyway = E heq edhe pse puna është ende e papërfunduar

cli-error-reserved = --actor mund të mos fillojë me "{ $value }": Analyte regjistron vetëm për njerëzit e identifikuar në faqet e administratorit

cli-error-text = --actor: { $value }

cli-named-by-config = emëruar nga --config

cli-named-by-analyte-config = emëruar nga ANALYTE_CONFIG

cli-in-the-current-folder = në dosjen aktuale

cli-the-installed-program-file = skedarin e programit të instaluar

cli-log-path = Regjistri: { $path }

cli-choice-write-name-value = shkruani NAME=VALUE, për shembull protocol_mode=alternate

cli-token-missing = skedari i tokenit të administratorit mungon; ekzekutoni analyte admin token --rotate

cli-token-invalid = skedari i tokenit të administratorit duhet të përmbajë një token 256-bit të koduar në base64

cli-token-permissions = skedari i tokenit të administratorit duhet të jetë skedar i rregullt i mbrojtur (Unix mode 0600)

cli-token-access = skedari i tokenit të administratorit: { $error }

cli-token-random = burimi i rastësisë së sistemit dështoi për tokenin e administratorit: { $error }

cli-token-io = tokeni i administratorit: { $operation } dështoi: { $error }

cli-token-op-inspect = kontrollimi i kredencialit

cli-token-op-read = leximi i kredencialit

cli-token-op-create-staging = krijimi i skedarit të përkohshëm të mbrojtur

cli-token-op-write-staging = shkrimi i skedarit të përkohshëm

cli-token-op-flush-staging = ruajtja në disk e skedarit të përkohshëm

cli-token-op-install = instalimi i kredencialit

cli-token-op-flush-directory = ruajtja në disk e dosjes së kredencialit

cli-setup-settings-do-not-load = Cilësimet e Analyte nuk ngarkohen: { $error }

cli-setup-store-not-opened = Të dhënat e ruajtura të Analyte nuk mund të hapeshin: { $error }

cli-setup-ports-failed = listimi i porteve serike dështoi: { $error }

cli-setup-console-failed = konsola nuk mund të përdorej: { $error }

cli-setup-no-actor = tregoni kush e bën këtë ndryshim me --actor <name>; regjistrohet në historikun e auditimit

cli-setup-not-confirmed = asgjë nuk u ruajt: nuk u konfirmua

cli-setup-engine-stopped = çelësat mund t'i ruajë vetëm motori i Analyte, i cili nuk po punon ({ $error })

cli-setup-keys-not-sent = çelësat nuk u dërguan: { $error }

cli-setup-engine-no-answer = nuk dihet nëse motori e ruajti ndryshimin: { $error }

cli-advice-run-as-service-account = Ekzekutojeni këtë komandë si llogaria me të cilën punon shërbimi Analyte, ose vendosni çelësat në faqen Aplikacionet e laboratorit.

cli-advice-remove-anyway = Për ta hequr gjithsesi, ekzekutojeni përsëri komandën me --anyway.

cli-advice-same-analyzer = Nëse është i njëjti analizator, ekzekutojeni përsëri komandën me --same-analyzer.

cli-advice-check-config = Shihni çfarë nuk shkon me: analyte check-config <file>.

cli-advice-start-engine = Nisni motorin me Start-Service Analyte në një PowerShell administratori, pastaj ekzekutojeni përsëri këtë komandë.

cli-advice-check-lab-apps-list = Kontrolloni me analyte lab-apps list para se ta ekzekutoni përsëri komandën.

cli-setup-engine-runs-unknown = Nuk dihet çfarë po ekzekuton motori ({ $why }), kështu që çdo pajisje në cilësime llogaritet si në punë.

cli-setup-ask-actor = Emri juaj për regjistrin e auditimit

cli-setup-restart-note = { $restart } Në Windows ekzekutoni Restart-Service Analyte në një PowerShell administratori, ose shtypni Rinis tani në faqen Konfigurimi.

cli-fix-this = Rregulloni këtë: { $reason }

cli-nothing-was-changed = Asgjë nuk u ndryshua.

cli-reason-character-cannot-be-recorded = Arsyeja përmban një karakter që nuk mund të regjistrohet.

cli-captures-patient-data = Këto regjistrime përmbajnë të dhëna pacientësh. Mbajini në këtë kompjuter; para se të ndani një të tillë, bëni një kopje të anonimizuar me captures de-identify dhe rishikojeni (docs/develop/adding-a-machine.md, "Capture, de-identify and replay").

cli-captures-machine-name = emri i një pajisjeje përdor vetëm shkronja, shifra, - dhe _: { $name }

cli-captures-file-name = emërtoni një regjistrim siç e tregon captures list (një emër skedari që mbaron me .rec.jsonl), jo një shteg: { $name }

cli-captures-missing = pajisja { $machine } nuk ka regjistrim me emrin { $file }

cli-captures-no-folder = dosja { $path } nuk ekziston; krijojeni më parë

cli-captures-exists = { $path } ekziston tashmë; asgjë nuk u mbishkrua

cli-captures-too-many = pajisja { $machine } ka më shumë se 4096 regjistrime; zhvendosni më parë ato më të vjetrat

cli-captures-format = regjistrimi nuk mund të lexohet: { $error }

cli-captures-io = { $operation } { $path } dështoi: { $error }

cli-captures-op-read = leximi i

cli-captures-op-open = hapja e

cli-captures-op-flush = ruajtja në disk e

cli-captures-op-create = krijimi i

cli-captures-op-encode = kodimi i

cli-captures-op-write = shkrimi i

cli-captures-stopped-early = { " " }(regjistrimi ndaloi herët: i paplotë)

cli-captures-modified-unknown = e panjohur

cli-deidentify-checklist = Rishikojeni kopjen para se ta ndani; fushat me tekst të lirë u zëvendësuan.

cli-deidentify-unsupported = anonimizimi nuk mbështetet për regjistrimet { $protocol }; asgjë nuk u shkrua

cli-deidentify-unknown-stack = regjistrimi emërton një grup protokollesh që Analyte nuk e njeh; asgjë nuk u shkrua

cli-deidentify-unparsed = ngjarja { $event }: { $reason }; Analyte nuk mund të dallojë ku është çdo fushë pacienti, kështu që asgjë nuk u shkrua

cli-deidentify-checksum = ngjarja { $event }: { $reason }; asgjë nuk u shkrua

cli-deidentify-deadline = anonimizimi zgjati më shumë se { $seconds } sekonda; asgjë nuk u shkrua

cli-deidentify-random = burimi i rastësisë së sistemit dështoi: { $error }

cli-deidentify-limit = u tejkalua një kufi i anonimizimit; asgjë nuk u shkrua

cli-deidentify-checksum-not-hex = shuma kontrolluese origjinale nuk është dy shifra heksadecimale

cli-deidentify-checksum-case-unknown = madhësia e shkronjave të kësaj shume kontrolluese nuk dallohet nga regjistrimi; ekzekutojeni përsëri me --checksum-case upper ose lower, siç thonë cilësimet e pajisjes

cli-deidentify-checksum-not-sum = shuma kontrolluese origjinale nuk është një shumë treshifrore modulo 256

cli-deidentify-astm-outside-frames = bajte jashtë kuadrove ASTM

cli-deidentify-astm-repeat-differs = një kuadër ASTM i përsëritur ndryshon nga kuadri që përsërit

cli-deidentify-astm-header-delimiters = një kokë ASTM pa katër ndarës të ndryshëm

cli-deidentify-astm-record-before-header = një regjistrim ASTM para kokës së tij

cli-deidentify-astm-record-no-delimiter = një lloj regjistrimi ASTM pa ndarës fushe

cli-deidentify-astm-manufacturer-record = një regjistrim prodhuesi (M), fushat e të cilit i përcakton vetëm prodhuesi

cli-deidentify-astm-unknown-record = një lloj i panjohur regjistrimi ASTM

cli-deidentify-autolyser-host-outside = bajte të hostit jashtë një shkëmbimi Autolyser

cli-deidentify-autolyser-unasked = bajte të pajisjes që Analyte nuk i kërkoi

cli-deidentify-autolyser-short-record = një regjistrim Autolyser më i shkurtër se kodi i pacientit

cli-deidentify-field-escape = një fushë pacienti mban një sekuencë escape që mund të shkruajë tekst pacienti

cli-deidentify-hl7-outside-blocks = bajte jashtë blloqeve MLLP

cli-deidentify-hl7-msh-encoding = një MSH pa karaktere kodimi të ndryshme

cli-deidentify-hl7-segment-before-msh = një segment HL7 para MSH-së së tij

cli-deidentify-hl7-segment-no-separator = një emër segmenti HL7 pa ndarës fushe

cli-deidentify-hl7-segment-no-rules = një segment HL7 për të cilin Analyte nuk ka rregulla për fushat e pacientit

cli-deidentify-vidas-foreign-byte = një bajt që paketat VIDAS nuk e mbajnë kurrë

cli-deidentify-vidas-outside-packets = bajte jashtë paketave VIDAS

cli-program-file-not-found = nuk u gjet skedar programi; u kërkua në { $looked }. Jepni --config <file> ose vendosni { $variable }

cli-program-file-unreadable = nuk mund të kontrollohet { $path }: { $error }. Skedari i instaluar i programit lexohet vetëm nga administratorët: ekzekutojeni komandën si administrator (një PowerShell me të drejta të ngritura në Windows, sudo në Linux), ose jepni --config <file>
