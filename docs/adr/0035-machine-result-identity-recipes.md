# 0035. Machine definitions declare result identity recipes

Status: accepted, 2026-11-23. Extends [ADR 0003](0003-clinical-safety-rules.md)
rule 4 and [ADR 0002](0002-config-first.md). The lab app keeps the reporting
decision, consistent with [ADR 0029](0029-lab-app-owns-test-names.md).

## Decision

- Analyte determines whether an upload is a resend, a new measurement or a
  revision. A definition cites the manual and section for the fields that
  identify a measurement and for its correction and rerun codes. The recipe
  applies in the definition's protocol, model and mapping choice scope.
- For ASTM and HL7 a documented recipe uses the exact sample ID and test code
  alongside the existing order, sub-ID and required run selectors. Run
  evidence can include a source completion time or a unique vendor run
  field. A marker can be selected as run evidence if the manual establishes
  that it distinguishes the measurement and stays stable on resend. A generic
  rerun flag by itself may cover several measurements. If the source cannot
  distinguish them, keep the upload for review.
- Status overrides match the entire source code. A correction targets the
  same measurement key. A rerun is a different measurement when the
  selected source fields distinguish it. Its status marker carries no
  release guarantee. Preserve the raw code and keep the standard meaning
  where no override is configured.
- VIDAS can opt into the sample ID, assay and completion date and time
  recipe. Enable it within a profile only after the evidence establishes
  that the timestamp distinguishes measurements. Autolyser's fixed report
  has no supported unique run field (Rev18 section 23.2, pp. 177-178).
  Preserve it unresolved. IHE LAW and POCT keep their protocol-owned rules.
- The store retains every receipt. Matching key and content is a resend
  with one delivery. A distinct key starts at revision zero. Changed content
  under a retained key requires a correction or an existing protocol
  lifecycle rule to advance the revision. Otherwise it is held with the
  retained history. Missing recipe evidence or sample ID also holds.
- Contract v1 keeps its shape. Selecting a new recipe can change the key for
  previously received data. Review existing results before switching a live
  connection's recipe.

## Evidence and limits

Roche cobas e 411 HIM v2.3 section 6, Table 18 gives cobas-format R.9
`C` the meaning of rerun and omits the per-test R.13 completion timestamp
in that format. The marker establishes its meaning, but an installed
profile still needs evidence that distinguishes successive reruns. BCI Link
161150-486 rev C p. 4-6 identifies both VIDAS completion fields. A timestamp
with insufficient precision cannot qualify a recipe on its own.

## Operator decisions on retained measurements

A review item for changed content under one ordinary recipe key keeps the
claimed key alongside its reading. The operator compares the held result with
the latest retained revision under that key. **This is a resend** records
the relationship in the item's resolution. **This is a rerun** appends a
reading and enqueues one new result at revision zero. Its run starts with
`operator:` and includes the immutable receipt ID and result index. The
host-assigned run is backed by an audit with the actor, time, reason and both
receipts. The source values, timestamps, sample ID and original receipt stay
as received. Rerun requires an eligible production result and the same
effective configuration and delivery route that received it. Atomic protocol
batches retain their protocol-owned reconciliation.

A missing-sample suggestion requires a uniquely matching later retained
delivery from the same configured machine, with explicit measurement time
and matching result evidence. When the comparison cannot cover the receipt
or more than one candidate qualifies, leave the item open for normal review.
The operator may confirm that the later receipt supersedes the held one.
Confirmation resolves the item and adds an audit entry in one transaction.
The later receipt's normal delivery history is the only delivery history.

The protected comparison records access before showing result data. Both
interfaces submit the displayed snapshot. The store rechecks it in the same
transaction as the resolution and any enqueue. If the item or comparison
changes, reopen it and review the evidence again.

## Amendment 2026-11-26: the lab can always be asked about repeats

The owner approved a per-machine site setting, **Repeated results**, on
2026-11-26.

- `automatic` is the default and is not written to the site file. The decisions
  above apply unchanged. A resend is stored once and not delivered again. A new
  key is a new result. A correction advances the revision, and changed content
  under the same key is held.
- `always_ask` is for a lab that wants a person to see every repeat. Analyte
  holds a result when an earlier upload from the same machine already retained
  its identity key. This includes purged history. The result keeps its claimed
  key and appears in Needs review with the same comparison and the decisions
  **This is a resend** and **This is a rerun**. This applies to
  byte-identical resends, corrections and same-key changes alike. A first-seen
  key still flows normally. A receipt that is read again, such as after
  reprocessing, is not a repeat of itself.
- The switch can only make handling more manual. It never creates a recipe,
  never resolves an unresolved result and never relaxes a hold. A held repeat
  reaches the protocol as any other held result does. For example, HL7 answers
  it AA after durable storage, as ADR 0010 (amended 2026-11-26) answers every
  stored upload held for an operator.
- The store makes the decision in one place, the revision assignment for
  ordinary reconciliation. The host passes the machine's effective setting in
  through `ReconcileMode::Standard`. IHE LAW and POCT keep their
  protocol-owned rules, and record-only connections read no results. The site
  file refuses `always_ask` for these, so the setting is never silently ignored.
- The value is part of the effective snapshot, and changing it changes the
  settings version. A rerun decision still requires the configuration that
  received the result. Resolve held repeats before switching the setting back.
- Setup step 2 and `analyte machines set --repeated-results` share one plan
  and save path. The machine's page and `machines list` show the choice in use.

## Amendment 2026-11-27: staff can confirm a held change as a correction

The owner approved a third decision, **This is a correction**, on
2026-11-27.

Why: a machine can change a result under the same identity key without
marking it as a correction, for example tube 000123 glucose 5.4 and then
5.9 with no correction code. Analyte holds it, as above. Until now staff
could only call it a resend, which drops the new value, or a rerun, which
sends it as a second result next to the old one. Neither tells the lab app
that the new value replaces the old one, so staff had no correct choice.
Retyping the value in the lab app instead risks a transcription error on a
patient report.

- The decision applies to the same items, comparison and snapshot recheck as
  resend and rerun: a held ordinary-recipe result with changed content under
  a retained key. It is offered only when the held result differs clinically
  from the latest retained revision under that key. A byte-identical repeat
  held by `always_ask` has nothing to correct, and a missing-sample
  suggestion is not a same-key change; neither offers it.
- The held reading becomes the next revision of the same result, exactly as
  a machine-marked correction does: the machine's identity key is kept and
  the revision is the latest one plus one. The store appends a reading and
  enqueues one delivery for that revision through the normal outbox, so a
  retry reuses the delivery's ID. It requires an eligible production result
  and the same effective configuration and delivery route that received it,
  like a rerun.
- The run is not rewritten. An `operator:` run would make it a new result,
  which is what a rerun is for; keeping the key is what makes the lab app
  replace the value. The operator decision is marked instead by the audit
  action `parked-measurement-correction`, with the actor, time, reason, both
  receipts and the snapshot, written in the same transaction as the
  resolution and the enqueue. The source values, timestamps, sample ID and
  both original receipts stay as received.
- If the item or comparison changed after it was shown, the decision is
  refused and the item stays open to be reviewed again.
- The admin page asks for confirmation first, showing the earlier and new
  values side by side, and requires a reason. The command line takes
  `parked decide ITEM correction` with the printed snapshot.
- Contract v1 keeps its shape: the delivery is an ordinary result whose
  revision is above zero. With Needs review notices on (ADR 0038), the
  closing notice carries the new outcome `correction`, an additive value of
  the closed outcome enum.
