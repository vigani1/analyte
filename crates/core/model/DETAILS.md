# Common model

Pure data and lifecycle logic. The host generates receipt/delivery UUIDv7 IDs
once and supplies them here for validation; this crate never reads a clock,
performs I/O, hashes bytes or interprets clinical meaning.

Observation keys contain code/system, sub-ID and namespaced order context.
Analyte assigns numeric revisions independently of source action codes. A
correction can be the first observed revision. Result replacement units contain
one or more source parts, each with ordered repeated values. Unknown value types
retain raw text and require parking. Raw protocol bytes remain in the host store.

Limits are model admission limits, not protocol limits: identifiers and decimals
256 UTF-8 bytes, reasons 1024 bytes, source text 16 KiB, lists 256 entries.
Exceeding these limits is an error: callers must retain the original receipt and
park it, never truncate. Format readers must bound the whole input document before
serde; individual model bounds do not replace transport/document limits.

Unit mappings record explicit configuration choices. The model does not validate
UCUM terminology, convert units, decode attachments, infer locale or calculate
clinical values. Exact decimals retain sign, leading zeros and fractional scale.
Time text is authoritative; optional civil date/time and offset never imply a
missing timezone. The source path preserves hierarchy without recursive trees.

Tests cite plan section 6, ADR 0002/0003 and clinical safety rules 3-8. They use
synthetic data and property tests, without analyzers or lab apps.
