# HL7 decisions

The [checklist](checklist.md) owns conformance requirements; the
[version page](versions.md) owns edition support. These entries explain
choices within that scope, not additional promises of conformance.

## 1. Declare laboratory transactions rather than claim all of HL7

- **what**: Implement the laboratory transaction set declared in the scope;
  do not infer support for other transactions from a working segment parser.
- **why**: Parsing a segment does not implement its workflow or response.
  An explicit transaction boundary prevents a machine from being credited
  with support that has only been tested at the grammar level.
- **source**: [ADR 0007](../../adr/0007-standards-first.md), scope definition.
- **test**: none yet.

## 2. Act as the receiving application for store-and-forward

- **what**: Original-mode AA means Analyte durably retained and interpreted
  the message; it does not wait for the destination lab app to receive it.
- **why**: Waiting for an external app would tie the analyzer's response
  deadline to an unrelated network outage. Analyte instead accepts custody
  at its own durable boundary and owns subsequent delivery.
- **source**: HL7 2.5.1 §2.9.2.2, pp.2-26–2-27;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), receiving responsibility.
- **test**: `hl7_251_2_9_2_aa_only_after_store_and_interpret`.

## 3. Separate enhanced custody from semantic acceptance

- **what**: Use raw durable storage for the accept phase and interpretation
  for the application phase; do not treat CA as permission to send AA.
- **why**: Stored but uninterpretable input is recoverable evidence, not a
  successfully processed result. Combining the phases would erase that
  distinction precisely when the sender has already released responsibility.
- **source**: HL7 2.5.1 §2.9.3–2.9.3.3, pp.2-27–2-28;
  [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), HL7 row.
- **test**: `hl7_251_2_9_3_independent_conditions`.

## 4. Agree on encoding instead of guessing it from payload bytes

- **what**: Support configured ASCII, Latin-1 or UTF-8 and check the declared
  character set against the connection setting.
- **why**: Byte sequences can be valid under several encodings with different
  meanings. A successful heuristic decode is insufficient evidence for
  changing a patient's identifier or a textual result.
- **source**: HL7 2.3.1 §2.24.1.18, pp.2-108–2-109;
  HL7 2.5.1 §2.15.9.18, pp.2-78–2-80.
- **test**: `hl7_251_2_15_9_18_charset_is_lossless_and_connection_checked`.

## 5. Require a named departure for legacy UTF-8

- **what**: Accept the later UTF-8 character-set code in 2.3.1 only when the
  explicit compatibility flag has a matching named vendor departure.
- **why**: The older table does not define that specific code. A global
  tolerance would make one vendor's extension indistinguishable from standard
  behavior and hide which installation relies on it.
- **source**: HL7 2.3.1 §2.24.1.18, pp.2-108–2-109;
  HL7 2.5.1 §2.15.9.18, pp.2-79–2-80;
  [ADR 0002](../../adr/0002-config-first.md), machine configuration ownership.
- **test**: `hl7_231_2_24_1_18_utf8_requires_named_vendor_departure`.

## 6. Keep vendor extensions out of the base grammar's meaning

- **what**: Put repurposed fields and other vendor departures in the machine
  definition, rather than changing the common interpretation of a field.
- **why**: HL7 allows local extensions, but their agreement is local. Giving
  one installation's convention a global meaning could silently remap another
  analyzer's standard data.
- **source**: HL7 2.5.1 §2.11–2.11.1, p.2-37;
  [ADR 0002](../../adr/0002-config-first.md), Decision.
- **test**: none yet.

## 7. Bound custody and interpretation separately

- **what**: Give pending storage, interpretation and application-reply waits
  bounded lifetimes, and reject a late completion after its deadline.
- **why**: A disk or interpreter stall must not hold the connection forever.
  A late success also cannot retroactively justify a positive reply after the
  dialog already reported failure.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded waits;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), dialog deadlines.
- **test**: `hl7_251_2_9_deadline_precedes_late_commit`.

## 8. Do not automatically resend an uncertain machine order

- **what**: Keep acceptance and application-response phases distinct; a lost
  response after sending an order leaves an uncertain outcome without resend.
- **why**: HL7 leaves some retry decisions to the application. For a machine
  order, repeating bytes could schedule another test even though the first
  command succeeded, so delivery-style retries are inappropriate.
- **source**: HL7 2.5.1 §2.9.2.2, p.2-26;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle rule.
- **test**: `hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends`.

## 9. Retain malformed input before parking

- **what**: Preserve the raw message even when a usable semantic message or
  successful acknowledgment cannot be constructed.
- **why**: Rejecting a parse must not destroy the bytes needed to diagnose a
  wrong encoding, repair a mapping or demonstrate what the machine sent.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Never drop data
  and Keep the original.
- **test**: `adr_0003_malformed_message_is_committed_before_parking`.

## 10. Assign revisions from trusted identity and history

- **what**: Use the common store's revision policy instead of treating a
  transmission identifier, segment position or changed value as a new result.
- **why**: A correction can arrive before its original, and a normal status
  progression can change content without being a correction. Only the retained
  identity and history can distinguish those cases from a retransmission.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities;
  [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `adr0003_revisions_new_retransmission_correction_conflict_runs_and_restart`.
