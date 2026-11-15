# POCT implementation status

Evidence: documented. This crate is a syntax foundation, not a conformant
POCT01-A2 Observation Reviewer. Do not connect it to an analyzer yet.

The bounded nonrecursive XML reader preserves exact bytes, rejects DTDs and
unknown entities, checks balanced roots and duplicate attributes, and bounds
message size, nesting, element count and attribute size. Decoded content is
available beside the raw bytes. Rejected input remains the host's responsibility
and must be retained in the protected store. Debug output omits document content.

Names use the ASCII DML vocabulary without namespace prefixes. Only UTF-8 XML
1.0 is accepted. Limits are host policy, not values attributed to the standard.

## Checklist coverage

No mandatory Basic Profile row is complete. Tests cover only the secure XML
handling portion of the structure/data-type rows and the malformed-input portion
of the project gate. Test payloads are independently written synthetic fragments,
not standard examples or DTD extracts.

Still required:

- Typed HEL/DST/OBS/EOT/END/ACK/ESC and outbound REQ messages.
- Normative object containment, ordering, cardinality, codes and control IDs.
- POCT coded, quantity, null, timestamp and encapsulated data types.
- Reviewer message builders and structural round-trip tests.
- Startup, capabilities, topic requests, repeated topics and termination.
- Durable-storage commit gate, ordered asynchronous ACKs and backpressure.
- Timeout behavior, reconnect/restart and independent observation deduplication.
- Mapping with explicit patient/nonpatient classification and original values.
- Every optional extension; none is advertised or implemented.

Appendix A physical access and Appendix C HL7 reporting remain outside scope.
