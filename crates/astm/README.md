# analyte-astm

Pure LIS02-A2 record syntax. Evidence level: **documented**; synthetic tests
are based on CLSI LIS02-A2 sections 4–14. No real analyzer or official
simulator has been used.

`parse` takes complete CR-terminated records, including H and L. `parse_with`
accepts a strict text decoder returning `None` for undecodable text. Latin-1
is the default. Parsing never performs I/O, acknowledges data, assigns result
identities, or interprets clinical values.

Messages contain records in wire order. Each record has a parent index into
that message, giving a tree without recursive allocation or traversal. Original
records, fields and components remain available. Field positions are one-based;
an omitted trailing field returns `None`, while a transmitted empty field has
`Presence::Empty`. Exactly two quotes have `Presence::Delete`.

H/P/O/R/C/Q/L/S/M views expose the standard field positions, including reserved
fields. Unknown extensions remain accessible by position. The status enums retain
unknown codes as bytes. These are syntax views, not full semantic validation of
dates, identifiers, codes, search services, passwords or clinical constraints.

Escape policy (section 5.4.5): F/S/R/E decode to the selected delimiter bytes.
All other closed operations, including H/N, X, Z, unknown and malformed bodies,
remain literal in decoded text and raw bytes. Their bodies cannot change field,
repeat or component boundaries. An unclosed operation is an error; escape bodies
are never recursively interpreted. CR always terminates a record.

`serialize` preserves the original spelling exactly. `serialize_with` changes
delimiters and escapes literal content while preserving decoded delimiter values.
It refuses a target escape byte that collides with a preserved optional operation,
or target delimiters that collide with a record identifier. Reparse with the same
decoder; different delimiter selections necessarily change raw bytes.

Default agreed implementation limits (sections 5.2–5.3): 16 MiB per input,
65,536 bytes per record excluding CR, 128 fields per record, 128 repeats per
field and 128 components per repeat. Limits are configurable and exceeded limits
are errors, never truncations. Header delimiter definition is a bootstrap scalar,
not three components or an unfinished escape. Input is already a bounded byte
slice; this crate does not buffer streams.

The caller must retain the input when parsing fails, and park it with the specific
error. This crate does not log payloads. Framing, dialogs, durable storage,
query lifecycle and common-model mapping belong to their respective layers.
