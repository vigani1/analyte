# Autolyser internal host protocol

Evidence level: **documented**, from the Autolyser / Autolyser ISE Operator's
Manual Rev18, chapter 23 (pp.175-187). No machine or official simulator tested.
Tests use our synthetic research fixtures, never copied vendor examples.

`records` parses and builds fixed-width worklists and both result layouts.
Patient codes and values retain spaces, leading zeroes and case. The optional
8+7 constructor appends seven spaces to exactly eight supplied characters;
it does not normalize arbitrary identifiers. Analysis codes occupy four bytes.
Checksum arithmetic sums payload bytes modulo 256 and emits three decimal
digits, excluding initialization, checksum digits and EOT. That span is inferred
and checked against the consistent standard examples in the research notes.

`dialog` owns one serialized connection: STX, ACK, request/data plus EOT,
response. Journal transport input before feeding individual bytes, including unexpected
bytes and input arriving at expired deadlines. Split/merged reads behave alike.
Every complete response (including NAK and insertion replies) is emitted for
durable storage before interpretation. Return its connection-scoped token only
after committing exact bytes with the connection incarnation, driver version
and effective configuration. A checksum failure parks the stored receipt and
blocks R and A until a valid L response resolves recovery. Empty L does not
resolve it. Recovery can be manual or L at the configured interval; A always
requires an explicit caller operation. Normal R polling is caller-driven and
rate-limited. There are no automatic worklist retries.

All timing values and the response byte bound are required configuration.
They are deployment policy, not vendor defaults. Total response and inter-byte
deadlines are separate. Host monotonic time is supplied with each event. On
Close stop feeding this dialog and preserve remaining transport bytes in the
protected host journal; keep consuming pending Store effects. Incomplete and
overlong responses are stored as bounded evidence, then the connection closes.
Storage failure closes visibly. A timeout/disconnect after insertion may have
changed the analyzer: reconcile rather than resubmit. On restart the host must
reconcile the last outstanding poll with L before normal polling; a new Dialog
cannot discover an earlier process's send cursor or journal.

The source does not provide an immutable run identity, timestamps, units or
result-receipt ACK. This crate invents none. Mapping must retain unresolved
result identities and keep controls out of patient delivery. The one-byte
accepted insertion position is preserved without assuming special values.
Do not log byte-bearing actions or parsed records.

## Source inconsistencies and limits

- Page 176 uses differing analysis spellings in the prose and transmitted
  example; it is not a golden fixture here.
- Page 177's diagram appears to put a checksum inside each repeated result;
  its consistent example instead has one checksum after all pairs.
- Page 178 adds six unexplained spaces absent from its layout. Its printed
  checksum is 063; the documented layout gives 167, or 103 with those extra
  spaces. We implement the field layout, not either defective example.
- Page 187's variable-protocol transmit script omits the list field present
  in the internal layout. Variable protocol also uses different list codes.
  It is a different configuration, not an alternate internal golden fixture.

Precise request/EOT timing, cursor persistence and recovery across restarts,
installed software behavior and serial/TCP settings need vendor clarification
and real captures. Variable-protocol scripting/compiler support is a later
option. Transport, storage, mapping, order lifecycle, clinical decisions and
production qualification belong to the host and are deliberately not included.
