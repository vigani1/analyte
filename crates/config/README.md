# Machine configuration, schema 1

Evidence: documented. The examples are synthetic teaching definitions, not
qualified machine drivers or evidence of standards conformance.

Call `Draft::load((path, text), (path, text), (path, text))`, then `validate()`.
The caller bounds file reads to `MAX_FILE_BYTES + 1` before supplying text.
The loader also rejects oversized strings before parsing. It performs no I/O.

The shipped definition owns protocol choices, mappings, vendor departures and
evidence. Site TOML owns `endpoint` and `destination`. Local overrides contain
`schema_version = 1` plus optional replacement sections. Sections and lists
replace in full; there is no implicit array merging or expression language.
All three layers reject unknown keys. Overrides cannot change evidence.

`examples/astm.toml` and `examples/hl7.toml` use `examples/site.toml` and an empty
local layer (`schema_version = 1`). The serial example may replace the synthetic
site endpoint with `COM9` or `/dev/ttyUSB0`. TCP endpoints require numeric socket
addresses, including an explicit bind address for servers.

Resource ceilings: 1 MiB per input layer; 4096 entries per mapping table;
128 deviations/capabilities and 128 references per capability; 16 MiB messages;
65536 records; 1024 fields/repetitions/components; 300000 ms deadlines.
Individual identifiers use the model crate's bounded text types.

Field selectors are one-based and relative to their declared group. Missing
identity evidence explicitly leaves reconciliation unresolved; no identity is
invented. Unit tables translate exact spelling only and never change numbers.
QC equality rules carry classification evidence, not acceptance decisions.

After validation, `Effective::stack()` is the single runtime protocol selection.
`Stack` has `Astm`, `Hl7`, `Law`, `Poct`, `VidasLiteral`, `Autolyser`,
`FileAstm` and `FileHl7` variants. Each carries only its supported transport,
framing/dialog/syntax settings and required mapping tables or vendor policy.
Serial transport carries installed serial settings; TCP-only stacks cannot select
serial or file transport. Folder stacks cannot enter a stream dialog.

The public validated `Machine` contains identity, deviations and evidence;
protocol sections are accessed through `Stack`, not optional sibling fields.
`tests()` and `units()` return that stack's required tables without an absent-table
fallback. Optional order capabilities remain legitimate: omission disables the
capability; an enabled capability carries its required layout and namespace.
HL7 processing IDs, fixed priority codes and VIDAS sample limits are typed.
Custom ASTM priority spelling remains configurable.

The existing TOML layout is unchanged. Its document type is private to loading
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
Only a validated snapshot can be activated. The host must enforce the
between-conversations switch and persist the snapshot beside receipts.
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
`[codes]` for exact test/unit translations instead of field-position `[mappings]`.
Incompatible stacks and vendor field mappings have distinct typed errors.
All three examples remain synthetic and documented. Outbound permissions and
patient-namespace rules are owned by [program orders](../analyte/ORDERS.md);
wire limits belong to the [VIDAS](../vidas/README.md) and
[Autolyser](../autolyser/README.md) protocol guides.

Vendor order tables omit `[orders.astm]`, which remains required for ASTM.
Vendor tables cannot enable ASTM queries or carry ASTM destinations. Send-ahead
also requires the vendor's explicit `download = true` permission. A clone
setting on another stack, or a patient namespace on the current ASTM layout
(which has no patient field), is rejected with the setting's qualified key.

VIDAS outbound test mappings require one printable 1-4 byte assay code, without
pipes. The stack has no specimen-type tag; supplied specimen types are rejected
rather than misusing the origin or specimen-start tags. App patient IDs must
match `patient_namespace` exactly before their values enter the vendor field.

Autolyser `[orders] specimen_types` maps exact app namespace/value pairs to
`S` (serum) or `U` (urine); absent tables map no values. Duplicate pairs, unknown
medium codes and empty supplied tables are rejected. Autolyser analysis codes
must occupy exactly four printable bytes; list mappings use only `T` or `R`.
Missing patient, specimen medium or cup position in an app order produces a
specific rejection before reservation or transmission. Cup zero is never inferred.

ASTM optional position/specimen context is documented in
[program orders](../analyte/ORDERS.md#astm-optional-order-context-lis02-section-8).


HL7 legacy order queries opt in through `[orders.query]` and `[orders.hl7]`;
see [the synthetic layout](examples/hl7_orders.toml). The specimen selector,
exact trailing components, other supported QRD filters, DSP line sources and
application-ACK deadline are all explicit and validated. Unsupported filters
never become empty work. HL7 requires the error policy for untrusted lookups;
this interface does not enable ORM/OML send-ahead or cancellations.

Optional `[mappings.component_parts]` decomposes one component of ASTM R.3
(LIS02 5.6.1). For example:

```toml
[mappings.component_parts]
component = 4
separator = "/"
count = 2
code = 1
dilution = 2
```

Positions are one-based, bounded to 1024. The separator is one character and
cannot be a protocol delimiter or control character. The exact part count is
required; empty parts count. Missing/extra parts park the receipt with a located
finding. This selection replaces the normal universal/manufacturer code lookup.
Omission preserves that lookup. The dilution part is optional and is retained
exactly, including empty text; only an exact decimal supplies a factor. Ratios
are never interpreted. Application state remains Unknown unless optional
`applied = { field = <selector>, yes = "Y", no = "N" }` matches a source field
exactly. Other codes stay Unknown. Yes/no must differ; applied requires dilution.

Each `mappings.tests` entry optionally declares `value_kind = "numeric"`,
`"coded"` or `"either"`. Omission defaults to `either`, preserving current
ASTM/HL7 behavior. Numeric requires a parsed number; coded accepts nonempty
qualitative text or a coded value, not a plain number. Mismatches retain the
original value and park with `ValueKindMismatch`. No clinical decisions or
numeric conversions are performed.


HL7 unsolicited orders additionally require `[orders.hl7_send]`, `send_ahead`
and `placer_namespace`; see [the teaching interface](examples/hl7_send.toml).
The configured edition selects ORM/O01 or OML/O21. Header applications,
processing, specimen placement and response deadline are explicit. Test
components are code/text/system. Position output is available only in ORM's
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
sample/specimen bindings and shared `[orders]` translations. Without that layout,
query execution remains disabled. Isolate/parent and rack/tray-position queries
receive QAK AR (unsupported query), never no-work.
Isolate/parent needs app-supplied identities in a future contract version.
Query and response deadlines must be 1 through 60000 milliseconds. LAW permits
only explicit C correction status and cannot select legacy display queries or
non-LAW order layouts.

Program behavior and current qualification gates are maintained in the
[LAW checklist](../../docs/protocols/ihe-law.md).

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
`law_tf2b_3_27_3_28_work_requires_explicit_bindings_and_allows_send_ahead` in this
crate, and the program's source-named `tests/law/work.rs` matrix. The topology choices and validation are tested by
`law_tf2b_b6_declared_topologies_require_explicit_direction_and_outbound_endpoint`.
Persistent mode uses the site TCP client/server endpoint for all transactions.
Independent mode requires an inbound `tcp_server` site endpoint plus an explicit
`law.outbound_endpoint` analyzer listener for LAB-28. The host owns bounded queued
follow-ups and opens a separate outgoing connection per transaction. A query peer
may close after RSP; work ownership survives that closure. Failed attempts are
uncertain and never automatically resent. See the program's B.6 tests in
`docs/protocols/ihe-law.md`; evidence remains documented.
