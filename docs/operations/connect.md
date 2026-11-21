# Configure and connect

## Keep the configuration parts separate

The program file, `analyte.toml`, selects a protected `data_directory`, storage
budgets and installed machines. Start from the
[program example](../../crates/analyte/examples/analyte.toml). It is synthetic,
not an installation-ready configuration. All paths in the program file resolve
relative to that file, including definition, site and overrides paths.

Each `[[machines]]` entry has `id`, `definition`, `site` and optional `overrides`.
Use a nonclinical installation ID, never a patient or sample ID.

| Layer | Contents |
| --- | --- |
| Shipped definition | Protocol, mappings, departures and evidence; qualified definitions belong under `machines/`. |
| Site file | `schema_version = 1`, endpoint (or `usb_serial` for a serial port), required retention period, the lab app it sends to (`lab_app = "<name>"`) and optional `time_zone`. |
| Optional local overrides | `schema_version = 1` and explicit replacement sections for this installation. |

Overrides replace whole sections and lists; an override's `endpoint` or
`usb_serial` replaces the site's port choice. They do not merge list entries or
change evidence. Retention belongs in the site file. Unknown keys fail loading.
`time_zone` (an IANA name such as `"Europe/Berlin"`, or a fixed offset such as
`"+01:00"`) is optional. It is used only to read an IHE LAW MSH-7 that arrives
without an offset, with a visible note; leave it out to keep refusing such
headers.
Keep all installation files outside the repository. The
[config examples](../../crates/core/config/examples/) teach syntax only. The
admin [Setup page](operate.md#setup-page) can write a machine's site file for
you, validated with the same rules and with a backup of the old one.

A synthetic site file looks like this:

```toml
schema_version = 1
retention = 90
endpoint = "COM9"          # the port by name, or instead:
# usb_serial = "A10K3XYZ"  # the port by its USB adapter's serial number
lab_app = "demo"           # a lab app listed in lab-apps.toml
```

### Lab apps: set once, picked by each machine

Each lab app is listed once in `lab-apps.toml`, beside the program file: its
name, address, deadlines and the names of its two keys. The keys themselves
stay in the locked credentials file or the service environment, under those
names. The admin [Lab apps page](operate.md#lab-apps-page) writes this file for
you, with the same checks and backups as the Setup page. A synthetic example
(also in the [config examples](../../crates/core/config/examples/lab-apps.toml)):

```toml
schema_version = 1

[[lab_apps]]
name = "demo"
base_url = "https://lab.example.invalid"
api_key_name = "demo-api"
signing_key_name = "demo-signing"
# freshness_seconds = 30, request_timeout_seconds = 10 when left out
```

Each `[[lab_apps]]` table follows the same rules as a site file's
`[destination]` table, and each name appears once. A machine's site file picks
one with `lab_app = "<name>"`. If it names none and exactly one lab app is
listed, it uses that one; with several, `check-config` refuses it and asks you
to choose. An unknown name is refused too. A missing `lab-apps.toml` lists no
lab apps. A `signing_rotation` entry may name any lab app.

The stored snapshot of a machine's settings (kept with every result and
digested) holds the lab app's values under `destination`, exactly as before, so
moving a lab app into `lab-apps.toml` with the same values does not change the
settings version.

### Older setups

Older site files that write their own `[destination]` table instead of
`lab_app`, and `[[retired_destinations]]` tables that older Setup pages kept in
the program file after a machine was removed, still load exactly as before and
are read as lab apps. Analyte never rewrites them at startup: a machine's own
table moves into `lab-apps.toml` when that machine is saved or removed on the
Setup page, and a kept table moves there when its lab app is saved on the Lab
app page. A site file may not set both `lab_app` and `[destination]`.

The same lab app name may appear in several of these places only with
identical values. If one name has different values in two places (for example
`lab-apps.toml` and a machine's site file), Analyte refuses to start and the
message names both places; make them the same, or give one another name.

### Name a serial port

A serial machine's site file names its port in one of two ways. Set exactly
one of them; setting both or neither fails `check-config`.

| By port name | By USB adapter serial number |
| --- | --- |
| `endpoint = "COM9"` (Windows) or `endpoint = "/dev/ttyUSB0"` (Linux) | `usb_serial = "A10K3XYZ"` |
| Opens exactly that port. If Windows gives the adapter another COM number, edit the file. | Finds the port whose USB adapter reports that serial number, at start and before every reconnect. If the adapter moves from COM9 to COM12, Analyte follows it. |

With `usb_serial`, Analyte never guesses:

- **No adapter has that number:** nothing is opened. The log says
  `no USB serial adapter with serial number A10K3XYZ is connected`, and Analyte
  keeps retrying with its usual reconnect delay (up to 5 seconds).
- **Two or more ports have that number** (a multi-port adapter, or two adapters
  sold with the same number): nothing is opened. The log names every matching
  port. Use `endpoint` with the port name instead.
- **One port has it:** that port is opened and the log names it
  (`serial port found by USB serial number`, `port="COM12"`).

Each of these is logged once when it changes, not on every retry. Letter case in
the serial number does not matter. TCP and folder links refuse `usb_serial`;
they always need `endpoint`.

The admin [Setup page](operate.md#setup-page) lists the detected ports with
their adapter names and serial numbers and makes this choice for you. To find
the number by hand on the lab PC:

- **Windows:** Device Manager → Ports (COM & LPT) → the adapter → Properties →
  Details → Device instance path. The serial number is the last part, after the
  last `\` (for FTDI adapters, between the last `+` and the next `\`). If that
  part contains `&`, such as `6&2A9C1F&0&1`, the adapter has no serial number of
  its own; Windows made that text up and it can change with the USB socket. Use
  the COM name for such an adapter.
- **Linux:** `ls /dev/serial/by-id/` lists each adapter with its serial number
  in the name. Such a `/dev/serial/by-id/...` path also works as a fixed
  `endpoint`.

The same FTDI adapter can show a slightly different number on Windows (an
extra letter at the end) than on Linux. Copy the number as the lab PC shows it.
Lookup by serial number is tested with simulated port lists only; it has not
yet been tried with a real adapter (evidence level: documented).

### Network machines

Most analyzers on a lab network connect **to** Analyte: Analyte listens on an
address and port of the lab PC, and the analyzer's host (LIS) settings name
that address and port. A few wait for Analyte to connect to them instead; then
the site file names the analyzer's own address and port. The machine
definition decides which (`tcp_server` when Analyte listens, `tcp_client` when
it connects).

**When the analyzer connects to Analyte**, step 2 of the
[Setup page](operate.md#setup-page) lists this PC's IPv4 addresses with the
network adapter that holds each, such as `192.168.1.20 – Ethernet`, and
**every network card** (`0.0.0.0`). Pick the address on the analyzer's
network, then type that address and the port into the analyzer's host
settings. With every network card, Analyte answers on all of this PC's
addresses, and the analyzer still needs the one on its own network. The list
leaves out addresses no analyzer can reach (loopback, self-assigned
`169.254.x.x`) and adapters that are down; it shows at most 32. An address
saved earlier that the PC no longer has stays in the list, marked as such.

**When Analyte connects to the analyzer**, step 2 asks for the analyzer's IP
address and port, as its own network settings show them.

#### Accept only the analyzer

Analyzer protocols have no password, so any device that reaches a port
Analyte listens on could send results that Analyte would store and send to
the lab app as that machine's. Each listening machine should therefore name
the only devices it accepts, in its site file:

```toml
endpoint = "0.0.0.0:5600"
allowed_peers = ["192.168.1.50"]   # the analyzer's own IP address
```

A connection from any other address is closed at once: Analyte reads
nothing from it, answers nothing (no ACK, no NAK) and stores nothing as that
machine's data ([ADR 0026](../adr/0026-accept-only-the-machine.md)). List 1
to 16 plain addresses; names, ports and ranges such as `192.168.1.0/24` are
refused, because a range lets other devices on the subnet in. The key is
refused for serial, folder and outgoing (`tcp_client`) machines.

The setup steps write it for you. Step 3's connection test shows the address
of the device that connected ("Connected from 192.168.1.50"); step 4 offers
**Only accept this device: 192.168.1.50**, ticked. Check that address
against the analyzer's own network settings before saving. Step 2 lets you
type or change the list. Give each analyzer a fixed address: an address that
changes is refused until it is added.

A machine without the key still accepts any device, as before, and says so:
its page, the Machines list and the Overview show "Accepts connections from
any device" until it is set. Each refused connection shows on the machine's
page with its address and time (the newest five devices since Analyte
started), on the Overview for 24 hours, and in the log. If the refused
address is the analyzer's own, its address changed; otherwise find that
device.

**Keep analyzers on their own network.** An allow-list stops a laptop on the
lab network, but not a device that takes the analyzer's address. Put the
analyzers and the lab PC's analyzer card on their own cable, switch or VLAN,
with no route from office or guest networks. Then a second Analyte or laptop
cannot take results from an analyzer that connects to this PC unless it
takes over this PC's own IP address on that network, which the analyzer
network's owner can watch for.

**When Analyte connects to the analyzer** (`tcp_client`), Analyte cannot
choose who answers. The controls are the analyzer's own "allowed host" (or
"LIS address") setting, set to this PC's address, and the same network
separation. If the connection is lost, the machine shows **Not connected
(retrying)** and the app sends a notification.

#### Firewall

Windows Firewall blocks analyzers that connect to a service and never asks,
because a service has no window. The Windows installer and `install.ps1` add
one inbound rule, **Analyte - analyzers on the lab network**:

| Setting | Value | Why |
| --- | --- | --- |
| Program | `C:\Program Files\Analyte\analyte.exe` | Only Analyte; no other program gets through. |
| Protocol and ports | TCP, any local port | The ports Analyte listens on are set per machine on the setup page; a fixed port list would need a reinstall for each new analyzer. |
| Remote addresses | `LocalSubnet` | Only devices on the same network as one of this PC's cards; `allowed_peers` then narrows each machine to its own analyzer. |
| Profiles | Domain, Private and Public | A cable straight to an analyzer, or a lab switch without a router, usually shows as an "Unidentified network", which Windows treats as Public. A Private-only rule would block the most common case; `LocalSubnet` keeps every other network out on all three. |

Reinstalling or upgrading replaces the rule (it has a fixed name, so there is
always exactly one), and uninstalling removes it. The rule cannot open the
admin pages to the network: Analyte refuses any admin address other than
`127.0.0.1`, and a firewall rule cannot make a socket bound to `127.0.0.1`
reachable from another computer. When Analyte connects to an analyzer, the
connection goes out from this PC, which Windows Firewall allows by default.

**An analyzer on another subnet or VLAN** is outside `LocalSubnet`, so the
rule does not let it in. IT can either put the analyzer on the lab PC's
network, or add a rule of their own for exactly that analyzer, for example:

```powershell
New-NetFirewallRule -DisplayName 'Analyte - analyzer 10.20.30.40' -Direction Inbound `
  -Action Allow -Protocol TCP -LocalPort 5600 -RemoteAddress 10.20.30.40 `
  -Program 'C:\Program Files\Analyte\analyte.exe'
```

Routers or firewalls between the two networks must allow that TCP port too.
Where Group Policy manages Windows Firewall and ignores local rules, IT must
add the same rule in the policy.

**Linux** packages change no firewall. Where firewalld or ufw is active, open
each port Analyte listens on, to the analyzer's network only, for example
`sudo firewall-cmd --permanent --add-rich-rule='rule family=ipv4 source address=192.168.1.0/24 port port=5600 protocol=tcp accept'`
then `sudo firewall-cmd --reload`, or
`sudo ufw allow from 192.168.1.0/24 to any port 5600 proto tcp`.

If step 3's test hears nothing, its message says where Analyte listened, what
to set on the analyzer, and that a firewall may block it. The rule has been
checked on a clean Windows runner in CI; it has not yet been tried with a real
analyzer on a lab network (evidence level: documented).

Validate the **program file**, not a machine definition alone:

```text
analyte check-config analyte.toml
```

It prints each machine's ID, effective config digest and evidence level.
Record these. A pass does not check credentials, reach the app or prove mappings.
Restart after ordinary config changes; there is no operator hot-reload command.
Reprocessing reads current files separately from running receive sessions.

## Credentials

Destination keys come from the process environment or from the locked
credentials file, which the [setup page](operate.md#setup-page) writes
(or `analyte lab-apps keys`, [from the command line](#from-the-command-line)). An
environment variable always wins; a name without one is read from the file.

The locked file is `<data_directory>/credentials/credentials.toml`. The service
writes it itself (a staging file, protected, flushed, then renamed). On Windows
its folder grants only SYSTEM and the account the service runs as, not even
Administrators; on Linux the folder is mode 0700 and the file mode 0600, owned
by the service user. Analyte refuses to start if anyone else can read either.
Because only the service can read it, a foreground `analyte run` under another
account cannot use the file; give that run environment variables instead. The
database backup does not include the file; keep the keys in the protected
recovery set.

A credential name becomes
`ANALYTE_SECRET_` plus uppercase letters/digits, with other characters replaced
by underscores. The example names read `ANALYTE_SECRET_DEMO_API` and
`ANALYTE_SECRET_DEMO_SIGNING`. Choose names that do not collide after conversion.
The signing secret uses Standard Webhooks `whsec_` encoding.

Never put values in TOML, command arguments, recordings or Git. On Windows,
IT must provision a protected service environment; the installer provides no
secret manager or provisioning command. A terminal variable does not configure
the SCM service. On Linux the supplied unit reads `/etc/analyte/environment`;
keep it root-owned, mode 0600, in systemd EnvironmentFile syntax.
Give recovery operators the required credentials through protected host
provisioning too. Restart after changes and check under the service identity.
The admin token and retention digest key are separate credentials; see
[Operate](operate.md) and [Recovery](recovery.md).

## Connect the lab app

Have the app team implement and test [contract v1](../contract/v1.md) and its
test vectors. The reference receiver is an implementation example, not a
production LIS. Agree the base URL, destination namespace, API key, signing
secret and exact test/code-system mappings with that team.

Analyte makes outbound verified HTTPS requests: `POST /results`,
`POST /order-events` and `GET /orders` relative to the base URL. Requests use a
bearer API key; bodies also carry Standard Webhooks signatures. Synchronize
clocks. There is no production HTTP bypass. Do not expose analyzer or admin
ports to the internet.

The app must store delivery ID and effects before answering accepted, and answer
repeats as duplicate. Retries keep the same ID and bytes. Authentication failure
pauses delivery; conflicts and rejected content are parked. Fix the cause before
using an audited resend. Orders need a complete paged snapshot and changes feed.
Stale or incomplete lookup is an error or silence, never a guessed empty list.
A lab app's `freshness_seconds` defaults to 30; agree the offline policy with the lab.
Its `request_timeout_seconds` (default 10, 1 through 120; "Timing (advanced)" on
the Lab apps page) bounds each
request to the app, including TLS and the app storing; raise it for a slow link
or app, but keep it well below the freshness setting.

Prove synthetic delivery, duplicate handling, authentication failure, outage
recovery and order freshness with the app team before clinical connection.

## Add a machine

Follow [Adding a machine](../adding-a-machine.md) and the
[machine inventory rules](../../machines/README.md). Results, queries and
send-ahead orders need separate evidence. Do not enable orders just because
results work. Never infer identity from position or invent units or run keys.

All six first machines are blocked. No activatable `machine.toml` is shipped
for them. Obtain every fact in the corresponding README, not just the summary:

| Machine | Blocking worksheet |
| --- | --- |
| Roche cobas e 411 | [Dialect, packed frames, serial settings and host codes](../../machines/roche/cobas-e411/README.md) |
| YHLO iFlash 1200 | [1200 protocol applicability, status/QC mapping and deadlines](../../machines/yhlo/iflash-1200/README.md) |
| Boule Swelab Lumi | [Installed protocol, framing, encoding and sample field](../../machines/boule/swelab-lumi/README.md) |
| BioCARE Biolyte 2000 | [Flow control, codes and identity evidence](../../machines/biocare/biolyte-2000/README.md) |
| bioMérieux VIDAS | [PC version, ID length and identity evidence](../../machines/biomerieux/vidas/README.md) |
| Dialab Autolyser | [Installed interface, layout and identity evidence](../../machines/dialab/autolyser/README.md) |

Capture de-identified conversations with direction and timing. Keep originals
protected outside Git. A maintainer must review config and replay evidence;
the lab must check values, units, flags, corrections and sample identity through
the report. Only exercised real-machine capabilities can become **proven**.

Current VIDAS and Autolyser result mapping leaves result identity unresolved,
so those results stay parked rather than reaching the app. A successful serial
exchange does not remove this blocker. iFlash also needs evidence-backed code
support for its status semantics and OBR-only QC arrays; config cannot supply
that behavior today.

Machine definitions use `schema_version = 2`: set `protocol` and
`transport.kind`, and the loader derives framing, dialog and syntax, so those
sections carry no `kind` key. Any other machine schema version is refused as
unsupported.

## From the command line

Everything the [Setup page](operate.md#setup-page) and the Lab apps page do
can also be done from PowerShell (or a Linux shell) with `analyte machines`
and `analyte lab-apps` ([ADR 0025](../adr/0025-setup-from-the-command-line.md)).
They use the pages' own checks, dated backups, safe write and roll back, and
write the same audit row, naming the `--actor` you give. Run them in an
administrator PowerShell; without `--config` they use the installed program
file ([Commands](commands.md#before-you-start) says where they look):

```powershell
analyte machines list          # settings, and the live state if the engine runs
analyte machines models        # models, ready or not, how each was checked
analyte machines ports         # serial ports with adapter and USB serial number
```

Add a network machine (for a model where the machine connects to Analyte,
`--address` is this PC's address), check it with `--dry-run` first, then
save. `$model` is a key that `machines models` lists as Ready:

```powershell
$model = 'vendor/model'
analyte --actor it-alex machines add hl7-1
analyte --actor it-alex machines set hl7-1 `
    --model $model --address 192.168.1.20 --port 5600 --lab-app main-lab --dry-run
analyte --actor it-alex machines set hl7-1 `
    --model $model --address 192.168.1.20 --port 5600 --lab-app main-lab `
    --allowed-peer 192.168.1.50
```

`--allowed-peer` is the analyzer's own address: only that device may connect
([Accept only the analyzer](#accept-only-the-analyzer)). Repeat it for
several; `--any-device` removes the list again. Other choices left out keep
the saved devices. `machines list` shows each machine's devices, or a warning
that it accepts any device, and the newest refused connection; `analyte
doctor` warns for a listening machine without a list.

Add a serial machine on a USB adapter, found by its serial number in any USB
socket (`--serial-port COM3` names a fixed port instead):

```powershell
$model = 'vendor/serial-model'
analyte --actor it-alex machines add cobas-1
analyte --actor it-alex machines set cobas-1 `
    --model $model --usb-serial A10K3XYZ --lab-app main-lab
```

Test the connection: the listen-only test of step 3, at most 60 seconds and
64 KiB, nothing sent and nothing stored. It prints the page's verdict; the
exit code is 0 when it saw the model's own format and 4 when it did not.
`--show-bytes` also prints the bytes, which may contain patient data: never
save or share that output.

```powershell
analyte machines test cobas-1 --seconds 30
```

For a machine that connects to Analyte, the test also names the device that
connected. At a terminal it then asks whether to accept only that device from
now on, and saves that through the same checks as `machines set`; in a script
it prints the `machines set ... --allowed-peer` command instead.

Add a lab app and set its keys. Keys are never options: the command asks for
them without showing what you type, or reads them from standard input, the
API key on the first line and the signing key on the second (an empty line
keeps the stored key). The locked keys file can only be written as the
account the service runs as, so while the engine runs, `lab-apps add` and
`lab-apps keys` hand the lab app and its keys to the engine, which stores them
as the Lab apps page does ([ADR 0028](../adr/0028-lab-app-keys-through-the-engine.md)).
They are sent only after the engine has proved it holds the admin key. When
the engine is stopped and the command cannot write the file as you, it says
so and nothing is stored: run `Start-Service Analyte` and the command again.

```powershell
analyte --actor it-alex lab-apps add main-lab --address https://lab.example.org
analyte --actor it-alex lab-apps set main-lab --request-timeout-seconds 20
analyte --actor it-alex lab-apps keys main-lab
analyte lab-apps list
```

In a terminal, a missing choice is asked for step by step, then a summary and
`Save? [y/N]`. In a script, or with piped input, nothing is asked: a missing
choice is refused with the page's reason. A refusal prints `Fix this: …` and
exits with 1 and nothing changed; 3 means the change was written but its
audit row or its keys were not. Remove a machine with
`machines remove <name>`; while work is still unfinished it lists it and
refuses unless you add `--anyway`.

Changes apply after a restart, as on the page:

```powershell
Restart-Service Analyte
```
