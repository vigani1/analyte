### Review status: machine-translated, needs native review (2026-11-27)
###
### Glosario para la revisión
### sample = muestra; result = resultado; test = prueba
### held for review = retenido para revisión; Needs review = Requiere revisión
### machine = equipo; analyzer = analizador; lab app = aplicación del laboratorio
### delivery = entrega; order = orden; record only = solo grabar; engine = motor
### store = almacén; program file = archivo del programa; doctor = diagnóstico
### Los comandos, las clases CSS y los tokens OK, WARN, FAIL, PROBLEM y ATTENTION no se traducen.

doctor-overview-free-disk-space = Espacio libre en disco

doctor-overview-always-kept-free = Siempre se mantiene libre

doctor-overview-analyte-stops-receiving-below-this = Por debajo de esto, Analyte deja de recibir

doctor-overview-disk-status = Estado del disco

doctor-overview-active-alerts = Alertas activas

doctor-overview-alert-relays = Retransmisión de alertas

doctor-overview-relay-status-for-support = Estado de la retransmisión para soporte

doctor-overview-main-center = main--center

doctor-overview-let-s-connect-your-first-machine = Conectemos su primer equipo.

doctor-overview-analyte-reads-results-from-your-lab-s-analyzers-keeps = Analyte lee los resultados de los analizadores de su laboratorio, guarda una copia y los envía a su aplicación del laboratorio. Las órdenes van en sentido contrario.

doctor-overview-set-up-your-first-machine = Configure su primer equipo

doctor-overview-the-engine-is-running = El motor está en funcionamiento.

doctor-overview-what-to-do = Qué hacer: { $error }

doctor-overview-fix-on = { "   " }Se corrige en: { $page } ({ $href })

doctor-overview-command = { "   " }Comando: { $command }

doctor-overview-analyte-cannot-say-how-things-are-right-now = Analyte no puede indicar el estado en este momento

doctor-overview-problem = PROBLEM

doctor-overview-attention = ATTENTION

doctor-overview-add-an-admin-table-with-a-port-to-the = añada una tabla [admin] con un puerto al archivo del programa y reinicie el motor (consulte docs/operations/operate.md)

doctor-overview-run-this-from-an-administrator-powershell-if-the-key = ejecútelo desde un PowerShell de administrador; si falta el archivo de la clave, ejecute analyte admin token --rotate y reinicie el motor

doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start = inicie el motor (Start-Service Analyte, o systemctl start analyte en Linux), espere unos segundos y vuelva a intentarlo; si no se inicia, ejecute analyte doctor y analyte logs

doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection = busque el programa que ocupa el puerto de administración (Get-NetTCPConnection -LocalPort <port>) y deténgalo, o reinicie el motor con Restart-Service Analyte si la clave de administración se renovó después de que se iniciara

doctor-overview-check-that-this-command-and-the-engine-are-the = compruebe que este comando y el motor son de la misma versión de Analyte y luego ejecute analyte logs

doctor-overview-the-engine-could-not-read-its-state = El motor no pudo leer su estado.

doctor-overview-sending-to-lab-app-is-failing-waiting = El envío a la aplicación del laboratorio { $destination } está fallando: { $waiting } en espera

doctor-overview-sending-to-lab-app-is-paused-waiting = El envío a la aplicación del laboratorio { $destination } está en pausa: { $waiting } en espera

doctor-overview-analyte-stopped-working-with = Analyte dejó de trabajar con { $machine }

doctor-overview-orders-cancelled-by-a-machine = Órdenes canceladas por un equipo: { $cancelled }

doctor-overview-for = { $UNCHECKED-WARNING } para { $machine }

doctor-overview-disk-space-is-low-free = Queda poco espacio en disco: { $free } libres

doctor-overview-check-the-software-version-reported-by = Compruebe la versión de software que indica { $machine }.

doctor-overview-has-gone-quiet = { $machine } ha dejado de enviar datos

doctor-overview-has-sent-nothing-for = { $machine } no ha enviado nada desde hace { $seconds }

doctor-overview-a-result-has-waited-to-be-sent = Un resultado lleva { $seconds } esperando su envío

doctor-overview-waiting = , { $count } en espera

doctor-overview-cannot-send-to-lab-app = No se puede enviar a la aplicación del laboratorio { $destination }: { $why }{ $waiting }

doctor-overview-orders-with-unknown-outcome = Órdenes con desenlace desconocido: { $count }

doctor-overview-sending = Envío

doctor-overview-sending-in-line = envío

doctor-overview-disk = Disco

doctor-overview-analyte-review-list = analyte review list

doctor-overview-analyte-actor-you-deliveries-list-reason-why = analyte --actor <you> deliveries list --reason "<why>"

doctor-overview-analyte-orders-list = analyte orders list

doctor-overview-analyte-orders-cancelled = analyte orders cancelled

doctor-overview-analyte-data-keeping = analyte data-keeping

doctor-overview-analyte-actor-you-orders-resume-after-restore-reason-why = analyte --actor <you> orders resume-after-restore --reason "<why>"

doctor-overview-analyte-activity = analyte activity

doctor-overview-analyte-machines-list = analyte machines list

doctor-overview-analyte-lab-apps-list = analyte lab-apps list

doctor-overview-disk-almost-full-analyte-refuses-new-data-from-machines = Disco casi lleno: Analyte rechaza los datos nuevos de los equipos

doctor-overview-analyte-could-not-save-a-machine-software-version-warning = Analyte no pudo guardar un aviso de versión de software de un equipo

doctor-overview-keys-missing = faltan claves

doctor-overview-not-set-up-any-more = ya no está configurado

doctor-overview-items-may-need-review = Puede haber elementos que requieren revisión

doctor-overview-bytes-free = { $free } bytes libres

doctor-overview-bytes = { $reserve } bytes

doctor-overview-alert = Alerta

doctor-overview-responder = Responsable

doctor-overview-raised = Emitida

doctor-overview-last-value = Último valor

doctor-overview-pick-the-machine-s-model = Elija el modelo del equipo

doctor-overview-choose-how-it-s-connected-and-which-lab-app = Elija cómo está conectado y qué aplicación del laboratorio recibe sus resultados

doctor-overview-run-a-listen-only-test-analyte-only-reads-and = Ejecute una prueba de solo escucha. Analyte solo lee y no envía nada al equipo

doctor-overview-save-then-restart-analyte-to-use-the-new-settings = Guarde y luego reinicie Analyte para usar la nueva configuración

doctor-overview-nothing-needs-you-right-now = Nada requiere su atención en este momento.

doctor-overview-analyte-keeps-everything-it-has-received-while-you-fix = Analyte conserva todo lo que ha recibido mientras usted corrige esto.

doctor-overview-legend-line-legend-line-wait = legend__line legend__line--wait

doctor-overview-legend-line-legend-line-cut = legend__line legend__line--cut

doctor-overview-link-open = Enlace abierto

doctor-overview-waiting-2 = En espera

doctor-overview-broken = Interrumpido

doctor-overview-enough-space = Espacio suficiente

doctor-overview-almost-full-receiving-refused = Casi lleno: recepción rechazada

doctor-overview-order-list = Lista de órdenes · { $name }

doctor-overview-cannot-deliver = No se puede entregar

doctor-overview-sending-paused = Envío en pausa

doctor-overview-not-delivering = No está entregando

doctor-overview-up-to-date = Actualizada

doctor-overview-api-key = Clave de API

doctor-overview-signing-key = clave de firma

doctor-overview-its-keys-cannot-be-checked = No se pueden comprobar sus claves.

doctor-overview-out-of-date = No actualizada

doctor-overview-not-checked-yet = Todavía sin comprobar

doctor-overview-items-waiting-to-send = Elementos en espera de envío

doctor-overview-results-and-order-messages = resultados y mensajes de órdenes

doctor-overview-oldest-waiting = El más antiguo en espera

doctor-overview-the-check-did-not-finish-within-seconds = la comprobación no terminó en { $as-secs } segundos

doctor-overview-is-valid = { $config } es válido

doctor-overview-cannot-be-used = { $config } no se puede usar: { $error }

doctor-overview-what-to-do-2 = { "      " }Qué hacer: { $fix }

doctor-overview-doctor-and = Diagnóstico: { $failures } y { $warnings }.

doctor-overview-the-store-did-not-open-within-seconds = el almacén no se abrió en { $as-secs } segundos

doctor-overview-running-version-and-answering = en funcionamiento con la versión { $version } y respondiendo

doctor-overview-running-version-but-this-command-is-version = en funcionamiento con la versión { $version }, pero este comando es de la versión { $CARGO-PKG-VERSION }

doctor-overview-there-is-no-store-at-yet = todavía no hay ningún almacén en { $file }

doctor-overview-opened-but-did-not-close-cleanly = se abrió, pero no se cerró correctamente: { $error }

doctor-overview-cannot-be-opened = { $data-directory } no se puede abrir: { $error }

doctor-overview-free-is-always-kept-free = { $free } libres; siempre se mantienen libres { $reserve }

doctor-overview-free-at-or-below-the-kept-free-analyte-refuses = { $free } libres, igual o por debajo de los { $reserve } que se mantienen libres: Analyte rechaza los datos nuevos de los equipos

doctor-overview-the-free-space-could-not-be-read = no se pudo leer el espacio libre: { $error }

doctor-overview-a-usb-serial-number-names-a-network-or-folder = un número de serie USB ({ $number }) indica un enlace de red o de carpeta

doctor-overview-cable = { $what }: cable

doctor-overview-analyzer-settings = { $what }: ajustes del analizador

doctor-overview-report-poll-interval = { $what }: intervalo de consulta de informes

doctor-overview-mode = { $what }: modo

doctor-overview-every-byte-is-kept-and-recorded-under-captures = { $recording-only }; cada byte se conserva y se graba en captures/{ $id }

doctor-overview-devices = { $what }: dispositivos

doctor-overview-accepts-connections-only-from = acepta conexiones solo de { $allowed }

doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s = ejecute analyte machines set { $id } --allowed-peer <dirección IP del analizador>, o marque Aceptar solo este dispositivo en el paso 4 de su configuración

doctor-overview-the-check-for-folder-failed = falló la comprobación de la carpeta { $path }: { $error }

doctor-overview-folder-is-there = la carpeta { $path } existe

doctor-overview-is-not-a-folder = { $path } no es una carpeta

doctor-overview-folder-cannot-be-read = no se puede leer la carpeta { $path }: { $error }

doctor-overview-the-serial-ports-could-not-be-listed = no se pudieron listar los puertos serie: { $error }

doctor-overview-serial-port-is-present = el puerto serie { $name } está presente

doctor-overview-serial-port-is-not-present = el puerto serie { $name } no está presente

doctor-overview-the-adapter-with-serial-number-is-at = el adaptador con número de serie { $number } está en { $port }

doctor-overview-no-adapter-with-serial-number-is-plugged-in = no hay conectado ningún adaptador con número de serie { $number }

doctor-overview-adapters-report-serial-number = { $several } adaptadores indican el número de serie { $number }: { $join }

doctor-overview-nothing-listens-on-although-the-engine-runs = nada escucha en { $address }, aunque el motor está en funcionamiento

doctor-overview-is-free-for-analyte-to-listen-on = { $address } está libre para que Analyte escuche en ella

doctor-overview-is-held-by-the-running-engine = { $address } está ocupada por el motor en funcionamiento

doctor-overview-another-program-holds = otro programa ocupa { $address }

doctor-overview-is-held-by-the-engine-or-by-another-program = { $address } está ocupada, por el motor o por otro programa

doctor-overview-cannot-be-used-on-this-computer = { $address } no se puede usar en este ordenador: { $error }

doctor-overview-the-engine-connects-to-analyte-overview-says-if-the = el motor se conecta a { $address }; analyte overview indica si el enlace está desactivado

doctor-overview-not-tried-the-engine-may-be-connected-to-and = no se intentó: el motor puede estar conectado a { $address }, y un equipo puede aceptar una sola conexión a la vez

doctor-overview-accepts-a-connection = { $address } acepta una conexión

doctor-overview-does-not-accept-a-connection = { $address } no acepta una conexión: { $error }

doctor-overview-did-not-answer-within-seconds = { $address } no respondió en { $as-secs } segundos

doctor-overview-lab-app = Aplicación del laboratorio { $name }

doctor-overview-when-its-order-list-last-came-could-not-be = no se pudo leer cuándo llegó por última vez su lista de órdenes: { $error }

doctor-overview-credential-is-missing = falta la credencial { $name }

doctor-overview-its-keys-could-not-be-read-from-this-account = no se pudieron leer sus claves desde esta cuenta: { $error }

doctor-overview-resolves-to-no-address = { $host } no se resuelve en ninguna dirección

doctor-overview-does-not-resolve = { $host } no se resuelve: { $error }

doctor-overview-did-not-resolve-within-seconds = { $host } no se resolvió en { $as-secs } segundos

doctor-overview-answers-over-https-with-a-valid-certificate-http = { $url } responde por HTTPS con un certificado válido (HTTP { $status })

doctor-overview-gives-no-https-answer = { $url } no da ninguna respuesta HTTPS: { $error }

doctor-overview-last-complete-order-list = última lista de órdenes completa { $when }

doctor-overview-its-order-list-is-not-up-to-date-last = su lista de órdenes no está actualizada (última lista completa: { $when })

doctor-overview-create-it-with-new-localgroup-name-then-add-people = créelo con New-LocalGroup -Name "{ $GROUP }" y luego añada personas con Add-LocalGroupMember -Group "{ $GROUP }" -Member <user>

doctor-overview-the-group-exists = el grupo { $GROUP } existe

doctor-overview-the-group-is-missing-so-only-administrators-can-sign = falta el grupo { $GROUP }, así que solo los administradores pueden iniciar sesión con Windows

doctor-overview-is-an-account-but-not-a-local-group = { $GROUP } es una cuenta, pero no un grupo local

doctor-overview-the-group-could-not-be-looked-up = no se pudo buscar el grupo { $GROUP }: { $error }

doctor-overview-the-rule-is-there = la regla "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) existe

doctor-overview-no-rule-lets-analyzers-connect-to = ninguna regla "{ $FIREWALL-RULE }" ({ $FIREWALL-RULE-NAME }) permite que los analizadores se conecten a { $join }

doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended = no se pudieron leer las reglas del firewall (PowerShell terminó con { $status })

doctor-overview-the-firewall-rules-could-not-be-read = no se pudieron leer las reglas del firewall: { $error }

doctor-overview-ok = OK

doctor-overview-warn = WARN

doctor-overview-fail = FAIL

doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run = vuelva a ejecutar analyte doctor; si sigue lento, ejecute analyte logs

doctor-overview-fix-what-it-names-or-choose-the-right-file = corrija lo que indica (o elija el archivo correcto con --config) y vuelva a ejecutar analyte doctor; las demás comprobaciones necesitan un archivo del programa válido

doctor-overview-doctor-everything-checked-is-ok = Diagnóstico: todo lo comprobado está OK.

doctor-overview-store = Almacén

doctor-overview-listed-but-not-set-up-yet-so-it-does = en la lista, pero todavía sin configurar, así que no funciona

doctor-overview-choose-its-model-on-the-setup-page-save-then = elija su modelo en la página de configuración, guarde y luego reinicie el motor

doctor-overview-run-the-analyte-exe-the-service-runs-or-update = ejecute el analyte.exe que usa el servicio, o actualice ambos a la misma versión

doctor-overview-run-analyte-overview-for-where-and-how-to-fix = ejecute analyte overview para ver dónde y cómo corregir cada uno

doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run = vuelva a ejecutar analyte doctor; si sigue siendo desconocido, ejecute analyte logs

doctor-overview-start-the-engine-once-start-service-analyte-it-creates = inicie el motor una vez (Start-Service Analyte); así se crea el almacén

doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte = vuelva a ejecutar analyte doctor; si se repite, ejecute analyte logs

doctor-overview-run-this-from-an-administrator-powershell-if-it-names = ejecútelo desde un PowerShell de administrador; si indica una lista de acceso, consulte docs/operations/install.md

doctor-overview-check-the-machine-s-settings = compruebe la configuración del equipo

doctor-overview-machine-software = Software del equipo

doctor-overview-check-the-store-and-run-analyte-doctor-again = compruebe el almacén y vuelva a ejecutar analyte doctor

doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing = libere espacio en esta unidad; Analyte no elimina nada para hacer sitio

doctor-overview-check-the-data-directory-s-drive-in-file-explorer = compruebe la unidad de la carpeta de datos en el Explorador de archivos

doctor-overview-choose-the-machine-s-address-or-folder-on-the = elija la dirección o la carpeta del equipo en la página de configuración

doctor-overview-accepts-connections-from-any-device-on-the-network-which = acepta conexiones de cualquier dispositivo de la red, que podría enviar resultados haciéndose pasar por este equipo

doctor-overview-choose-the-machine-s-export-folder-on-the-setup = elija la carpeta de exportación del equipo en la página de configuración

doctor-overview-create-the-folder-or-share-it-again-or-choose = cree la carpeta o vuelva a compartirla, o elija otra en la página de configuración

doctor-overview-check-the-usb-serial-adapter-s-driver-in-device = compruebe el controlador del adaptador serie USB en el Administrador de dispositivos

doctor-overview-run-analyte-doctor-again = vuelva a ejecutar analyte doctor

doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the = conecte el adaptador serie USB o elija el puerto correcto en la página de configuración

doctor-overview-unplug-the-adapter-that-does-not-belong-to-this = desconecte el adaptador que no pertenece a este equipo

doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte = ejecute analyte overview: el servicio del equipo en Analyte puede haberse detenido; reinicie el motor con Restart-Service Analyte

doctor-overview-stop-the-other-program-or-choose-another-port-on = detenga el otro programa o elija otro puerto en la página de configuración y en el equipo

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell = active la página de administración para que doctor pueda saberlo, o compruébelo con Get-NetTCPConnection -LocalPort <port>

doctor-overview-choose-an-address-this-computer-has-on-the-setup = elija en la página de configuración una dirección que tenga este ordenador

doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2 = active la página de administración para que doctor pueda saber si el motor está en funcionamiento

doctor-overview-check-that-the-machine-is-on-its-network-settings = compruebe que el equipo está encendido, su configuración de red y el cable

doctor-overview-the-running-engine-has-its-keys = el motor en funcionamiento tiene sus claves

doctor-overview-its-api-key-and-signing-key-are-there = su clave de API y su clave de firma existen

doctor-overview-its-keys-were-not-checked = sus claves no se comprobaron

doctor-overview-no-order-list-is-kept-for-it = no se conserva ninguna lista de órdenes para ella

doctor-overview-restart-the-engine = reinicie el motor

doctor-overview-start-the-engine-and-run-analyte-doctor-again-the = inicie el motor y vuelva a ejecutar analyte doctor; el propio motor comprueba las claves

doctor-overview-its-address-has-no-host-name = su dirección no tiene nombre de host

doctor-overview-fix-its-address-on-the-lab-apps-page = corrija su dirección en la página Aplicaciones del laboratorio

doctor-overview-check-the-lab-app-s-address-on-the-lab = compruebe la dirección de la aplicación del laboratorio en la página Aplicaciones del laboratorio y el DNS de este ordenador

doctor-overview-check-this-computer-s-dns-and-network = compruebe el DNS y la red de este ordenador

doctor-overview-check-the-address-the-lab-app-s-server-and = compruebe la dirección, el servidor de la aplicación del laboratorio y su certificado; un certificado en el que este ordenador no confía debe corregirse en el servidor

doctor-overview-check-that-the-engine-runs-and-the-lab-app = compruebe que el motor está en funcionamiento y que la aplicación del laboratorio responde; mientras tanto, los equipos que piden órdenes no reciben respuesta

doctor-overview-computername-is-not-set-so-the-group-cannot-be = COMPUTERNAME no está definido, así que no se puede buscar el grupo

doctor-overview-run-doctor-from-a-normal-powershell-window = ejecute doctor desde una ventana normal de PowerShell

doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to = vuelva a ejecutar el instalador, o permita en el Firewall de Windows Defender el TCP entrante a estos puertos desde la red del laboratorio

doctor-overview-check-the-rule-in-windows-defender-firewall = compruebe la regla en el Firewall de Windows Defender

doctor-overview-reading-the-firewall-rules-took-too-long = la lectura de las reglas del firewall tardó demasiado

doctor-overview-with-an-unknown-outcome-check-the-machine = { $uncertain } con desenlace desconocido (compruebe el equipo)

doctor-overview-cancelled-by-a-machine = { $cancelled } canceladas por un equipo

doctor-overview-analyte-could-not-read-its-store = Analyte no pudo leer su almacén.

doctor-overview-analyte-could-not-read-whether-order-sending-is-paused = Analyte no pudo leer si el envío de órdenes está en pausa tras una restauración.

doctor-overview-analyte-could-not-read-the-free-disk-space = Analyte no pudo leer el espacio libre en disco.

doctor-overview-analyte-could-not-read-how-many-items-need-review = Analyte no pudo leer cuántos elementos requieren revisión.

doctor-overview-analyte-could-not-read-when-each-lab-app-last = Analyte no pudo leer cuándo aceptó cada aplicación del laboratorio su última entrega.

doctor-overview-analyte-could-not-read-which-lab-apps-keep-failing = Analyte no pudo leer qué aplicaciones del laboratorio siguen sin aceptar entregas.

doctor-overview-analyte-could-not-read-the-active-alerts = Analyte no pudo leer las alertas activas.

doctor-overview-analyte-could-not-read-the-machine-orders = Analyte no pudo leer las órdenes de equipo.

doctor-overview-analyte-could-not-read-when-each-machine-last-sent = Analyte no pudo leer cuándo envió datos cada equipo por última vez.

doctor-overview-analyte-could-not-read-what-waits-for-each-lab = Analyte no pudo leer qué espera para cada aplicación del laboratorio.

doctor-overview-analyte-could-not-read-what-waits-for-lab-apps = Analyte no pudo leer qué espera para las aplicaciones del laboratorio que no usa ningún equipo configurado.

doctor-overview-analyte-could-not-read-the-alert-relays = Analyte no pudo leer la retransmisión de alertas.

doctor-overview-analyte-read-everything-but-could-not-write-the-page = Analyte leyó todo, pero no pudo escribir la página.

doctor-overview-no-orders-with-an-unknown-outcome = No hay órdenes con desenlace desconocido

doctor-overview-see-machine-orders = Ver órdenes de equipo

doctor-overview-no-orders-cancelled-by-a-machine = No hay órdenes canceladas por un equipo

doctor-overview-review-cancelled-orders = Revisar órdenes canceladas

doctor-overview-a-result-has-waited-too-long-to-be-sent = Un resultado lleva demasiado tiempo esperando su envío

doctor-overview-machine-has-gone-quiet = El equipo ha dejado de enviar datos

doctor-overview-machine-software-version-changed = La versión de software del equipo ha cambiado

doctor-overview-disk-space-is-low = Queda poco espacio en disco

doctor-overview-order-list-is-out-of-date = La lista de órdenes no está actualizada

doctor-overview-orders-with-unknown-outcome-2 = Órdenes con desenlace desconocido

doctor-overview-items-need-review = Elementos que requieren revisión

doctor-overview-results-cannot-be-delivered = No se pueden entregar los resultados

doctor-overview-error-admin-off = las páginas de administración están desactivadas en la configuración (no hay tabla [admin]), así que no se puede consultar al motor en funcionamiento

doctor-overview-error-not-running = nada responde en la dirección de administración { $value }: el motor no está en funcionamiento

doctor-overview-error-unreachable = no se puede acceder a la dirección de administración { $address }: { $source }

doctor-overview-error-timed-out = el motor no respondió en { $as-secs } segundos

doctor-overview-error-key = no se puede leer la clave de administración con este usuario ({ $value }); ejecute el comando como administrador

doctor-overview-error-key-thread = la lectura de la clave de administración se detuvo: { $value }

doctor-overview-error-client = no se pudo preparar la solicitud: { $value }

doctor-overview-error-unproven = otra cosa responde en la dirección de administración: no pudo demostrar que es este motor de Analyte, así que no se envió la clave de administración

doctor-overview-error-silent = el motor no respondió: { $value }

doctor-overview-error-status = el motor respondió HTTP { $value }

doctor-overview-error-too-large = la respuesta del motor era demasiado grande

doctor-overview-error-unreadable = no se pudo leer la respuesta del motor: { $value }

doctor-overview-error-request = no se pudo escribir la solicitud: { $value }

doctor-overview-error-not-sent = no se pudo acceder al motor: { $value }

doctor-overview-error-timed-out-2 = el motor no respondió al cambio en { $as-secs } segundos

doctor-overview-signal-path-section = 02 · Recorrido de la señal

doctor-overview-signal-path-flow = — equipos → Analyte → aplicaciones del laboratorio

doctor-overview-disk-alerts-section = 04 · Disco y alertas

doctor-overview-lab-app-key-advice = introduzca sus claves en la página Aplicaciones del laboratorio y luego reinicie el motor

doctor-overview-orders-section = 03 · Órdenes
