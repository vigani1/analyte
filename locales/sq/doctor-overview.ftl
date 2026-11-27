### Review status: machine-translated, needs native review (2026-11-27)

doctor-overview-free-disk-space = Hapësirë e lirë në disk

doctor-overview-always-kept-free = I mbajtur gjithmonë i lirë

doctor-overview-analyte-stops-receiving-below-this = Analyte nuk merr më poshtë kësaj

doctor-overview-disk-status = Statusi i diskut

doctor-overview-active-alerts = Sinjalizime aktive

doctor-overview-alert-relays = Reletë e alarmit

doctor-overview-relay-status-for-support = Statusi i stafetës për mbështetje

doctor-overview-main-center = main--center

doctor-overview-let-s-connect-your-first-machine = Le të lidhim pajisjen tuaj të parë.

doctor-overview-analyte-reads-results-from-your-lab-s-analyzers-keeps = Analyte lexon rezultatet nga analizatorit e laboratorit tuaj, mban një kopje dhe i dërgon ato në aplikacionin tuaj laboratorik. Porositë shkojnë në anën tjetër.

doctor-overview-set-up-your-first-machine = Vendosni pajisjen tuaj të parë

doctor-overview-the-engine-is-running = Motori është në punë.

doctor-overview-what-to-do = Çfarë duhet të bëni: { $error }

doctor-overview-fix-on = { "   " }Rrigjo në: { $page } ({ $href })

doctor-overview-command = { "   " }Komanda: { $command }

doctor-overview-analyte-cannot-say-how-things-are-right-now = Analyte nuk mund të thotë se si janë gjërat tani

doctor-overview-problem = PROBLEM

doctor-overview-attention = ATTENTION

doctor-overview-add-an-admin-table-with-a-port-to-the = shtoni një tabelë [admin] me një port në skedarin e programit dhe rinisni motorin (shih docs/operations/operate.md)

doctor-overview-run-this-from-an-administrator-powershell-if-the-key = ekzekuto këtë nga një administrator PowerShell; nëse skedari i çelësit mungon, ekzekutoni analyte admin token --rotate dhe rindizni motorin

doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start = ndizni motorin (Start-Service Analyte, ose systemctl start analyte në Linux), prisni disa sekonda dhe provoni përsëri; nëse nuk fillon, ekzekutoni analyte doctor dhe analyte logs

doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection = gjeni programin që mban portën e administratorit (Get-NetTCPConnection -LocalPort <port>) dhe ndaloni atë ose rindizni motorin me Restart-Service Analyte nëse çelësi i administratorit është rrotulluar që nga fillimi

doctor-overview-check-that-this-command-and-the-engine-are-the = kontrolloni që kjo komandë dhe motori janë të njëjtin version Analyte, më pas ekzekutoni analyte logs

doctor-overview-the-engine-could-not-read-its-state = Motori nuk mund ta lexonte gjendjen e tij.

doctor-overview-sending-to-lab-app-is-failing-waiting = Dërgimi te aplikacioni i laboratorit { $destination } po dështon: { $waiting } në pritje

doctor-overview-sending-to-lab-app-is-paused-waiting = Dërgimi te aplikacioni i laboratorit { $destination } është ndërprerë: { $waiting } në pritje

doctor-overview-analyte-stopped-working-with = Analyte ndaloi së punuari me { $machine }

doctor-overview-orders-cancelled-by-a-machine = Porositë e anuluara nga një pajisje: { $cancelled }

doctor-overview-for = { $UNCHECKED-WARNING } për { $machine }

doctor-overview-disk-space-is-low-free = Ka pak hapësirë në disk: { $free } të lira

doctor-overview-check-the-software-version-reported-by = Kontrolloni versionin e softuerit të raportuar nga { $machine }.

doctor-overview-has-gone-quiet = { $machine } ka rënë në heshtje

doctor-overview-has-sent-nothing-for = { $machine } nuk ka dërguar asgjë për { $seconds }

doctor-overview-a-result-has-waited-to-be-sent = Një rezultat ka pritur që të dërgohet { $seconds }

doctor-overview-waiting = , { $count } në pritje

doctor-overview-cannot-send-to-lab-app = Nuk mund të dërgohet në aplikacionin e laboratorit { $destination }: { $why }{ $waiting }

doctor-overview-orders-with-unknown-outcome = Porositë me rezultat të panjohur: { $count }

doctor-overview-sending = Dërgimi

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

doctor-overview-disk-almost-full-analyte-refuses-new-data-from-machines = Disku pothuajse plot: Analyte refuzon të dhënat e reja nga pajisjet

doctor-overview-analyte-could-not-save-a-machine-software-version-warning = Analyte nuk mund të ruante një paralajmërim për versionin e softuerit të pajisjes

doctor-overview-keys-missing = mungojnë çelësat

doctor-overview-not-set-up-any-more = nuk është ngritur më

doctor-overview-items-may-need-review = Artikujt mund të kenë nevojë për rishikim

doctor-overview-bytes-free = { $free } bajt falas

doctor-overview-bytes = { $reserve } bajt

doctor-overview-alert = Alarmi

doctor-overview-responder = Përgjigje

doctor-overview-raised = I ngritur

doctor-overview-last-value = Vlera e fundit

doctor-overview-pick-the-machine-s-model = Zgjidhni modelin e pajisjes

doctor-overview-choose-how-it-s-connected-and-which-lab-app = Zgjidh se si është i lidhur dhe cili aplikacion i laboratorit merr rezultatet e tij

doctor-overview-run-a-listen-only-test-analyte-only-reads-and = Kryeni një test vetëm për të dëgjuar. Analyte vetëm lexon dhe nuk dërgon asgjë në pajisje

doctor-overview-save-then-restart-analyte-to-use-the-new-settings = Ruani, më pas rinisni Analyte për të përdorur cilësimet e reja

doctor-overview-nothing-needs-you-right-now = Asgjë nuk ka nevojë për ju tani.

doctor-overview-analyte-keeps-everything-it-has-received-while-you-fix = Analyte ruan gjithçka që ka marrë ndërkohë që ju i rregulloni këto.

doctor-overview-legend-line-legend-line-wait = legend__line legend__line--wait

doctor-overview-legend-line-legend-line-cut = legend__line legend__line--cut

doctor-overview-link-open = Lidhja e hapur

doctor-overview-waiting-2 = Në pritje

doctor-overview-broken = E thyer

doctor-overview-enough-space = Hapësirë e mjaftueshme

doctor-overview-almost-full-receiving-refused = Pothuajse e plotë: marrja e refuzuar

doctor-overview-order-list = Lista e porosive · { $name }

doctor-overview-cannot-deliver = Nuk mund të dorëzohet

doctor-overview-sending-paused = Dërgimi u ndërpre

doctor-overview-not-delivering = Nuk dorëzohet

doctor-overview-up-to-date = Të përditësuar

doctor-overview-api-key = Tasti API

doctor-overview-signing-key = çelësi i nënshkrimit

doctor-overview-its-keys-cannot-be-checked = Çelësat e tij nuk mund të kontrollohen.

doctor-overview-out-of-date = E vjetëruar

doctor-overview-not-checked-yet = Nuk është kontrolluar ende

doctor-overview-items-waiting-to-send = Artikujt që presin të dërgohen

doctor-overview-results-and-order-messages = rezultatet dhe mesazhet e porosive

doctor-overview-oldest-waiting = Pritja më e vjetër

doctor-overview-the-check-did-not-finish-within-seconds = kontrolli nuk përfundoi brenda { $as-secs } sekondave

doctor-overview-is-valid = { $config } është i vlefshëm

doctor-overview-cannot-be-used = { $config } nuk mund të përdoret: { $error }

doctor-overview-what-to-do-2 = { "      " }Çfarë duhet të bëni: { $fix }

doctor-overview-doctor-and = Mjeku: { $failures } dhe { $warnings }.

doctor-overview-the-store-did-not-open-within-seconds = depoja e të dhënave nuk u hap brenda { $as-secs } sekondave

doctor-overview-running-version-and-answering = ekzekutimi i versionit { $version } dhe përgjigjja

doctor-overview-running-version-but-this-command-is-version = ekzekuton versionin { $version }, por kjo komandë është versioni { $CARGO-PKG-VERSION }

doctor-overview-there-is-no-store-at-yet = nuk ka ende asnjë depo të dhënash në { $file }

doctor-overview-opened-but-did-not-close-cleanly = u hap por nuk u mbyll pastër: { $error }

doctor-overview-cannot-be-opened = { $data-directory } nuk mund të hapet: { $error }

doctor-overview-free-is-always-kept-free = { $free } falas; { $reserve } mbahet gjithmonë falas

doctor-overview-free-at-or-below-the-kept-free-analyte-refuses = Hapësira e lirë ({ $free }) është në ose nën kufirin e rezervuar ({ $reserve }). Analyte refuzon të dhënat e reja nga pajisjet

doctor-overview-the-free-space-could-not-be-read = hapësira e lirë nuk mund të lexohej: { $error }

doctor-overview-a-usb-serial-number-names-a-network-or-folder = një numër serial USB ({ $number }) emërton një lidhje rrjeti ose dosjeje

doctor-overview-cable = Kabllo { $what }

doctor-overview-analyzer-settings = Cilësimet e analizatorit { $what }

doctor-overview-report-poll-interval = { $what } raportoni intervalin e sondazhit

doctor-overview-mode = Modaliteti { $what }

doctor-overview-every-byte-is-kept-and-recorded-under-captures = { $recording-only }; çdo bajt mbahet dhe regjistrohet nën kapjen/{ $id }

doctor-overview-devices = Pajisjet { $what }

doctor-overview-accepts-connections-only-from = pranon lidhje vetëm nga { $allowed }

doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s = ekzekutoni analyte machines set { $id } --allowed-peer <the analyzer's IP address>, ose shënoni Pranojeni këtë pajisje vetëm në hapin 4 të konfigurimit të saj

doctor-overview-the-check-for-folder-failed = kontrolli për dosjen { $path } dështoi: { $error }

doctor-overview-folder-is-there = dosja { $path } është atje

doctor-overview-is-not-a-folder = { $path } nuk është një dosje

doctor-overview-folder-cannot-be-read = dosja { $path } nuk mund të lexohet: { $error }

doctor-overview-the-serial-ports-could-not-be-listed = portat serike nuk mund të renditeshin: { $error }

doctor-overview-serial-port-is-present = porta serike { $name } është e pranishme

doctor-overview-serial-port-is-not-present = porta serike { $name } nuk është e pranishme

doctor-overview-the-adapter-with-serial-number-is-at = përshtatësi me numër serial { $number } është në { $port }

doctor-overview-no-adapter-with-serial-number-is-plugged-in = asnjë përshtatës me numër serial { $number } nuk është i lidhur

doctor-overview-adapters-report-serial-number = Përshtatësit { $several } raportojnë numrin serial { $number }: { $join }

doctor-overview-nothing-listens-on-although-the-engine-runs = asgjë nuk dëgjon në { $address } edhe pse motori funksionon

doctor-overview-is-free-for-analyte-to-listen-on = { $address } është falas për Analyte për të dëgjuar

doctor-overview-is-held-by-the-running-engine = { $address } mbahet nga motori që funksionon

doctor-overview-another-program-holds = një program tjetër mban { $address }

doctor-overview-is-held-by-the-engine-or-by-another-program = { $address } mbahet, nga motori ose nga një program tjetër

doctor-overview-cannot-be-used-on-this-computer = { $address } nuk mund të përdoret në këtë kompjuter: { $error }

doctor-overview-the-engine-connects-to-analyte-overview-says-if-the = motori lidhet me { $address }; analyte overview thotë nëse lidhja është e fikur

doctor-overview-not-tried-the-engine-may-be-connected-to-and = nuk është provuar: motori mund të lidhet me { $address } dhe një pajisje mund të marrë një lidhje në të njëjtën kohë

doctor-overview-accepts-a-connection = { $address } pranon një lidhje

doctor-overview-does-not-accept-a-connection = { $address } nuk pranon një lidhje: { $error }

doctor-overview-did-not-answer-within-seconds = { $address } nuk u përgjigj brenda { $as-secs } sekondave

doctor-overview-lab-app = Aplikacioni i laboratorit { $name }

doctor-overview-when-its-order-list-last-came-could-not-be = kur lista e porosive të fundit erdhi nuk mund të lexohej: { $error }

doctor-overview-credential-is-missing = kredenciali { $name } mungon

doctor-overview-its-keys-could-not-be-read-from-this-account = çelësat e tij nuk mund të lexoheshin nga kjo llogari: { $error }

doctor-overview-resolves-to-no-address = { $host } zgjidh në asnjë adresë

doctor-overview-does-not-resolve = { $host } nuk zgjidh: { $error }

doctor-overview-did-not-resolve-within-seconds = { $host } nuk u zgjidh brenda { $as-secs } sekondave

doctor-overview-answers-over-https-with-a-valid-certificate-http = { $url } përgjigjet mbi HTTPS me një certifikatë të vlefshme (HTTP { $status })

doctor-overview-gives-no-https-answer = { $url } nuk jep përgjigje HTTPS: { $error }

doctor-overview-last-complete-order-list = Lista e fundit e plotë e porosive { $when }

doctor-overview-its-order-list-is-not-up-to-date-last = lista e porosive të saj nuk është e përditësuar (lista e fundit e plotë: { $when })

doctor-overview-create-it-with-new-localgroup-name-then-add-people = krijoni atë me New-LocalGroup -Emri "{ $GROUP }", më pas shtoni njerëz me Add-LocalGroupMember -Group "{ $GROUP }" -Anëtar <user>

doctor-overview-the-group-exists = ekziston grupi { $GROUP }

doctor-overview-the-group-is-missing-so-only-administrators-can-sign = grupi { $GROUP } mungon, kështu që vetëm administratorët mund të identifikohen me Windows

doctor-overview-is-an-account-but-not-a-local-group = { $GROUP } është një llogari, por jo një grup lokal

doctor-overview-the-group-could-not-be-looked-up = grupi { $GROUP } nuk mund të kërkohej: { $error }

doctor-overview-the-rule-is-there = rregulli "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) është atje

doctor-overview-no-rule-lets-analyzers-connect-to = asnjë rregull "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) lejon që analizatorit të lidhen me { $join }

doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended = rregullat e murit të zjarrit nuk mund të lexoheshin (PowerShell përfundoi me { $status })

doctor-overview-the-firewall-rules-could-not-be-read = rregullat e murit të zjarrit nuk mund të lexoheshin: { $error }

doctor-overview-ok = OK

doctor-overview-warn = WARN

doctor-overview-fail = FAIL

doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run = ekzekutoni përsëri analyte doctor; nëse qëndron i ngadaltë, ekzekutoni analyte logs

doctor-overview-fix-what-it-names-or-choose-the-right-file = rregulloni atë që emërton (ose zgjidhni skedarin e duhur me --config), më pas ekzekutoni përsëri analyte doctor; kontrollet e tjera kanë nevojë për një skedar të vlefshëm programi

doctor-overview-doctor-everything-checked-is-ok = Mjeku: gjithçka që kontrollohet është OK.

doctor-overview-store = Depoja e të dhënave

doctor-overview-listed-but-not-set-up-yet-so-it-does = listuar por nuk është konfiguruar ende, kështu që nuk funksionon

doctor-overview-choose-its-model-on-the-setup-page-save-then = zgjidhni modelin e tij në faqen e konfigurimit, ruajeni dhe më pas rindizni motorin

doctor-overview-run-the-analyte-exe-the-service-runs-or-update = ekzekutoni analyte.exe shërbimin që funksionon, ose përditësoni të dyja në të njëjtin version

doctor-overview-run-analyte-overview-for-where-and-how-to-fix = ekzekutoni analyte overview se ku dhe si ta rregulloni secilën

doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run = ekzekutoni përsëri analyte doctor; nëse mbetet i panjohur, ekzekutoni analyte logs

doctor-overview-start-the-engine-once-start-service-analyte-it-creates = ndizni motorin një herë (Start-Service Analyte); ajo krijon depon e të dhënave

doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte = ekzekutoni përsëri analyte doctor; nëse përsëritet, ekzekutoni analyte logs

doctor-overview-run-this-from-an-administrator-powershell-if-it-names = ekzekuto këtë nga një administrator PowerShell; nëse emërton një listë aksesi, shihni docs/operations/install.md

doctor-overview-check-the-machine-s-settings = kontrolloni cilësimet e pajisjes

doctor-overview-machine-software = Softueri i pajisjes

doctor-overview-check-the-store-and-run-analyte-doctor-again = kontrolloni depon e të dhënave dhe ekzekutoni përsëri analyte doctor

doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing = lironi hapësirë në këtë disk. Analyte nuk fshin asgjë për të liruar hapësirë

doctor-overview-check-the-data-directory-s-drive-in-file-explorer = kontrolloni diskun e drejtorisë së të dhënave në File Explorer

doctor-overview-choose-the-machine-s-address-or-folder-on-the = zgjidhni adresën ose dosjen e pajisjes në faqen e konfigurimit

doctor-overview-accepts-connections-from-any-device-on-the-network-which = pranon lidhje nga çdo pajisje në rrjet, e cila mund të dërgojë rezultate si kjo pajisje

doctor-overview-choose-the-machine-s-export-folder-on-the-setup = zgjidhni dosjen e eksportit të pajisjes në faqen e konfigurimit

doctor-overview-create-the-folder-or-share-it-again-or-choose = krijoni dosjen ose ndajeni atë përsëri, ose zgjidhni një tjetër në faqen e konfigurimit

doctor-overview-check-the-usb-serial-adapter-s-driver-in-device = kontrolloni drejtuesin e përshtatësit serial USB në Device Manager

doctor-overview-run-analyte-doctor-again = ekzekutoni përsëri analyte doctor

doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the = futni përshtatësin serial USB ose zgjidhni portin e duhur në faqen e konfigurimit

doctor-overview-unplug-the-adapter-that-does-not-belong-to-this = shkëputni përshtatësin që nuk i përket kësaj pajisjeje

doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte = ekzekutoni analyte overview: shërbimi i pajisjes në Analyte mund të ketë ndaluar; rinisni motorin me Restart-Service Analyte

doctor-overview-stop-the-other-program-or-choose-another-port-on = ndaloni programin tjetër ose zgjidhni një port tjetër në faqen e konfigurimit dhe në pajisje

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell = aktivizo faqen e administratorit në mënyrë që doktori të mund ta tregojë, ose shiko me Get-NetTCPConnection -LocalPort <port>

doctor-overview-choose-an-address-this-computer-has-on-the-setup = zgjidhni një adresë që ka ky kompjuter në faqen e konfigurimit

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2 = aktivizoni faqen e administratorit në mënyrë që mjeku të tregojë nëse motori funksionon

doctor-overview-check-that-the-machine-is-on-its-network-settings = kontrolloni nëse pajisja është e ndezur, cilësimet e saj të rrjetit dhe kabllon

doctor-overview-the-running-engine-has-its-keys = motori i ndezur ka çelësat e tij

doctor-overview-its-api-key-and-signing-key-are-there = çelësi i tij API dhe çelësi i nënshkrimit janë aty

doctor-overview-its-keys-were-not-checked = çelësat e tij nuk janë kontrolluar

doctor-overview-no-order-list-is-kept-for-it = nuk mbahet lista e porosive për të

doctor-overview-restart-the-engine = rindizni motorin

doctor-overview-start-the-engine-and-run-analyte-doctor-again-the = ndizni motorin dhe aktivizoni përsëri analyte doctor; motori kontrollon vetë çelësat

doctor-overview-its-address-has-no-host-name = adresa e tij nuk ka emër pritës

doctor-overview-fix-its-address-on-the-lab-apps-page = rregulloni adresën e tij në faqen e aplikacioneve të laboratorit

doctor-overview-check-the-lab-app-s-address-on-the-lab = kontrolloni adresën e aplikacionit të laboratorit në faqen e aplikacioneve të laboratorit dhe DNS të këtij kompjuteri

doctor-overview-check-this-computer-s-dns-and-network = kontrolloni DNS dhe rrjetin e këtij kompjuteri

doctor-overview-check-the-address-the-lab-app-s-server-and = kontrolloni adresën, serverin e aplikacionit të laboratorit dhe certifikatën e tij; një certifikatë që ky kompjuter nuk i beson duhet të fiksohet në server

doctor-overview-check-that-the-engine-runs-and-the-lab-app = kontrolloni që motori të funksionojë dhe aplikacioni i laboratorit të përgjigjet; pajisjet që kërkojnë porosi nuk marrin përgjigje ndërkohë

doctor-overview-computername-is-not-set-so-the-group-cannot-be = COMPUTERNAME nuk është vendosur, kështu që grupi nuk mund të kërkohet

doctor-overview-run-doctor-from-a-normal-powershell-window = drejtuar doktor nga një dritare normale PowerShell

doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to = ekzekutoni përsëri instaluesin ose lejoni hyrjen TCP në këto porte nga rrjeti laboratorik në Windows Defender Firewall

doctor-overview-check-the-rule-in-windows-defender-firewall = kontrolloni rregullin në Windows Defender Firewall

doctor-overview-reading-the-firewall-rules-took-too-long = leximi i rregullave të murit të zjarrit zgjati shumë

doctor-overview-with-an-unknown-outcome-check-the-machine = { $uncertain } me një rezultat të panjohur (kontrolloni pajisjen)

doctor-overview-cancelled-by-a-machine = { $cancelled } anulohet nga një pajisje

doctor-overview-analyte-could-not-read-its-store = Analyte nuk mund ta lexonte depon e të dhënave e tij.

doctor-overview-analyte-could-not-read-whether-order-sending-is-paused = Analyte nuk mund të lexonte nëse dërgimi i porosisë është ndërprerë pas një rivendosjeje.

doctor-overview-analyte-could-not-read-the-free-disk-space = Analyte nuk mund të lexonte hapësirën e lirë të diskut.

doctor-overview-analyte-could-not-read-how-many-items-need-review = Analyte nuk mund të lexonte se sa artikuj kërkojnë shqyrtim.

doctor-overview-analyte-could-not-read-when-each-lab-app-last = Analyte nuk mund të lexonte se kur çdo aplikacion i laboratorit pranoi për herë të fundit një dërgesë.

doctor-overview-analyte-could-not-read-which-lab-apps-keep-failing = Analyte nuk mund të lexonte se cilat aplikacione të laboratorit vazhdojnë të dështojnë të pranojnë dërgesat.

doctor-overview-analyte-could-not-read-the-active-alerts = Analyte nuk mund të lexonte sinjalizimet aktive.

doctor-overview-analyte-could-not-read-the-machine-orders = Analyte nuk mund të lexonte urdhrat e pajisjes.

doctor-overview-analyte-could-not-read-when-each-machine-last-sent = Analyte nuk mund të lexonte se kur çdo pajisje dërgoi të dhëna për herë të fundit.

doctor-overview-analyte-could-not-read-what-waits-for-each-lab = Analyte nuk mund të lexonte se çfarë pret për çdo aplikacion i laboratorit.

doctor-overview-analyte-could-not-read-what-waits-for-lab-apps = Analyte nuk mund të lexonte se çfarë pret për aplikacionet e laboratorit që nuk përdor asnjë pajisje e konfiguruar.

doctor-overview-analyte-could-not-read-the-alert-relays = Analyte nuk mund të lexonte reletë e alarmit.

doctor-overview-analyte-read-everything-but-could-not-write-the-page = Analyte lexoi gjithçka, por nuk mund ta shkruante faqen.

doctor-overview-no-orders-with-an-unknown-outcome = Asnjë urdhër me një rezultat të panjohur

doctor-overview-see-machine-orders = Shihni porositë e pajisjeve

doctor-overview-no-orders-cancelled-by-a-machine = Asnjë porosi e anuluar nga një pajisje

doctor-overview-review-cancelled-orders = Rishiko porositë e anuluara

doctor-overview-a-result-has-waited-too-long-to-be-sent = Një rezultat ka pritur shumë për t'u dërguar

doctor-overview-machine-has-gone-quiet = Pajisja ka rënë në heshtje

doctor-overview-machine-software-version-changed = Versioni i softuerit të pajisjes ndryshoi

doctor-overview-disk-space-is-low = Hapësira në disk është e ulët

doctor-overview-order-list-is-out-of-date = Lista e porosive është e vjetëruar

doctor-overview-orders-with-unknown-outcome-2 = Porositë me rezultat të panjohur

doctor-overview-items-need-review = Artikujt kërkojnë shqyrtim

doctor-overview-results-cannot-be-delivered = Rezultatet nuk mund të dorëzohen

doctor-overview-error-admin-off = faqet e administratorit janë fikur në cilësimet (pa tabelë [admin]), kështu që motori që funksionon nuk mund të kërkohet

doctor-overview-error-not-running = asgjë nuk përgjigjet në adresën e administratorit { $value }: motori nuk funksionon

doctor-overview-error-unreachable = adresa e administratorit { $address } nuk mund të arrihet: { $source }

doctor-overview-error-timed-out = motori nuk u përgjigj brenda { $as-secs } sekondave

doctor-overview-error-key = çelësi i administratorit nuk mund të lexohet si ky përdorues ({ $value }); ekzekutoni komandën si administrator

doctor-overview-error-key-thread = leximi i çelësit të administratorit ndaloi: { $value }

doctor-overview-error-client = kërkesa nuk mund të përgatitej: { $value }

doctor-overview-error-unproven = diçka tjetër përgjigjet në adresën e administratorit: nuk mund të vërtetohej se është ky motor Analyte, kështu që çelësi i administratorit nuk u dërgua

doctor-overview-error-silent = motori nuk u përgjigj: { $value }

doctor-overview-error-status = motori u përgjigj HTTP { $value }

doctor-overview-error-too-large = përgjigja e motorit ishte shumë e madhe

doctor-overview-error-unreadable = Përgjigja e motorit nuk mund të lexohej: { $value }

doctor-overview-error-request = kërkesa nuk mund të shkruhet: { $value }

doctor-overview-error-not-sent = motori nuk mund të arrihet: { $value }

doctor-overview-error-timed-out-2 = motori nuk iu përgjigj ndryshimit brenda { $as-secs } sekondave

doctor-overview-signal-path-section = 02 · Rruga e sinjalit

doctor-overview-signal-path-flow = — pajisja → Analyte → aplikacione të laboratorit

doctor-overview-disk-alerts-section = 04 · Disku dhe sinjalizimet

doctor-overview-lab-app-key-advice = futni çelësat e tij në faqen e aplikacioneve të laboratorit dhe më pas rinisni motorin
