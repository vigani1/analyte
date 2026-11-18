# ASTM decisions

These choices explain Analyte's policy and interpretations. Mandatory wire
rules and their coverage belong only in [checklist.md](checklist.md).

## 1. Resolve the frame-size wording in favor of the total ceiling

- **what**: Subtract overhead when splitting a record into frames; smaller
  vendor limits remain explicit configuration.
- **why**: The later sentence about fitting a message in one frame does not
  subtract overhead. Reading it literally could exceed the total ceiling
  stated immediately before it, so the overhead-inclusive rule wins.
- **source**: LIS01-A2 §6.3.1–6.3.1.2, p.7; §4.4.1, p.2.
- **test**: `lis01_6_3_1_6_3_2_6_3_3_6_6_large_records_rollover_separate_records`.

## 2. Make the checksum span explicit

- **what**: Include ETB or ETX in the raw-byte checksum, before decoding text.
- **why**: The addition sentence does not explicitly name the terminator,
  although the frame layout puts it inside the checked block. An explicit
  span prevents competing readings; decoding first could change checked bytes.
- **source**: LIS01-A2 §6.3.1.2–6.3.3.1, pp.7–8.
- **test**: `lis01_6_3_3_checksum_sums_raw_digit_through_etx`.

## 3. Do not autodetect bare LIS02 over TCP

- **what**: The ASTM TCP stack retains the link dialog; bare LIS02 file input
  is a separate configured transport, not a TCP fallback.
- **why**: LIS01 suggests direct LIS02 over TCP as an alternative. Silently
  selecting that alternative on unexpected bytes would change the acceptance
  contract and could conceal a connection to the wrong endpoint.
- **source**: LIS01-A2 §4.4.2, p.2;
  [ADR 0002](../../adr/0002-config-first.md), Decision.
- **test**: none yet.

## 4. Reject oversized records instead of truncating

- **what**: Enforce configured field and record bounds without shortening data.
- **why**: LIS02 leaves practical limits to the implementations and permits
  agreed truncation. A shortened identifier or result could still look valid,
  so Analyte rejects it and keeps the original evidence instead.
- **source**: LIS02-A2 §§5.2–5.3, pp.5–6;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), values and bounds policy.
- **test**: `lis02_5_2_5_3_limits_reject_instead_of_truncating`.

## 5. Configure whether to honor receiver interrupts

- **what**: Expose honoring an interrupt as a dialog setting.
- **why**: The sender may continue or yield. An explicit setting lets the
  installation choose that behavior without a vendor special case in the
  shared state machine.
- **source**: LIS01-A2 §6.3.5.1–6.3.5.3, p.8.
- **test**: `lis01_6_3_5_interrupt_honor_ignore_and_issue`.

## 6. Keep one outgoing message buffer

- **what**: Refuse a second outgoing message while the dialog owns one.
- **why**: Extra buffering is optional. The host already owns durable order
  scheduling, so a second queue in the link would complicate shutdown and
  uncertain sends without adding a needed capability.
- **source**: LIS01-A2 §6.5.2.6, p.10;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), dialog ownership.
- **test**: `lis01_6_5_2_6_one_message_buffer_and_validated_vendor_values`.

## 7. Refuse unsupported search semantics

- **what**: Serve supported exact-sample order queries; refuse unsupported
  ranges and filters instead of evaluating only their understood parts.
- **why**: LIS02 describes searches without requiring every search service.
  Ignoring a filter could select a different order set and send tests the
  query did not request.
- **source**: LIS02-A2 §§11.3–11.8, pp.25–26.
- **test**: `lis02_section11_cancel_is_distinct_and_unsupported_filters_are_not_empty_orders`.

## 8. Make acknowledgment depend on durable custody

- **what**: Reserve capacity before accepting a transfer and commit each frame,
  with its transfer identity, before its positive data acknowledgment.
  Accepting ENQ only means "ready". A transfer that was acknowledged frame by
  frame but never completed is kept for recovery.
- **why**: A valid checksum proves nothing about crash survival. The analyzer
  must retain responsibility if the host cannot keep the promised evidence.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), ASTM row;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), Store before acknowledging.
- **test**: `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack`.

## 9. Detect conflicting retransmitted frames

- **what**: Compare retransmission identity with retained payload evidence;
  a reused frame number with different content is not a duplicate.
- **why**: The small cyclic frame number handles sequencing, not durable
  content identity. A lost ACK must not let conflicting bytes pass as the
  previously stored frame.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Keep the original
  and Three identities.
- **test**: `adr0003_lis01_6_3_2_6_5_1_commit_duplicate_and_conflict`.

## 10. Isolate nonproduction queries from real orders

- **what**: Refuse real order lookup for training, debug and QC query contexts.
- **why**: A syntactically valid test exchange must not reserve patient work.
  The processing label is therefore a host authorization boundary, not merely
  retained metadata.
- **source**: LIS02-A2 §6.12, p.12;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle policy.
- **test**: `lis02_6_12_p12_nonproduction_queries_never_reserve_or_send`.

## 11. Keep an unknown lookup distinct from an empty lookup

- **what**: A failed lookup or elapsed query budget produces a query error or
  silence, never a successful no-information answer.
- **why**: No-information would tell the analyzer there is no work when
  Analyte merely failed to learn what work exists.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Lookup failure
  is not no orders.
- **test**: `lis02_section11_adr0004_missed_deadline_is_query_error_or_silence_never_no_information`.

## 12. Keep result history separate from transfer sequencing

- **what**: Assign revisions from trusted observation identity and status
  history, independently of frame numbers and transmission IDs.
- **why**: A transfer retry must not create another clinical result, while a
  preliminary-to-final change must not become an unmarked-correction conflict.
  Link sequencing cannot decide either question.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities;
  [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `adr0003_revisions_new_retransmission_correction_conflict_runs_and_restart`.

## Consecutive comments (standards pass 2)

LIS02-A2 sections 10 and 4.1 attach consecutive comments to the first preceding
noncomment record; manufacturer ownership remains immediate. Regression:
`lis02_10_consecutive_comments_share_noncomment_owner` checks parser parents
and delivered result annotations. Previously interpreted receipts containing
consecutive result comments could have omitted later annotations or retained a
comment as their parent. Raw receipts are unchanged. No deployed store was
inspected. Identify affected receipts in the protected store and reprocess the
original bytes with the fixed parser and recorded config. Reprocessing adds a
new interpretation version, preserving the raw receipt and old interpretations;
review prior deliveries for reconciliation rather than overwriting their history.

Contextual C/M levels are tracked independently of P/O/R ancestry. Section
5.6.7 resets the counter when the same type changes level or a lower numbered
level intervenes. Regression: `lis02_4_1_5_6_7_comment_manufacturer_level_changes_reset_sequence`.

Section 5.1 text components reject literal controls before escape expansion;
header-selected delimiters remain syntax. Delimiter escapes and raw bytes are
unchanged. Test: `lis02_5_1_literal_controls_rejected_in_text_components`.

Status evidence now asserts every section 9.9 code against its named meaning
and parsed result view (`lis02_9_9_each_wire_code_has_specified_meaning`).
Manufacturer evidence checks all eight fixture positions, sequence and exact
round-trip (`lis02_14_all_manufacturer_positions_are_retained`); this proves
retention, not vendor field semantics. Evidence remains documented.

## Strict records and semantic selectors (standards pass 3)

LIS01 sections 3.3.2, 6.3.1.1 and 8.3.1.1 require LIS02 records to start
in separate frames. The link rejects packed records with NAK and retains the
rejected frame as evidence. This remains the default. An installed machine may
select only the evidenced packing departure described below; shipped synthetic
machine definitions enable none.

LIS02 sections 5.4.5 and 8.4.3: semantic selectors use decoded delimiter escapes;
unknown, local, hexadecimal and formatting operations cannot become identity.
The original record bytes remain alongside interpretations. H-12 is recognized
as one exact ProcessingId code (section 6.12).

LIS02 section 6.10 describes receiver identification; section 4.1 allows fields
that are not needed for record identification or parsing to be omitted. H-10
therefore remains empty when no peer identity is agreed. Sites may configure
`orders.astm.receiver` to fill it; a local machine name is not a peer identity.

## Evidenced inbound record packing

- **what**: [ADR 0015](../../adr/0015-named-machine-departures.md) permits the
  typed `AstmPackedRecordsInFrame` policy only for the declared inbound result
  scope. Verify original envelopes, checksum, sequence and bounds first. Commit
  custody once for the whole frame before its single ACK; then emit actual
  CR-ended records in wire order. Partial records continue across ETB frames
  within the same bounds. Lost ACK retries do not emit records again. Later
  LIS02 hierarchy/mapping failures park retained data without retracting custody.
- **why**: Frame boundaries and record boundaries are different facts. TCP read
  boundaries supply neither. Packing never supplies a missing record, sample,
  result identity, unit or status. Outbound host traffic stays one record per
  frame even when inbound packing is selected. Bad/missing checksums still NAK.
- **evidence and delivery**: Interpretation ledgers retain enabled declarations
  and actual packed-record applications with record ordinals and source byte
  spans linking fragments to verified frames. An incomplete bounded ledger is
  explicit and prevents delivery. The project lead's 2026-11-16 decision keeps
  framing-only packed results on contract v1 unchanged; the applied departure
  remains in the protected receipt interpretation and admin page. It changes no
  clinical meaning and does not require a contract version in this task.
- **source**: LIS01-A2 §§3.3.2, 6.3.1.1 and 8.3.1.1; custody under ADRs 0003
  and 0010; named scope and evidence under ADR 0015. Manual or qualified ADR
  0009 recording evidence must establish the installed packing mode; synthetic
  tests do not qualify any machine.
- **tests**: [paired strict/packed dialog cases](../../../crates/steps/dialog/tests/packed.rs)
  cover frame custody, checksum failure, retries and ETB continuation.
  [Stored interpretation cases](../../../crates/analyte/src/machine/interpretation/departure_evidence/tests.rs)
  cover v1 delivery without semantic changes and evidence retained after bad
  LIS02 hierarchy. Existing link/storage recovery fixtures remain unchanged.
