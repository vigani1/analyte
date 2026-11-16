# HL7 v2 editions, with MLLP Release 1

Evidence level: documented. Phase 3.

The native profiles below remain 2.3.1 and 2.5.1. Bounded receiving profiles
also admit 2.3, 2.4 and 2.5 through HL7 2.5.1 §2.8 and the 2002-04-08
2.3.1/2.4 errata. They receive the declared ORU/R01 core and legacy query
exchange, preserve the exact edition in replies, and refuse or retain unresolved
extensions. Command sending stays blocked in config for those three editions
until their own writer field definitions are available. The bounded 2.6 receiving/query profile uses its own
DTM/CWE/XCN rules in the shared table, with deprecated and unselected elements
kept raw or refused visibly. The bounded 2.6 O21/O22 writer is qualified with explicit coding revisions;
see the field matrix in versions.md.
The bounded 2.8.2 and 2.9.1 profiles receive ORU/R01 results with the modern
typed rules; legacy queries remain refused by edition and withdrawing section.
The bounded 2.8.2 O21/O22 writer is qualified; 2.9.1 sending remains blocked
on the ORC-4 source conflict documented in versions.md.
See [versions](versions.md) for the precise differences,
source sections, errata items and limitations; this is not full historical-schema
conformance or simulator qualification.

The transaction list below describes the two native profiles. The versions
matrix bounds the receiving extensions for the other six editions.

Draft conformance checklist, written in our own words from the standard and
checked in an independent review on 2026-11-15. Chapter, section and PDF page
numbers point into the standard; nothing here reproduces its text or tables.
The phase starts by confirming this scope (plan principle 7); the checklist
then becomes the test list, one or more tests per row.

Evidence audit, 2026-11-15: checked rows were compared with the actual assertions,
including macro-generated negative cases. Broader O35/O36 family coverage,
TQ2 relationships, continuation identity, RTB/RDY pairs and subscription lifecycles
remain unchecked; rejection tests and narrower selected profiles are partial
evidence. Checked declaration-only rows describe explicit unsupported scope,
not implemented transactions. Named tests below must pass before handoff.

## Scope decisions to confirm at phase start

- **Not all of HL7.** Analyte implements the selected encoding and control subset for both
  native editions, with open obligations and explicit refusals tracked in
  [the checklist](checklist.md), plus a declared list of laboratory transactions. The
  first list: ORU^R01 with ACK (both editions); ORM^O01 and ORR^O02 (both editions);
  non-LAW OML^O21 and ORL^O22 (2.5.1);
  OML^O33 and ORL^O34, OUL^R22 and ACK^R22, QBP^Q11 and RSP^K11 (2.5.1, also
  used by IHE LAW); the legacy QRY^Q02, QCK^Q02 and DSR^Q03 exchange (both editions).
  Everything else is rejected with a specific, safe answer, never silently
  dropped.
- **Who is the receiving application.** Each connection declares that
  Analyte takes on store-and-forward responsibility. It answers AA only for
  messages it durably stored *and* could interpret. Input it stored but could
  not interpret is answered AE or AR as the standard prescribes and parked
  for an operator. Enhanced mode (CA/CE/CR) follows MSH-15/16 exactly.
- **Identity.** An observation is identified by its code and code system,
  its sub-ID and its order context, never by OBX-1, test name or value.
  Several OBX segments with the same key can form one replacement unit.
  OBX-11 is kept as sent; Analyte assigns its own revision numbers, because a
  correction can arrive without the original. OBX-21 is not an identifier in
  either edition (absent in 2.3.1, reserved in 2.5.1).
- **Groups.** Parsing keeps the patient/order/observation/specimen group each
  segment belongs to; field paths are relative to their group.
- **Character sets.** Each connection declares its encoding (ASCII, ISO 8859-1
  or UTF-8); MSH-18 is checked against it. The `UNICODE UTF-8` code is standard
  in 2.5.1 only; accepting it in 2.3.1 requires `syntax.allow_utf8_in_v231 = true`
  and a named deviation with the same key and a reason. Undecodable bytes are kept and
  reported, never replaced.
- **Vendor departures** (repurposed fields, reused control IDs in responses,
  QC arrays in OBR) live in machine configs as named deviations, never in the
  standard code or the standard test suite.


## Notes

- [Design decisions](decisions.md)
- [Conformance checklist and retained evidence](checklist.md)
- [Versions](versions.md)
