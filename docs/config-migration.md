# Machine configuration schema 2

ADR 0002 requires a migration rule for the public configuration interface.
Machine schema 2 implements ADR 0012 §5: select the protocol key, its edition
where applicable (`syntax.edition` for HL7), and `transport.kind`. The loader
derives framing, dialog and syntax. Their sections still hold bounds and policy,
but must not contain `kind`. Site and local override schemas remain version 1.

Schema 1 machine files remain supported. Before validation, the loader checks
`framing.kind`, `dialog.kind` and `syntax.kind` against the stack derived from
`protocol` and `transport.kind`. A missing or disagreeing selector refuses the
file with its qualified key. It then removes those selectors and records machine
schema 2. Equivalent schema 1 and schema 2 inputs produce identical canonical
bytes and digests; previously stored receipt snapshots are never rewritten.

To migrate a file, change its machine `schema_version` to 2 and remove the three
layer `kind` keys. Keep bounds, policies, mappings and evidence unchanged.
Overrides replace settings sections; they cannot select layer kinds. A transport
replacement derives the stack again and must still satisfy its policy checks.
Recording header layer labels remain stable for existing evidence.

Tests: `adr0002_schema1_migrates_to_identical_schema2_canonical_bytes` compares
canonical bytes and rejects disagreement at each selector key.
