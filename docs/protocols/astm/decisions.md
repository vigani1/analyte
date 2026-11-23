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
- **test**: `long_records_split_and_numbers_roll_over`.

## 2. Make the checksum span explicit

- **what**: Include ETB or ETX in the raw-byte checksum, before decoding text.
- **why**: The addition sentence does not explicitly name the terminator,
  although the frame layout puts it inside the checked block. An explicit
  span prevents competing readings; decoding first could change checked bytes.
- **source**: LIS01-A2 §6.3.1.2–6.3.3.1, pp.7–8.
- **test**: `astm_checksum_sums_digit_through_etx`.

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
- **test**: `limits_reject_instead_of_truncating`.

## 5. Configure whether to honor receiver interrupts

- **what**: Expose honoring an interrupt as a dialog setting.
- **why**: The sender may continue or yield. An explicit setting lets the
  installation choose that behavior without a vendor special case in the
  shared state machine.
- **source**: LIS01-A2 §6.3.5.1–6.3.5.3, p.8.
- **test**: `interrupt_honored_ignored_and_issued`.

## 6. Keep one outgoing message buffer

- **what**: Refuse a second outgoing message while the dialog owns one.
- **why**: Extra buffering is optional. The host already owns durable order
  scheduling, so a second queue in the link would complicate shutdown and
  uncertain sends without adding a needed capability.
- **source**: LIS01-A2 §6.5.2.6, p.10;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), dialog ownership.
- **test**: `one_message_buffer_and_validated_settings`.

## 7. Refuse unsupported search semantics

- **what**: Serve supported exact-sample order queries; refuse unsupported
  ranges and filters instead of evaluating only their understood parts.
- **why**: LIS02 describes searches without requiring every search service.
  Ignoring a filter could select a different order set and send tests the
  query did not request.
- **source**: LIS02-A2 §§11.3–11.8, pp.25–26.
- **test**: `cancel_and_unsupported_filters_are_not_empty_orders`.

## 8. Make acknowledgment depend on durable custody

- **what**: Reserve capacity before accepting a transfer and commit each frame,
  with its transfer identity, before its positive data acknowledgment.
  Accepting ENQ only means "ready". A transfer that was acknowledged frame by
  frame but never completed is kept for recovery.
- **why**: A valid checksum proves nothing about crash survival. The analyzer
  must retain responsibility if the host cannot keep the promised evidence.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), ASTM row;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), Store before acknowledging.
- **test**: `failed_or_unanswered_commit_never_acks`.

## 9. Detect conflicting retransmitted frames

- **what**: Compare retransmission identity with retained payload evidence;
  a reused frame number with different content is not a duplicate.
- **why**: The small cyclic frame number handles sequencing, not durable
  content identity. A lost ACK must not let conflicting bytes pass as the
  previously stored frame.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Keep the original
  and Three identities.
- **test**: `committed_duplicate_acks_and_conflict_naks`.

## 10. Isolate nonproduction queries from real orders

- **what**: Refuse real order lookup for training, debug and QC query contexts.
- **why**: A syntactically valid test exchange must not reserve patient work.
  The processing label is therefore a host authorization boundary, not merely
  retained metadata.
- **source**: LIS02-A2 §6.12, p.12;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle policy.
- **test**: `nonproduction_queries_never_reserve_or_send`.

## 11. Keep an unknown lookup distinct from an empty lookup

- **what**: A failed lookup or elapsed query budget produces a query error or
  silence, never a successful no-information answer.
- **why**: No-information would tell the analyzer there is no work when
  Analyte merely failed to learn what work exists.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Lookup failure
  is not no orders.
- **test**: `missed_deadline_never_answers_no_information`.

## 12. Keep result history separate from transfer sequencing

- **what**: Assign revisions from trusted observation identity and status
  history, independently of frame numbers and transmission IDs.
- **why**: A transfer retry must not create another clinical result, while a
  preliminary-to-final change must not become an unmarked-correction conflict.
  Link sequencing cannot decide either question.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities;
  [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `revisions_tell_new_retransmission_correction_and_runs`.

## Consecutive comments (standards pass 2)

LIS02-A2 sections 10 and 4.1 attach consecutive comments to the first preceding
noncomment record; manufacturer ownership remains immediate. Regression:
`consecutive_comments_share_their_owner` checks parser parents
and delivered result annotations. Previously interpreted receipts containing
consecutive result comments could have omitted later annotations or retained a
comment as their parent. Raw receipts are unchanged. No deployed store was
inspected. Identify affected receipts in the protected store and reprocess the
original bytes with the fixed parser and recorded config. Reprocessing adds a
new interpretation version, preserving the raw receipt and old interpretations;
review prior deliveries for reconciliation rather than overwriting their history.

Contextual C/M levels are tracked independently of P/O/R ancestry. Section
5.6.7 resets the counter when the same type changes level or a lower numbered
level intervenes. Regression: `comment_and_manufacturer_level_changes_reset_sequence`.

Section 5.1 keeps BEL, HT, VT and FF out of text data but allows them in a
record. Deliberate tolerance (owner decision, 2026-11-18): a text or comment
component keeps them byte for byte instead of refusing the record, and the
mapping adds one `astm_control_character_in_text` note per field to the
receipt's departure evidence (`ControlCharacterKept`, shown on the admin
receipt evidence page). It is always on, not a machine departure: ADR 0015
covers rules that change how a value or status is read, while this one changes
nothing but acceptance and every use is recorded. Contract v1 has no field for
notes or departures, so the delivered document is unchanged, like packed ASTM
records. Every other control is still refused per record, header-selected
delimiters remain syntax and identifiers still refuse controls at their own
boundary. Tests: `text_controls_kept_byte_for_byte`,
`astm_comment_tab_kept_with_note`.

Status evidence now asserts every section 9.9 code against its named meaning
and parsed result view (`each_status_code_has_specified_meaning`).
Manufacturer evidence checks all eight fixture positions, sequence and exact
round-trip (`all_manufacturer_positions_are_retained`); this proves
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
- **record only**: [ADR 0034](../../adr/0034-record-only-connections.md)
  decision 10 lets a model's `record.toml` declare the same policy for one
  analyzer mode the lab answers (the cobas e 411's cobas protocol type). The
  record-only host keeps every chunk before replying and never reads records,
  so no ledger is written; the snapshot names the declaration. [Paired
  record-only cases](../../../crates/analyte/tests/record_only/packing.rs)
  cover strict NAK, cobas-type ACK with capture, and a bad checksum.
- **tests**: [paired strict/packed dialog cases](../../../crates/steps/dialog/tests/packed.rs)
  cover frame custody, checksum failure, retries and ETB continuation.
  [Stored interpretation cases](../../../crates/analyte/src/machine/interpretation/departure_evidence/tests.rs)
  cover v1 delivery without semantic changes and evidence retained after bad
  LIS02 hierarchy. Existing link/storage recovery fixtures remain unchanged.
