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
| Site file | `schema_version = 1`, endpoint, required retention period, destination with credential names and optional `time_zone`. |
| Optional local overrides | `schema_version = 1` and explicit replacement sections for this installation. |

Overrides replace whole sections and lists. They do not merge list entries or
change evidence. Retention belongs in the site file. Unknown keys fail loading.
`time_zone` (an IANA name such as `"Europe/Berlin"`, or a fixed offset such as
`"+01:00"`) is optional. It is used only to read an IHE LAW MSH-7 that arrives
without an offset, with a visible note; leave it out to keep refusing such
headers.
Keep all installation files outside the repository. The
[config examples](../../crates/core/config/examples/) teach syntax only.

A synthetic site file looks like this:

```toml
schema_version = 1
retention = 90
endpoint = "COM9"
[destination]
name = "demo"
base_url = "https://lab.example.invalid"
api_key_name = "demo-api"
signing_key_name = "demo-signing"
```

Validate the **program file**, not a machine definition alone:

```text
analyte check-config analyte.toml
```

It prints each machine's ID, effective config digest and evidence level.
Record these. A pass does not check credentials, reach the app or prove mappings.
Restart after ordinary config changes; there is no operator hot-reload command.
Reprocessing reads current files separately from running receive sessions.

## Credentials

Destination secrets come from the process environment. A credential name becomes
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
`destination.freshness_seconds` defaults to 30; agree the offline policy with the lab.
`destination.request_timeout_seconds` (default 10, 1 through 120) bounds each
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
