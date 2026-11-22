# Machine configuration, schema 1

Evidence: documented. The examples are synthetic teaching definitions, not
qualified machine drivers or evidence of standards conformance.

Call `Draft::load((path, text), (path, text), (path, text))`, then `validate()`.
The caller bounds file reads to `MAX_FILE_BYTES + 1` before supplying text.
The loader also rejects oversized strings before parsing. Validation resolves
bounded local departure recording references through the existing ADR 0009
reader; it never fetches a remote manual or recording.

The shipped definition owns protocol choices, mappings, vendor departures and
evidence. Site TOML owns the port choice and the lab app: either its own
`[destination]` table (the older form) or `lab_app = "<name>"`, a lab app from
`LabApps` (`lab-apps.toml`, checked by the same `[destination]` rules, each name
once, at most 128). A site file with neither uses the only lab app; several are
refused with `LabAppProblem::Choose`, an unknown name with
`LabAppProblem::Unknown`, both forms with `LabAppProblem::Both`. The effective
snapshot always holds the resolved settings under `destination` and never
`lab_app`, so the same values give the same canonical bytes and digest from
either form (`Effective::lab_app_source` says which). The port choice is
exactly one of `endpoint` (port name, socket address or folder) or `usb_serial`
(a USB adapter serial number, serial links only); both or neither is refused,
and loading turns them into one `Endpoint` value. An override's `endpoint` or
`usb_serial` replaces the site's choice. A `tcp_server` site may add
`allowed_peers` (1 to 16 plain IP addresses, each once; no name, port, range,
wildcard, multicast or broadcast address) as `AllowedPeers`; a connection from
any other address is closed unread and unanswered
([ADR 0026](../../../docs/adr/0026-accept-only-the-machine.md)). Other links refuse
the key, and overrides cannot set it. Local overrides contain
`schema_version = 1` plus optional replacement sections. Sections and lists
replace in full; there is no implicit array merging or expression language.
All three layers reject unknown keys. Overrides cannot change the machine-wide
evidence claim. Replacing departure declarations requires complete per-rule
evidence and matching selected settings.

`examples/astm.toml` and `examples/hl7.toml` use `examples/site.toml` and an empty
local layer (`schema_version = 1`). The serial example may replace the synthetic
site endpoint with `COM9`, `/dev/ttyUSB0` or `usb_serial = "A10K3XYZ"`. TCP endpoints require numeric socket
addresses, including an explicit bind address for servers.

Resource ceilings: 1 MiB per input layer; 4096 entries per mapping table;
12 departures, 16 references per departure and 16384 bytes per declaration
snapshot; 128 capabilities and 128 references per capability; 16 MiB messages;
65536 records; 1024 fields/repetitions/components; 300000 ms deadlines.
Individual identifiers use the model crate's bounded text types.

Field selectors are one-based and relative to their declared group. Missing
identity evidence explicitly leaves reconciliation unresolved; no identity is
invented. Unit tables translate exact spelling only and never change numbers.
QC equality rules carry classification evidence, not acceptance decisions.

There is no test table ([ADR 0029](../../../docs/adr/0029-lab-app-owns-test-names.md)):
the lab app owns test names and lists. `[mappings] test_system` (and
`[codes] test_system` for vendor formats) is required and names the machine's
own code system: the namespace of every result code whose message names none,
and the only namespace order tests may use. ASTM also requires
`test_component = "universal" | "manufacturer"` (R.3 component 1 or 4, LIS02
§5.6.1); other stacks refuse it. Optional `value_kinds` entries
(`{code, system?, kind}`) say which kind of value a machine code carries;
`kind` is `numeric` or `coded`, entries are unique per system and code, and
an entry without `system` applies to `test_system`.

After validation, `Effective::stack()` is the single runtime protocol selection.
`Stack` has `Astm`, `Hl7`, `Law`, `Poct`, `Vidas`, `Autolyser`,
ASTM and HL7 variants with a `RecordTransport::Folder` selection. Each carries only its supported transport,
framing/dialog/syntax settings and required mapping tables or vendor policy.
Serial transport carries installed serial settings; TCP-only stacks cannot select
serial or file transport. Folder stacks cannot enter a stream dialog.

The public validated `Machine` contains identity, deviations and evidence;
protocol sections are accessed through `Stack`, not optional sibling fields.
`test_system()` and `units()` return that stack's code system and unit table
without an absent-table fallback. Optional order capabilities remain legitimate: omission disables the
capability; an enabled capability carries its required layout and namespace.
HL7 processing IDs, fixed priority codes and VIDAS sample limits are typed.
Custom ASTM priority spelling remains configurable.

Protocol TOML layout is unchanged. ADR 0015 explicitly replaces the contents of
nonempty `deviations` lists with typed, scoped declarations. Old free-text
name/key/reason entries are rejected with a migration error, never converted to
evidence. See `machines/README.md` for the declaration vocabulary. Its document type is private to loading
and validation. Original and overridden documents are checked before building the
runtime stack, so an override cannot conceal an invalid definition. The resolved
TOML value is retained only for canonical serialization/provenance, never for
runtime protocol dispatch. Runtime objects cannot be deserialized or activated
by constructing a stack directly. Example tests cover every file (order fragments
with their documented bases), both file syntax variants and transport overrides;
private document tests compare every example's canonical bytes with the legacy
layout.

Validated snapshots expose immutable settings, canonical TOML and its SHA-256.
Schema 1 canonicalization serializes a sorted TOML value tree with the pinned
TOML serializer, UTF-8 and LF endings; list order is significant. Persist the
returned bytes, rather than reconstructing them with a different serializer.
Only a validated snapshot can be activated. Hosts replacing a live snapshot must switch between conversations and persist
it beside receipts. The Analyte service currently loads snapshots at startup;
applying changed files requires a restart, with no operator hot-reload command.
Activation itself performs no I/O, changes no queued delivery and sends no order.

Evidence references are declarations checked for structure, not verified files.
The loader cannot certify recordings or raise evidence. Hosts must apply the
selected HL7 edition to message headers, the ASTM text decoder and the declared
mapping/deviation rules. Protocol syntax success alone is not clinical acceptance.
Credential references must resolve through host-owned protected storage.

`mappings.identity` requires `order_id` and `order_namespace`. Namespace is
exactly `{fixed="system"}` or `{field=<selector>}`. `sub_id` is a selector or the
literal `"none"`; only that declaration deliberately supplies an empty sub-ID.
`run` is an ordered selector list, empty when no run evidence exists.
`correction_status_codes` is an explicit list of source correction codes, not
revision numbers. The former retransmission/correction/rerun lists are rejected
as unknown keys. Missing required order settings have a positioned configuration
error; selected evidence missing from a message remains unresolved in mapping.


Vendor receive examples are `examples/poct.toml`, `examples/vidas_literal.toml`
and `examples/autolyser.toml`. They require explicit installed policies and use
`[codes]` (the machine's `test_system` and exact unit translations) instead of
field-position `[mappings]`.
Incompatible stacks and vendor field mappings have distinct typed errors.
All three examples remain synthetic and documented. Outbound permissions and
patient-namespace rules are owned by [program orders](../../analyte/ORDERS.md);
wire limits belong to the [VIDAS](../../protocols/vidas/README.md) and
[Autolyser](../../protocols/autolyser/README.md) protocol guides.

Vendor order tables omit `[orders.astm]`, which remains required for ASTM.
Vendor tables cannot enable ASTM queries or carry ASTM destinations. Send-ahead
also requires the vendor's explicit `download = true` permission. A clone
setting on another stack, or a patient namespace on the current ASTM layout
(which has no patient field), is rejected with the setting's qualified key.

VIDAS order tests are sent as written when their namespace is the machine's
`test_system`; a code that is not 1-4 printable bytes without a pipe is
rejected when the order is mapped. The stack has no specimen-type tag; supplied specimen types are rejected
rather than misusing the origin or specimen-start tags. App patient IDs must
match `patient_namespace` exactly before their values enter the vendor field.

Autolyser `[orders] specimen_types` maps exact app namespace/value pairs to
`S` (serum) or `U` (urine); absent tables map no values. Duplicate pairs, unknown
medium codes and empty supplied tables are rejected. An Autolyser order code
must be exactly four printable bytes as the lab app wrote it (Analyte never
pads it), or the order is rejected; list mappings use only `T` or `R`.
Missing patient, specimen medium or cup position in an app order produces a
specific rejection before reservation or transmission. Cup zero is never inferred.

ASTM optional position/specimen context is documented in
[program orders](../../analyte/ORDERS.md#astm-optional-order-context-lis02-section-8).


HL7 legacy order queries opt in through `[orders.query]` and `[orders.hl7]`;
see [the synthetic layout](examples/hl7_orders.toml). The specimen selector,
exact trailing components, other supported QRD filters, DSP line sources and
application-ACK deadline are all explicit and validated. Unsupported filters
never become empty work. HL7 requires the error policy for untrusted lookups;
this interface does not enable ORM/OML send-ahead or cancellations.

Optional `[mappings.component_parts]` decomposes the ASTM R.3 component that
`test_component` names (LIS02 5.6.1). For example:

```toml
[mappings.component_parts]
separator = "/"
count = 2
code = 1
dilution = 2
```

Positions are one-based, bounded to 1024. The separator is one character and
cannot be a protocol delimiter or control character. The exact part count is
required; empty parts count. Missing/extra parts park the receipt with a located
finding. The code part is then the test code. The dilution part is optional and is retained
exactly, including empty text; only an exact decimal supplies a factor. Ratios
are never interpreted. Application state remains Unknown unless optional
`applied = { field = <selector>, yes = "Y", no = "N" }` matches a source field
exactly. Other codes stay Unknown. Yes/no must differ; applied requires dilution.

A `mappings.value_kinds` entry requires a numeric or coded value for its code;
a code without an entry is not checked. Numeric requires a parsed number;
coded accepts nonempty qualitative text or a coded value, not a plain number.
Mismatches retain the original value and park with `ValueKindMismatch`. No
clinical decisions or numeric conversions are performed.

`[orders]` has no test table. `[orders.astm] test` names the component each
order code goes in, and optional `test_components` (at least that component,
at most the syntax limit) keeps empty components after it.


HL7 unsolicited orders additionally require `[orders.hl7_send]`, `send_ahead`
and `placer_namespace`; see [the teaching interface](examples/hl7_send.toml).
The configured edition selects ORM/O01 or OML/O21. Header applications,
processing, specimen placement and response deadline are explicit. OBR-4 is
the order's code, the lab app's optional name and `mappings.test_system`
(plain, at most 20 bytes); CWE editions require `test_version` (at most 10
bytes) and CE editions refuse it. Position output is available only in ORM's
other placer-owned OBR field; an OML position mapping is refused.

## LAW configuration (results, queries and work)

`examples/law.toml` selects `dialog.kind = "law_manager"`, MLLP and HL7
2.5.1. `[law].connection_mode = "persistent"` explicitly selects one link for
all three transactions. `[transport].kind` selects `tcp_server` (the analyzer
opens it) or `tcp_client` (Analyte opens it); the site endpoint is respectively
the local bind address or the analyzer address. LAW requires UTF-8, CR segments,
strict escapes and store-and-forward
responsibility. `[law.peer]` explicitly declares every enhanced-content flag;
`LawPeer::capabilities` supplies the existing pure LAW builders. No option is
implicitly enabled. Local overrides may replace the whole `[law]` policy.

`law.query_forms` declares `container`, `all_pending`, or both, once each.
Container queries use the existing SampleId lookup; all-pending is scoped to
this analyzer. Append `examples/law_orders.toml` to the LAW definition to enable
queries and their LAB-28 followups. It declares `[law.work]` headers, explicit
sample/specimen bindings and shared `[orders]` specimen and priority
translations; OBR-4 carries the order's code, the lab app's required name and
`mappings.test_system` (plain, at most 12 bytes). Without that layout,
query execution remains disabled. Isolate/parent and rack/tray-position queries
receive QAK AR (unsupported query), never no-work.
Isolate/parent needs app-supplied identities in a future contract version.
Query and response deadlines must be 1 through 60000 milliseconds. LAW permits
only explicit C correction status and cannot select legacy display queries or
non-LAW order layouts.

Program behavior and current qualification gates are maintained in the
[LAW checklist](../../../docs/protocols/law/README.md).

LAW identity selects OBR-2 AWOS (OBR-2.2 namespace or an explicit fixed fallback)
and the complete OBX-4 OG. Config cannot replace these with arbitrary selectors
or additional run selectors. A source EI universal authority is retained with
its type; no authority is guessed. When AWOS is null, an explicit OBR-3 analyzer
order with its own assigning authority can identify unsolicited work; insufficient
evidence remains unresolved and parked. The configured sample selector still
owns sample identity; the example selects SAC-3.1 and preserves case/zeroes.

LAW work preparation and writes use `dialog.send_timeout_ms`; `law.response_ms`
starts after issuance for both work and cancel ORLs. Partial input still uses
`dialog.receive_timeout_ms`, and queries retain their first-byte budget. Restore
pause and stale/untrusted feed admission use the shared store checks. A failed
idle lookup produces no work bytes and retains the receiving connection. Tests:
`work_requires_bindings_and_allows_send_ahead` in this
crate, and the program's source-named `tests/law/work.rs` matrix. The topology choices and validation are tested by
`topologies_require_direction_and_outbound_endpoint`.
Persistent mode uses the site TCP client/server endpoint for all transactions.
Independent mode requires an inbound `tcp_server` site endpoint plus an explicit
`law.outbound_endpoint` analyzer listener for LAB-28. The host owns bounded queued
follow-ups and opens a separate outgoing connection per transaction. A query peer
may close after RSP; work ownership survives that closure. Failed attempts are
uncertain and never automatically resent. See the program's B.6 tests in
`docs/protocols/law/README.md`; evidence remains documented.

The required top-level `protocol` key selects `astm`, `hl7`, `law`, `poct`,
`vidas` or `autolyser`. `[transport].kind` independently selects serial,
TCP client/server or watched-folder pickup. ASTM and HL7 support complete-file
pickup with `framing.kind = "none"` and `dialog.kind = "file_pickup"`.
LAW and POCT require TCP; VIDAS and Autolyser require serial or TCP.
Incompatible protocol, framing, dialog and transport selections fail loading.

## Site retention

Every site layer includes `retention = 90` (recommended), a period from 1 to 3650
days, or `retention = "keep"`. Missing retention is a specific validation failure;
there is no default or override-layer fallback. Finite retention requires the
host `retention-digest` credential. See the program guide for offline conversion,
holds, backup rotation and key continuity (ADR 0013 items 1, 6 and 7).

## Site time zone

`time_zone` is optional: an IANA name that the time-zone database knows, or a
fixed `+HH:MM`/`-HH:MM` offset (hours 00–23). Anything else fails loading.
`SiteTimeZone::offset_at` gives the offset of a local time and returns none in a
daylight-saving gap or fold. Only a received IHE LAW MSH-7 without an offset
uses it, as a noted tolerance; absent keeps refusing such headers.

The Rust stack variant is `Stack::Vidas`, matching protocol key `vidas`.
Named departures retain the TOML field `deviations` for compatibility, as
specified by [ADR 0015](../../../docs/adr/0015-named-machine-departures.md).
