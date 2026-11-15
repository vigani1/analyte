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
