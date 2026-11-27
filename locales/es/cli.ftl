### Review status: machine-translated, needs native review (2026-11-27)
###
### Glosario para la revisión
### sample = muestra; sample ID = ID de muestra; result = resultado; test = prueba
### rerun = repetición; resend = reenvío; reprocess = reprocesar
### held for review = retenido para revisión; Needs review = Requiere revisión
### machine = equipo; analyzer = analizador; lab app = aplicación del laboratorio
### delivery = entrega; order = orden; record only = solo grabar; hold = retención
### recording = grabación; program file = archivo del programa; engine = motor
### Los nombres de comandos, opciones, archivos y los tokens OK, WARN y FAIL no se traducen.

cli-lang-help = Idioma de la interfaz (una etiqueta BCP 47). Por defecto se usa LC_ALL, LANG o el inglés

cli-interpretation-recorded-decisions-inspect-parked-items-for-failures = Interpretación { $version } registrada; decisiones: { $outcomes }. Revise los elementos apartados para ver los fallos.

cli-using = Usando { $path } ({ $describe }).

cli-scm-dispatch-was-not-selected = No se seleccionó el despacho de SCM ({ $shutdown-timeout-secs })

cli-resolution = { $id } { $evidence } resolution={ $as-deref }

cli-item-has-no-eligible-measurement-comparison = El elemento { $id } no tiene ninguna comparación de mediciones aplicable.

cli-continue-with-after = Continúe con --after { $id }

cli-not-set-up-choose-its-definition-on-the-setup = { $id } no está configurado: elija su definición en la página de configuración

cli-restored-order-sending-paused-pending-reconciliation-previous-store = Restaurado; el envío de órdenes está en pausa hasta la conciliación. Almacén anterior: { $kept }

cli-actor-is-required-for-recovery-commands = --actor es obligatorio para los comandos de recuperación

cli-dispatched-in-main = despachado en main

cli-reprocess-must-use-the-recovery-entry-point = reprocess debe usar el punto de entrada de recuperación

cli-resolve-must-use-the-recovery-entry-point = resolve debe usar el punto de entrada de recuperación

cli-no-machines-add-one-on-the-setup-page = no hay equipos: añada uno en la página de configuración

cli-delivery-queued-with-its-original-id-and-bytes = Entrega puesta en cola con su ID y sus bytes originales.

cli-backup-verified = Copia de seguridad verificada.

cli-no-log-file-at = No hay archivo de registro en { $path }.

cli-the-windows-service-writes-it-while-it-runs-on = El servicio de Windows lo escribe mientras está en funcionamiento. En Linux, el servicio escribe en el diario del sistema:

cli-more-analyte-activity-before = Más: analyte activity --before { $next }

cli-raised-for = { $alert } ({ $signal }) · emitida { $raised } · para { $responder }

cli-fix-on = { "  " }Se corrige en: { $page }

cli-command = { "  " }Comando: { $command }

cli-oldest-message-kept = Mensaje más antiguo conservado: { $oldest }

cli-more-analyte-data-keeping-after = Más: analyte data-keeping --after { $next }

cli-sample-order = Muestra { $sample } · orden { $order }

cli-more-analyte-orders-cancelled-after = Más: analyte orders cancelled --after { $next }

cli-no-earlier-activity-this-is-past-the-oldest-line = No hay actividad anterior: esto es anterior a la línea más antigua que conserva Analyte.

cli-no-activity-yet = Todavía no hay actividad.

cli-no-active-alerts = No hay alertas activas.

cli-not-recorded = no registrado

cli-how-long-messages-are-kept = Cuánto tiempo se conservan los mensajes

cli-no-machine-is-set-up = { "  No hay ningún equipo configurado." }

cli-not-run-yet = { "  Todavía no se ha ejecutado" }

cli-no-holds-messages-are-deleted-only-as-kept-above = { "  No hay retenciones: los mensajes solo se eliminan según la conservación indicada arriba." }

cli-no-orders-cancelled-by-a-machine = No hay órdenes canceladas por un equipo.

cli-settings-checked-for-this-version = Configuración comprobada para esta versión.

cli-exhausted-alert-events-queued-with-original-identities = Eventos de alerta agotados puestos en cola con sus identidades originales.

cli-retention-hold-updated = Retención actualizada.

cli-no-recordings = { $machine }: no hay grabaciones

cli-bytes-last-changed = { $machine } { $file } { $bytes } bytes, último cambio { $at }{ $else }

cli-copied-to-it-contains-patient-data-de-identify-a = Copiado en { $target }. Contiene datos de pacientes: desidentifique una copia antes de compartirla.

cli-wrote-values-replaced-distinct-the-original-is-unchanged = Se escribió { $path }: { $replaced } valores sustituidos ({ $distinct } distintos). El original no ha cambiado.

cli-copy-to = copiar en

cli-the-copy-exceeds-the-recording-file-bound = la copia supera el límite de tamaño del archivo de grabación

cli-write-a-reason-it-is-recorded-in-the-history = Escriba un motivo. Se registra en el historial.

cli-keep-the-reason-to-256-characters = El motivo no debe superar los 256 caracteres.

cli-order-sending-paused-restored-store-requires-machine-app-reconciliation = ENVÍO DE ÓRDENES EN PAUSA: el almacén restaurado requiere conciliar el equipo y la aplicación.

cli-help-main-cli-config = Archivo del programa para los comandos del operador. Si no se indica: el archivo que indica ANALYTE_CONFIG; si no, analyte.toml en la carpeta actual, si existe; si no, el archivo del programa instalado (Windows: %ProgramData%\Analyte\config\analyte.toml; Linux: /etc/analyte/analyte.toml), si existe. El archivo encontrado de esta forma se indica en la salida de error estándar

cli-help-main-cli-actor = Identidad explícita del operador local, registrada en la auditoría; no puede empezar como la de un inicio de sesión en las páginas de administración (`admin-session-`, `windows:`)

cli-help-main-command-retry-alerts = Reintenta los eventos de alerta agotados con sus identidades y bytes originales

cli-help-main-command-captures = Conversaciones grabadas (ADR 0009, ADR 0034): las lista o copia una a una carpeta. Contienen datos de pacientes; ambas acciones se auditan

cli-help-main-command-holds = Aplica o libera una retención indefinida

cli-help-main-command-admin = La clave de la página de administración

cli-help-main-command-machines = Lista, añade, configura, prueba y elimina equipos con las mismas comprobaciones que la página de configuración; los cambios se aplican tras un reinicio

cli-help-main-command-lab-apps = Lista, añade, cambia y elimina aplicaciones del laboratorio y guarda sus claves con las mismas comprobaciones que la página Aplicaciones del laboratorio; los cambios se aplican tras un reinicio

cli-help-main-command-orders = Órdenes de equipo: listar, canceladas por un equipo, conciliar, reanudar tras una restauración

cli-help-main-command-check-config = Comprueba un archivo del programa y lista la versión de configuración y el nivel de evidencia de cada equipo

cli-help-main-command-run = Ejecuta el motor en esta ventana hasta pulsar Ctrl+C (el servicio usa `service run`)

cli-help-main-command-run-shutdown-timeout-secs = Plazo máximo del proceso una vez iniciado el apagado

cli-help-main-command-service = Punto de entrada del servicio de Windows; inícielo con Start-Service Analyte

cli-help-main-command-status = Líneas técnicas de estado para soporte: alertas, colas, últimos datos de cada equipo

cli-help-main-command-parked = Requiere revisión: elementos que Analyte conservó pero no pudo procesar. `review` y `parked` son el mismo comando

cli-help-main-command-receipts = Reprocesa un mensaje guardado con la configuración actual

cli-help-main-command-deliveries = Entregas a las aplicaciones del laboratorio: listar, volver a enviar

cli-help-main-command-restore = Restaura el almacén desde una copia de seguridad verificada; el envío de órdenes queda en pausa hasta la conciliación

cli-help-main-command-backup = Escribe en un archivo una copia de seguridad verificada del almacén

cli-help-main-command-completions = Muestra un script que completa los comandos de analyte en `shell`. PowerShell: analyte completions powershell | Out-String | Invoke-Expression

cli-help-main-admin-token = Sustituye la credencial protegida de administración y la muestra una sola vez

cli-help-main-parked-compare = Muestra la comparación protegida de resultados y su instantánea de confirmación

cli-help-main-parked-suggestions = Lista sugerencias de muestras que faltan para una página de elementos abiertos

cli-help-main-parked-decide = Confirma una comparación de mediciones mostrada

cli-help-main-parked-list-after = Continúa después del último ID de una página limitada anterior

cli-about = Servicio fiel de recepción de analizadores

cli-help-operator-command-resume-after-restore = Libera la pausa por restauración una vez conciliados los equipos y la aplicación

cli-help-operator-command-resume-after-restore-reason = Evidencia de la decisión del operador

cli-help-operator-command-list = Lista una página limitada de instantáneas del ciclo de vida de las órdenes de equipo

cli-help-operator-command-list-after = Continúa después de este ID local de orden de equipo

cli-help-operator-command-list-machine = Limita la página a un equipo instalado

cli-help-operator-command-cancelled = La página Canceladas por el equipo: elementos de orden que un equipo canceló por sí mismo, para comprobarlos con el laboratorio y la aplicación del laboratorio. Muestra el ID de muestra de cada elemento según la copia de la aplicación del laboratorio, como la página, nunca valores de resultados; cada ejecución se registra en la auditoría

cli-help-operator-command-cancelled-after = Continúa después de esta posición, tal como la mostró la página anterior

cli-help-operator-command-cancelled-json = Muestra los elementos en JSON para scripts

cli-help-operator-command-reconcile = Resuelve un desenlace incierto del equipo con la evidencia del operador

cli-help-operator-command-reconcile-id = ID local de la orden de equipo mostrado por orders list

cli-help-operator-command-reconcile-outcome = Desenlace verificado por el operador

cli-help-operator-command-reconcile-reason = Explicación obligatoria que se conserva en la auditoría y en el evento de la aplicación

cli-help-holds-command-receipt = Aplica o libera una retención sobre una recepción

cli-help-holds-command-receipt-id = UUID de la recepción en el host

cli-help-holds-command-receipt-release = Libera una retención existente en lugar de aplicar una

cli-help-holds-command-receipt-reason = Explicación obligatoria del operador

cli-help-holds-command-order = Aplica o libera una retención que cubre una orden de la aplicación y sus recepciones vinculadas

cli-help-holds-command-order-destination = Espacio de nombres de destino

cli-help-holds-command-order-id = Identidad de la orden en la aplicación

cli-help-captures-command-list = Lista las grabaciones de cada equipo: archivo, tamaño y último cambio. Contienen datos de pacientes

cli-help-captures-command-list-machine = Solo las grabaciones de este equipo

cli-help-captures-command-export = Copia una grabación a una carpeta, byte a byte, sin sobrescribir. Contiene datos de pacientes: desidentifique una copia antes de compartirla

cli-help-captures-command-export-machine = El nombre del equipo

cli-help-captures-command-export-file = El nombre del archivo de la grabación, tal como lo muestra `captures list`

cli-help-captures-command-export-to = La carpeta donde copiarla; debe existir

cli-help-captures-command-export-reason = Por qué se exporta; la auditoría lo conserva

cli-help-captures-command-de-identify = Escribe en una carpeta una copia desidentificada de una grabación: los campos del paciente pasan a ser valores sintéticos con la misma forma y se recalculan las sumas de control. El original no cambia. Revise la copia antes de compartirla

cli-help-captures-command-de-identify-to = La carpeta donde escribir la copia; debe existir

cli-help-captures-command-de-identify-reason = Por qué se desidentifica; la auditoría lo conserva

cli-help-captures-command-de-identify-checksum-case = Mayúsculas o minúsculas de la suma de control que usa el equipo (VIDAS); solo es necesario cuando la grabación no lo muestra, y entonces el comando lo indica

cli-help-command-command-overview = El Resumen en palabras sencillas: "Todo en orden" o cada asunto que requiere su atención, con la página y el comando que lo corrigen. Consulta el motor en funcionamiento. Código de salida: 0 todo en orden, 1 requiere revisión, 2 un problema detiene los datos, 3 desconocido

cli-help-command-command-overview-json = Muestra la respuesta en JSON para scripts

cli-help-command-command-activity = La página Actividad: mensajes guardados, intentos de entrega (los intentos fallidos seguidos en una sola línea), órdenes y cambios, primero los más recientes. Las líneas indican IDs de muestra y recuentos, nunca valores de resultados; cada ejecución se registra en la auditoría

cli-help-command-command-activity-limit = Cuántas líneas mostrar (de 1 a 200)

cli-help-command-command-activity-machine = Solo las líneas de este equipo

cli-help-command-command-activity-before = Continúa antes de esta posición, tal como la mostró la página anterior

cli-help-command-command-activity-json = Muestra las líneas en JSON para scripts

cli-help-command-command-alerts = Alertas activas, cada una con su motivo y dónde se corrige

cli-help-command-command-data-keeping = La página Conservación de datos: cuánto tiempo se conservan los mensajes de cada equipo, la última limpieza, el mensaje más antiguo conservado y las retenciones activas

cli-help-command-command-data-keeping-after = Continúa después de este número de retención, tal como lo mostró la página anterior

cli-help-command-command-data-keeping-json = Muestra la página en JSON para scripts

cli-help-command-command-doctor = Una comprobación de estado de solo lectura para informática: el archivo del programa, el motor, la conexión de cada equipo, cada aplicación del laboratorio, el espacio en disco y el inicio de sesión de Windows. Cada línea indica OK, WARN o FAIL y qué hacer. No cambia nada. Código de salida: 0 todo OK, 1 un aviso, 2 un fallo

cli-help-command-command-doctor-json = Muestra las comprobaciones en JSON para scripts

cli-help-command-command-confirm-software = Confirma que la configuración actual se comprobó para la versión indicada

cli-help-command-command-confirm-software-machine = ID del equipo en la página Equipos

cli-help-command-command-confirm-software-version = Versión de software exacta mostrada en el aviso

cli-help-command-command-logs = Dónde está el registro del motor y sus últimas líneas. El registro no contiene datos de pacientes: Analyte registra nombres, recuentos e IDs, nunca mensajes ni valores

cli-help-command-command-logs-tail = Cuántas de las últimas líneas mostrar (de 1 a 1000)

cli-help-command-alerts-list = Lista todas las alertas activas

cli-help-command-alerts-list-json = Muestra las alertas en JSON para scripts

cli-help-lab-apps-command-list = Lista las aplicaciones del laboratorio: dirección, tiempos, los equipos que usan cada una y dónde se encuentra cada clave (nunca su valor)

cli-help-lab-apps-command-add = Añade una aplicación del laboratorio. Después se piden sus dos claves sin mostrarlas, o se leen de la entrada estándar, una por línea: la clave de API y luego la clave de firma

cli-help-lab-apps-command-add-name = Letras de la A a la Z, dígitos, - y _ (como máximo 64); también da nombre a sus claves

cli-help-lab-apps-command-set = Cambia la dirección o los tiempos de una aplicación del laboratorio; los valores no indicados se mantienen

cli-help-lab-apps-command-set-name = El nombre de la aplicación del laboratorio

cli-help-lab-apps-command-keys = Sustituye las claves de una aplicación del laboratorio, pedidas sin mostrarlas o leídas de la entrada estándar (la clave de API y luego la clave de firma, una por línea). Una respuesta vacía mantiene la clave guardada. Las claves nunca se toman de las opciones

cli-help-lab-apps-command-remove = Elimina una aplicación del laboratorio que ningún equipo usa y por la que nada espera

cli-help-lab-apps-settings-address = Su dirección HTTPS, por ejemplo `https://lab.example.org`

cli-help-lab-apps-settings-freshness-seconds = Cuánto tiempo se considera actual su lista de órdenes, en segundos enteros

cli-help-lab-apps-settings-request-timeout-seconds = Cuánto tiempo esperar su respuesta, en segundos enteros

cli-help-lab-apps-settings-review-notices = Si se le envían avisos de Requiere revisión: `yes` o `no` (ADR 0038). Una aplicación del laboratorio nueva no recibe ninguno salvo que aquí se indique yes

cli-help-args-cable-baud = Cable serie: velocidad en baudios: 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 o 115200

cli-help-args-cable-data-bits = Cable serie: bits de datos, de 5 a 8

cli-help-args-cable-parity = Cable serie: paridad: none, odd o even

cli-help-args-cable-stop-bits = Cable serie: bits de parada, 1 o 2

cli-help-args-cable-flow-control = Cable serie: control de flujo: none, hardware (RTS/CTS) o software (XON/XOFF)

cli-help-args-choice-model = El modelo del equipo: su clave tal como la lista `machines models`, o `installed` para mantener el actual

cli-help-args-choice-serial-port = Un puerto serie por su nombre, por ejemplo COM3

cli-help-args-choice-usb-serial = Un adaptador serie USB por su número de serie, para encontrarlo en cualquier puerto USB

cli-help-args-choice-address = La dirección IP de un enlace de red (la de este ordenador cuando el equipo se conecta a Analyte, la del equipo cuando Analyte se conecta a él), con --port

cli-help-args-choice-port = El número de puerto de red, con --address

cli-help-args-choice-folder = La carpeta de la que Analyte lee los resultados

cli-help-args-choice-lab-app = La aplicación del laboratorio a la que este equipo envía resultados

cli-help-args-choice-allowed-peer = Para un equipo que se conecta a Analyte: aceptar conexiones solo de este dispositivo, la dirección IP propia del analizador; repítalo para varios. Cualquier otro dispositivo se rechaza (ADR 0026)

cli-help-args-choice-any-device = Para un equipo que se conecta a Analyte: volver a aceptar conexiones de cualquier dispositivo, eliminando sus dispositivos permitidos

cli-help-args-choice-choice = Uno de los ajustes propios del modelo, tal como el paso 2 los lista en Ajustes del analizador y `machines models` los nombra; repítalo para varios. Un ajuste no indicado mantiene su valor guardado o, si no, la sugerencia del modelo

cli-help-args-choice-record-only = Solo grabar (ADR 0034): mantener la comunicación con el analizador y grabar cada byte, para redactar su definición; no se envía nada a una aplicación del laboratorio, así que no hay --lab-app. También para un modelo que aún no está listo. Las grabaciones contienen datos de pacientes: expórtelas con `analyte captures export`

cli-help-args-choice-connect-normally = Sale de solo grabar: vuelve a conectar el equipo con normalidad (requiere un modelo listo y --lab-app). Los mensajes grabados nunca se entregan

cli-help-args-choice-poll-seconds = Autolyser: segundos entre consultas de informes, cuando el modelo o el perfil de captura lo piden

cli-help-args-choice-reports-carry-controls = Solo grabar, Autolyser: si sus informes incluyen controles: yes o no

cli-help-args-choice-vidas-acknowledgment = Solo grabar, VIDAS: el acuse de recibo del PC de VIDAS: literal o alternate

cli-help-args-choice-analyzer-mode = Solo grabar, para un modelo que lo pide (cobas e 411: elecsys o cobas): el modo que muestran los ajustes propios del analizador; `machines models` lo lista

cli-help-args-choice-repeated-results = Resultados repetidos, para una conexión normal: `automatic` (recomendado; un reenvío se conserva una vez, una corrección pasa a ser una nueva revisión) o `always_ask` (cada repetición espera en Requiere revisión). Si no se indica, se mantiene la opción guardada o, si no, automatic (ADR 0035)

cli-help-machines-command-list = Lista los equipos de la configuración: modelo, conexión, aplicación del laboratorio, si está configurado, versión de configuración y, desde el motor en funcionamiento, el estado en vivo

cli-help-machines-command-list-json = Muestra JSON para scripts en lugar de líneas

cli-help-machines-command-models = Lista los modelos de equipo de la carpeta de equipos: si están listos o no y cómo se comprobó cada uno

cli-help-machines-command-ports = Lista los puertos serie de este ordenador con su adaptador y su número de serie USB

cli-help-machines-command-add = Añade un equipo a la configuración; todavía no queda configurado. Los ajustes del cable serie indicados aquí se conservan para su configuración

cli-help-machines-command-add-name = Letras de la A a la Z, dígitos, - y _; también da nombre a su archivo de configuración

cli-help-machines-command-add-same-analyzer = El nombre tiene historial guardado y se trata del mismo analizador

cli-help-machines-command-set = Configura un equipo: su modelo, conexión y aplicación del laboratorio, comprobados como lo hace el paso Guardar. Las opciones no indicadas mantienen sus valores guardados

cli-help-machines-command-set-dry-run = Muestra lo que se escribiría y no escribe nada

cli-help-machines-command-test = Ejecuta la prueba de conexión de solo escucha del paso 3 con la configuración guardada o las opciones indicadas: no se envía ni se guarda nada

cli-help-machines-command-test-seconds = Cuánto tiempo escuchar, de 1 a 60 segundos

cli-help-machines-command-test-show-bytes = También muestra los bytes recibidos (como máximo 64 KiB). Pueden contener datos de pacientes: nunca los guarde ni los comparta

cli-help-machines-command-remove = Elimina un equipo de la configuración; sus resultados guardados y su historial se conservan

cli-help-machines-command-remove-anyway = Lo elimina aunque haya trabajo sin terminar

cli-error-reserved = --actor no puede empezar por "{ $value }": Analyte lo registra solo para las personas que inician sesión en las páginas de administración

cli-error-text = --actor: { $value }

cli-named-by-config = indicado por --config

cli-named-by-analyte-config = indicado por ANALYTE_CONFIG

cli-in-the-current-folder = en la carpeta actual

cli-the-installed-program-file = el archivo del programa instalado

cli-log-path = Registro: { $path }

cli-choice-write-name-value = escriba NOMBRE=VALOR, por ejemplo protocol_mode=alternate

cli-token-missing = falta el archivo del token de administración; ejecute analyte admin token --rotate

cli-token-invalid = el archivo del token de administración debe contener un único token de 256 bits codificado en base64

cli-token-permissions = el archivo del token de administración debe ser un archivo normal protegido (modo Unix 0600)

cli-token-access = archivo del token de administración: { $error }

cli-token-random = falló la fuente aleatoria del sistema para el token de administración: { $error }

cli-token-io = token de administración: falló la operación «{ $operation }»: { $error }

cli-token-op-inspect = inspeccionar la credencial

cli-token-op-read = leer la credencial

cli-token-op-create-staging = crear el archivo temporal protegido

cli-token-op-write-staging = escribir el archivo temporal

cli-token-op-flush-staging = vaciar a disco el archivo temporal

cli-token-op-install = instalar la credencial

cli-token-op-flush-directory = vaciar a disco la carpeta de la credencial

cli-setup-settings-do-not-load = La configuración de Analyte no se carga: { $error }

cli-setup-store-not-opened = No se pudieron abrir los datos guardados de Analyte: { $error }

cli-setup-ports-failed = falló la lista de puertos serie: { $error }

cli-setup-console-failed = no se pudo usar la consola: { $error }

cli-setup-no-actor = indique quién hace este cambio con --actor <nombre>; se registra en la auditoría

cli-setup-not-confirmed = no se guardó nada: no se confirmó

cli-setup-engine-stopped = solo el motor de Analyte puede guardar las claves, y no está en funcionamiento ({ $error })

cli-setup-keys-not-sent = las claves no se enviaron: { $error }

cli-setup-engine-no-answer = no se sabe si el motor guardó el cambio: { $error }

cli-advice-run-as-service-account = Ejecute este comando con la cuenta con la que funciona el servicio de Analyte, o configure las claves en la página Aplicaciones del laboratorio.

cli-advice-remove-anyway = Para eliminarlo de todos modos, vuelva a ejecutar el comando con --anyway.

cli-advice-same-analyzer = Si es el mismo analizador, vuelva a ejecutar el comando con --same-analyzer.

cli-advice-check-config = Vea qué está mal con: analyte check-config <archivo>.

cli-advice-start-engine = Inicie el motor con Start-Service Analyte en un PowerShell de administrador y vuelva a ejecutar este comando.

cli-advice-check-lab-apps-list = Compruébelo con analyte lab-apps list antes de volver a ejecutar el comando.

cli-setup-engine-runs-unknown = No se sabe qué ejecuta el motor ({ $why }), así que todos los equipos de la configuración cuentan como en funcionamiento.

cli-setup-ask-actor = Su nombre para el registro de auditoría

cli-setup-restart-note = { $restart } En Windows, ejecute Restart-Service Analyte en un PowerShell de administrador o pulse Reiniciar ahora en la página de configuración.

cli-fix-this = Corrija esto: { $reason }

cli-nothing-was-changed = No se cambió nada.

cli-reason-character-cannot-be-recorded = El motivo contiene un carácter que no se puede registrar.

cli-captures-patient-data = Estas grabaciones contienen datos de pacientes. Consérvelas en este ordenador; antes de compartir una, haga una copia desidentificada con captures de-identify y revísela (docs/develop/adding-a-machine.md, "Capture, de-identify and replay").

cli-captures-machine-name = el nombre de un equipo solo usa letras, dígitos, - y _: { $name }

cli-captures-file-name = indique una grabación tal como la muestra captures list (un nombre de archivo terminado en .rec.jsonl), no una ruta: { $name }

cli-captures-missing = el equipo { $machine } no tiene ninguna grabación llamada { $file }

cli-captures-no-folder = la carpeta { $path } no existe; créela primero

cli-captures-exists = { $path } ya existe; no se sobrescribió nada

cli-captures-too-many = el equipo { $machine } tiene más de 4096 grabaciones; mueva primero las más antiguas a otro sitio

cli-captures-format = no se puede leer la grabación: { $error }

cli-captures-io = falló la operación «{ $operation }» en { $path }: { $error }

cli-captures-op-read = leer

cli-captures-op-open = abrir

cli-captures-op-flush = vaciar a disco

cli-captures-op-create = crear

cli-captures-op-encode = codificar

cli-captures-op-write = escribir

cli-captures-stopped-early = { " " }(la captura se detuvo antes de tiempo: incompleta)

cli-captures-modified-unknown = desconocido

cli-deidentify-checklist = Revise la copia antes de compartirla; se sustituyeron los campos de texto libre.

cli-deidentify-unsupported = la desidentificación no está disponible para grabaciones { $protocol }; no se escribió nada

cli-deidentify-unknown-stack = la grabación indica una pila de protocolos que Analyte no conoce; no se escribió nada

cli-deidentify-unparsed = evento { $event }: { $reason }; Analyte no puede saber dónde está cada campo del paciente, así que no se escribió nada

cli-deidentify-checksum = evento { $event }: { $reason }; no se escribió nada

cli-deidentify-deadline = la desidentificación tardó más de { $seconds } segundos; no se escribió nada

cli-deidentify-random = falló la fuente aleatoria del sistema: { $error }

cli-deidentify-limit = se superó un límite de la desidentificación; no se escribió nada

cli-deidentify-checksum-not-hex = la suma de control original no son dos dígitos hexadecimales

cli-deidentify-checksum-case-unknown = la grabación no permite saber si esta suma de control va en mayúsculas o minúsculas; vuelva a ejecutarlo con --checksum-case upper o lower, según indique la configuración del equipo

cli-deidentify-checksum-not-sum = la suma de control original no es una suma de tres dígitos módulo 256

cli-deidentify-astm-outside-frames = bytes fuera de las tramas ASTM

cli-deidentify-astm-repeat-differs = una trama ASTM repetida difiere de la trama que repite

cli-deidentify-astm-header-delimiters = una cabecera ASTM sin cuatro delimitadores distintos

cli-deidentify-astm-record-before-header = un registro ASTM antes de su cabecera

cli-deidentify-astm-record-no-delimiter = un tipo de registro ASTM sin delimitador de campo

cli-deidentify-astm-manufacturer-record = un registro de fabricante (M), cuyos campos solo define el fabricante

cli-deidentify-astm-unknown-record = un tipo de registro ASTM desconocido

cli-deidentify-autolyser-host-outside = bytes del host fuera de un intercambio de Autolyser

cli-deidentify-autolyser-unasked = bytes del equipo que Analyte no pidió

cli-deidentify-autolyser-short-record = un registro de Autolyser más corto que su código de paciente

cli-deidentify-field-escape = un campo del paciente contiene una secuencia de escape que podría formar texto del paciente

cli-deidentify-hl7-outside-blocks = bytes fuera de los bloques MLLP

cli-deidentify-hl7-msh-encoding = un MSH sin caracteres de codificación distintos

cli-deidentify-hl7-segment-before-msh = un segmento HL7 antes de su MSH

cli-deidentify-hl7-segment-no-separator = un nombre de segmento HL7 sin separador de campo

cli-deidentify-hl7-segment-no-rules = un segmento HL7 para el que Analyte no tiene reglas de campos del paciente

cli-deidentify-vidas-foreign-byte = un byte que los paquetes VIDAS nunca contienen

cli-deidentify-vidas-outside-packets = bytes fuera de los paquetes VIDAS

cli-program-file-not-found = no se encontró ningún archivo del programa; se buscó en { $looked }. Indique --config <archivo> o configure { $variable }

cli-program-file-unreadable = no se puede comprobar { $path }: { $error }. El archivo del programa instalado solo pueden leerlo los administradores: ejecute el comando como administrador (un PowerShell con privilegios elevados en Windows, sudo en Linux) o indique --config <archivo>
