# Autolyser decisions

The other notes and [checklist](checklist.md) retain the wire requirements.
These entries explain implementation choices and interpretations of Rev18,
including the distinctions between manual rules and Analyte's recovery policy.

## 1. Implement the internal format rather than a script engine

- **what**: Support the internal host format; leave the variable serial
  protocol and its script compiler outside this implementation.
- **why**: A script can change the layout and constants that determine
  identity and checksum coverage. Treating arbitrary scripts as internal-format
  variants would create an unbounded second configuration language to qualify.
- **source**: Operator's Manual Rev18, chapter 23 introduction, p.175;
  [ADR 0002](../../adr/0002-config-first.md), bounded configuration and code ownership.
- **test**: none yet.

## 2. Resolve checksum placement from the consistent layouts

- **what**: Check the payload before the final checksum/EOT, excluding the
  initialization exchange, and use one checksum after all result pairs.
- **why**: The checksum prose says to sum sent characters without fully
  delimiting the span, and the result diagram can be read as repeating the
  checksum per pair. The consistent examples support a single trailing sum;
  this is an interpretation to qualify against installed traffic.
- **source**: Operator's Manual Rev18 §§23.1–23.3, pp.175–179.
- **test**: `autolyser_rev18_p179_checksum_is_decimal_sum`.

## 3. Follow the controls field layout over the inconsistent example

- **what**: Parse the controls-enabled layout at its stated field offsets,
  without inserting the unexplained spaces shown in the example.
- **why**: The example's extra spacing and printed checksum do not agree
  with its field layout. Accepting both by guessing offsets could shift a
  control flag or result into another field while still yielding plausible text.
- **source**: Operator's Manual Rev18 §23.2, p.178.
- **test**: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.

## 4. Select the controls variant explicitly

- **what**: Require the configured controls setting instead of detecting the
  report layout from its length or content.
- **why**: The analyzer's option changes offsets for the whole report.
  Autodetection could mistake a damaged patient record for a control variant
  and hide the real defect.
- **source**: Operator's Manual Rev18 §23.2, pp.177–178;
  [ADR 0002](../../adr/0002-config-first.md), installed settings.
- **test**: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.

## 5. Preserve exact identity bytes rather than normalize padding

- **what**: Keep the fifteen-byte patient code intact; the optional 8+7
  constructor accepts exactly eight input characters and only appends spaces.
- **why**: The manual describes padding but does not establish a universal
  normalization for all identifiers. Trimming or extending arbitrary input
  could merge two different patient codes.
- **source**: Operator's Manual Rev18, chapter 23 introduction and §23.1, p.175;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), string identity policy.
- **test**: `autolyser_rev18_p179_checksum_is_decimal_sum`.

## 6. Require a known cup position instead of autoassignment

- **what**: Reject missing or zero cup positions in host work lists rather
  than selecting the manual's automatic-position option.
- **why**: The bridge cannot verify where staff loaded an unbarcoded tube.
  An explicit position makes that physical assignment reviewable before an
  order reaches the analyzer.
- **source**: Operator's Manual Rev18 §23.1, p.175;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), no invented sample context.
- **test**: `autolyser_rev18_ch23_missing_cup_position_never_autoassigns_zero`.

## 7. Do not substitute sample ID for required patient context

- **what**: Require the mapped patient identifier and namespace for insertion;
  a missing patient field is not filled from the app's sample ID.
- **why**: The wire has one patient-code field and no separate sample-ID
  field. Treating those app identities as interchangeable would conceal an
  incomplete mapping and could join unrelated records.
- **source**: Operator's Manual Rev18 §23.1, p.175;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), no invented identity.
- **test**: `autolyser_rev18_ch23_missing_patient_never_uses_sample_id`.

## 8. Retain even an empty poll response before interpreting it

- **what**: Store complete replies, including NAK and insertion replies,
  before reporting their meaning to the host.
- **why**: An empty queue response changes what the host knows about a poll.
  Losing that evidence on a crash would leave the recovery path unable to
  distinguish an answered request from an uncertain cursor advance.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), Autolyser row.
- **test**: `autolyser_rev18_p177_empty_queue_still_requires_storage`.

## 9. Recover a corrupt response through L before further cursor movement

- **what**: Block R and A until a valid L response resolves corruption;
  an empty L cannot clear the block.
- **why**: L revisits the last report, whereas R and A can change which report
  is being retrieved. Letting an empty recovery reply count as success could
  permanently step past the report whose evidence was corrupt.
- **source**: Operator's Manual Rev18 §23.2, p.177;
  [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), poll custody policy.
- **test**: `autolyser_rev18_p177_l_empty_does_not_resolve_corruption`.

## 10. Persist a poll guard across connection and process lifetimes

- **what**: Store poll intent before R; an unresolved guard after restart
  prevents session opening and sends neither L nor R until operator reconciliation.
  Vendor L recovery after restart is an operator step, not an automatic program path.
- **why**: A new dialog has no knowledge of the analyzer's earlier send
  cursor. Starting it as though nothing happened could replace the last
  recoverable report with the next one before custody is settled.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), uncertain R
  reconciliation; [ADR 0003](../../adr/0003-clinical-safety-rules.md), durable evidence.
- **test**: `autolyser_rev18_p175_adr0003_uncertain_r_blocks_restart_until_operator_reconciliation`.

## 11. Make timing and byte limits installation policy

- **what**: Require poll, ACK, response, interbyte and storage deadlines and
  a response bound; retain partial evidence and close on exhaustion.
- **why**: The host description does not specify safe numeric defaults for
  these waits. Separate total and interbyte limits prevent a slow trickle from
  keeping an incomplete response alive indefinitely.
- **source**: Operator's Manual Rev18 §§23.1–23.2, pp.175–178;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded operations.
- **test**: `autolyser_rev18_p177_partial_timeout_and_storage_failure_never_resend`.

## 12. Do not retry an uncertain insertion

- **what**: A lost insertion reply leaves the order uncertain without
  automatic retransmission.
- **why**: The analyzer may have inserted the patient before the reply was
  lost. Retrying would make the order's external effect depend on undocumented
  duplicate/clone behavior instead of retained acceptance evidence.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle rule.
- **test**: `autolyser_rev18_p175_lost_download_reply_is_uncertain_without_retry`.

## 13. Park results whose run identity cannot be established

- **what**: Do not infer a stable run identity, timestamp or missing unit
  from the patient code, result value or receipt time.
- **why**: Those invented values would make a retransmission, correction
  and rerun appear distinguishable when the supplied report does not establish
  that distinction. The manual documents no immutable execution or revision
  identifier, including in the variable transmission fields. Result identity
  therefore stays unresolved and the receipt stays parked for an operator;
  patient association alone cannot distinguish reruns from corrections.
- **source**: Operator's Manual Rev18 §23.2, pp.177–178 and §23.5,
  transmission variables p.183;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities and values.
- **test**: `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l`.

## 14. Finish pending custody after disconnect without reopening the dialog

- **what**: Consume pending storage completions after the connection closes,
  but do not let them resume the old protocol session.
- **why**: Disconnect ends the opportunity to reply, not the obligation to
  keep bytes already received. Separating those lifetimes prevents a late
  commit from reviving a connection whose external outcome is already uncertain.
- **source**: [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), host and
  dialog ownership; [ADR 0003](../../adr/0003-clinical-safety-rules.md), Keep the original.
- **test**: `autolyser_rev18_p175_disconnect_during_commit_cannot_reopen_connection`.
