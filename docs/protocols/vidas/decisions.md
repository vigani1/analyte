# VIDAS decisions

Download decisions were checked against BCI Guide C and VIDAS PC Manual F.
Some earlier protocol decisions still refer to the repository's
[VIDAS research](../../research/machines/vidas.md). Evidence stays documented.
Wire requirements remain in [checklist.md](checklist.md).

## 1. Limit support to the VIDAS PC Literal family

- **what**: Exclude mini VIDAS, VIDAS 3 extensions and XML/FTP from the Literal
  implementation's supported scope.
- **why**: Similar product names conceal different directions, fields and
  transports. Reusing the PC parser would imply equivalence the available
  evidence does not establish.
- **source**: [Research §§1–2](../../research/machines/vidas.md#1-identity-generations-and-typical-test-menu);
  [ADR 0007](../../adr/0007-standards-first.md), explicit scope.
- **test**: none yet.

## 2. Send one app order per download session

- **what**: Validate every request before opening one session for the app
  order. Send a complete packet for each assay, with an ACK at the selected
  Literal or Alternate boundary. End with EOT after the final packet.
- **why**: Guide C allows both individual sessions and a multi-packet session.
  A failed packet leaves the whole order uncertain. A successful exchange
  establishes transfer only, because the PC can still refuse an assay.
- **source**: Guide C pp. 3-2 to 3-4 and 10-2.
- **test**: `all_assays_confirm_once_and_survive_restart` and
  `second_packet_nak_leaves_entire_order_uncertain`.

## 3. Require explicit framing compatibility settings

- **what**: Configure Literal/Alternate, suffix handling, checksum span and
  hex policy; do not infer them from a packet or import ASTM defaults.
- **why**: The ACK boundary differs between the two modes, and suffix checksum
  participation is unresolved in the research. An inferred setting could
  acknowledge the wrong extent of a transfer or reject valid installed traffic.
- **source**: [Research §3](../../research/machines/vidas.md#3-literal-and-literal-alternate-low-level-protocol).
- **test**: `every_suffix_and_boundary_round_trips`.

## 4. Require installed timing policy rather than guessed vendor defaults

- **what**: Supply ACK, interbyte, session and commit deadlines explicitly;
  the retry configuration does not initiate automatic resend.
- **why**: The research supplies no dependable numeric timing or retry rules.
  Borrowing ASTM constants would give an unrelated protocol's values a false
  appearance of vendor authority.
- **source**: [Research §3](../../research/machines/vidas.md#3-literal-and-literal-alternate-low-level-protocol);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded waits.
- **test**: none yet.

## 5. Stop a failed decoder instead of scanning ahead for apparent recovery

- **what**: A checksum or bound failure ends that decoder instance; retain
  the transport evidence through the host rather than guessing a new packet start.
- **why**: A control-looking byte after malformed input may belong to the
  damaged transfer. Continuing could hide a lost packet boundary and accept
  only a plausible fragment of the original evidence.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Never drop data
  and Keep the original.
- **test**: `bad_checksum_or_oversize_is_refused`.

## 6. Keep ambiguous value and dilution fields as text

- **what**: Preserve qn as written, ql as a call, nc as a source flag and y3
  separately; perform no inferred numeric conversion or dilution multiplication.
- **why**: The research reports contradictory ql/qn descriptions, comma versus
  dot decimals and ratio versus integer dilution. Choosing a universal numeric
  interpretation would discard precisely the distinctions still needing evidence.
- **source**: [Research §4](../../research/machines/vidas.md#4-message-format-complete-field-table-and-assay-codes)
  and [§7](../../research/machines/vidas.md#7-annotated-example-messages);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), values and dilution policy.
- **test**: `keeps_exact_identity_unknown_tags_and_values`.

## 7. Select the installed sample-ID ceiling

- **what**: Require the 12- or 17-character policy and reject an ID that does
  not fit; do not trim it to make a work list encodable.
- **why**: The research identifies different limits for different PC
  generations. Choosing the larger limit silently could lose an order; choosing
  the smaller by truncation could associate it with another specimen.
- **source**: [Research §6](../../research/machines/vidas.md#6-sample-identification--the-critical-distinction);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), string identity policy.
- **test**: `overlong_sample_id_is_rejected_not_truncated`.

## 8. Leave result identity unresolved

- **what**: Do not use ci alone to decide that an upload is a new result,
  retransmission or correction; park unresolved receipts.
- **why**: Several assays and repeats can share ci. A sample ID identifies
  the specimen but supplies no trustworthy run history, so deduplication on it
  would either suppress legitimate work or publish repeated results.
- **source**: [Research §6](../../research/machines/vidas.md#6-sample-identification--the-critical-distinction);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities.
- **test**: `vidas_stores_before_ack_and_parks_unresolved`.

## 9. Do not overload origin or structural tags with specimen metadata

- **what**: Reject any supplied specimen type in a download
  (`unsupported-specimen-type`); do not place it in so or si, or infer a
  section/slot from ci. The machine definition also refuses `specimen_system`
  (ADR 0030).
- **why**: The known tags describe origin and structure, not those physical
  attributes. Reusing them would make the message syntactically plausible while
  falsely claiming that the analyzer received the requested specimen context.
- **source**: [Research §4](../../research/machines/vidas.md#4-message-format-complete-field-table-and-assay-codes)
  and [§6](../../research/machines/vidas.md#6-sample-identification--the-critical-distinction).
- **test**: `specimen_type_is_rejected`.

## 10. Require explicit assay mappings

- **what**: Do not ship a guessed assay-code catalogue or derive a code from
  an assay's display name; reject an unmapped requested test before transmission.
- **why**: Product menus do not establish the installed codes or language.
  Sending a plausible code could request the wrong assay or be rejected after
  transport acceptance without a reliable application-level response.
- **source**: [Research §4](../../research/machines/vidas.md#4-message-format-complete-field-table-and-assay-codes);
  [ADR 0002](../../adr/0002-config-first.md); since
  [ADR 0029](../../adr/0029-lab-app-owns-test-names.md) the order supplies the
  machine's assay code itself, in its code system.
- **test**: `foreign_test_is_rejected_before_send`; mapper
  `vidas_assay_codes_are_checked_when_ordered`.

## 11. Tie the data ACK to durable custody

- **what**: Reserve capacity before admission and wait for durable transfer
  storage before the configured data-ACK boundary.
- **why**: Literal can acknowledge data before the final ETX/EOT exchange.
  A later disconnect must therefore leave the acknowledged evidence retained,
  even if the transfer never reaches normal completion.
- **source**: [Research §3](../../research/machines/vidas.md#3-literal-and-literal-alternate-low-level-protocol);
  [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), VIDAS row.
- **test**: `ack_waits_for_store_and_keeps_incomplete_transfer`.

## 12. Park uncertain downloads instead of resending

- **what**: Timeout, NAK, collision or disconnect stops the conversation;
  do not emit an undocumented abort sequence or automatically retry work.
- **why**: The PC may already have inserted the request when its response is
  lost. Repeating it without reconciliation could schedule duplicate testing,
  and the research does not establish a safe abort or retry exchange.
- **source**: [Research §3](../../research/machines/vidas.md#3-literal-and-literal-alternate-low-level-protocol);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle rule.
- **test**: `download_never_resends_after_lost_ack`.

## 13. Treat completion as transport evidence only

- **what**: A completed download does not become proof of machine acceptance
  or clinical validation.
- **why**: The PC can reject an assay after its transport ACK, and the nc flag
  concerns source assay state rather than clinical approval. Promoting either
  into app acceptance would invent an outcome the exchange does not establish.
- **source**: [Research §§4–5](../../research/machines/vidas.md#5-directions-and-request-download);
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), distinct order states.
- **test**: none yet.

## Read result labels and completion evidence

Guide C pp. 4-5 to 4-9 and B-7 and PC Manual F chapter 10 distinguish internal
kit controls from the PC's external QC slots. Keep the exact control label
and split a kit lot at the single documented separator. The default
`pc_slots` policy uses the reserved PC IDs. A definition can select
`result_control_labels = "appendix_b"` in `[vidas]` when evidence confirms
the exact control flags identify other labels. Conflicting flags or
unrecognized flagged labels are held. See
`vidas_control_labels_keep_composite_and_enforce_qc_width`.

Preserve completion to its written minute precision using the guide's
four-digit year layout. Figure 4-3 shows a shorter date, which stays
uninterpreted. Carry instrument ID, serial and technologist when sent. The
PC sends CMVA doses in separate uploads without a component key. Each
dose keeps its own unresolved receipt, including when the completion
recipe is selected. See
`vidas_cmva_doses_remain_separate_even_with_completion_recipe`.

Guide C appendix B distinguishes invalid measurements from invalid
recalibrations. Literal's supported wire repertoire is printable ASCII.
Recognized invalid calls preserve the original qualitative call and
quantitative text, with an invalid interpretation beside the text. Units
and dilution require an explicit source field. See
`localized_invalid_call_preserves_original_measurement_and_call`.
