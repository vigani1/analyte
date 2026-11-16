# 0016. AWOS status notifications

Status: proposed, 2026-11-16.

## Context

LAW permits LAB-29 without observations for SC, CA and IP. The current
[restriction](../protocols/law/decisions.md#status-only-lab-29-restriction)
parks these as `UnsupportedLawStatusOnly`. Normal progress would therefore
create operator noise. [Research and sources](../research/law-status-notifications.md)
distinguish AWOS execution status, command outcomes and observation status.

The current [model](../../crates/core/model/src/lifecycle.rs) has nine order
states and no InProgress state. The [order store](../../crates/core/store/src/orders/machine_orders/mod.rs)
retains per-item placer identities, namespace, test coverage, cancellation
origin and append-only history. Its
[exchange completion](../../crates/core/store/src/orders/machine_orders/items.rs)
marks omitted items Uncertain and conservatively aggregates mixed outcomes.
That API describes a completed command exchange, not a sparse asynchronous
status notice; using it for LAB-29 would damage unrelated item states.

This proposal preserves [ADR 0003](0003-clinical-safety-rules.md),
[ADR 0004](0004-outbound-only-polled-orders.md),
[ADR 0008](0008-app-contract.md),
[ADR 0010](0010-acknowledgment-boundaries.md) and
[ADR 0011](0011-result-status-progression.md). It authorizes no code by itself
and changes no current support claim or plan status. The explicitly requested
ADR number is retained even though 0015 is absent in this checkout.

## Proposed decision

### Keep execution status alongside command state

Represent a status notification as a typed, non-result interpretation in the
existing receipt/store path. Give each retained order item an optional AWOS
execution projection: Scheduled, InProgress or AnalyzerCancelled, plus the
evidence that established it. Missing evidence remains unknown. This is an
extension of existing order evidence, not a new queue or protocol subsystem.

**A status-only LAB-29 changes no existing `OrderState`.** Accepted remains
Accepted while its AWOS execution projection can move to InProgress. It does
not move Accepted to a newly invented wire state, Requested to Accepted,
Uncertain to Accepted, or any state to Done. It also does not resolve Sending
or a missing LAB-28 acceptance. Those cases keep their command uncertainty
and the existing reconciliation requirement. This deliberate separation
avoids silently redefining contract v1.

| New status evidence | Execution projection | Command/order-item effect |
| --- | --- | --- |
| SC, no RESULT group | Unknown to Scheduled; Scheduled remains Scheduled | None |
| IP, no RESULT group | Unknown or Scheduled to InProgress; InProgress remains InProgress | None; a missing earlier SC is permitted |
| CA, no RESULT group | Unknown or Scheduled to AnalyzerCancelled, if consistent with all retained evidence | None; record analyzer origin and whether a cancel request was outstanding |
| Status inconsistent with retained later evidence | Preserve existing projection; park conflicting interpretation | None; never rewind a terminal state |

Initialize/check this projection against retained LAB-28 status and result-
bearing LAB-29 history, not just empty notifications. If the analyzer already
reported processing, completion or results incompatible with cancellation,
CA is a conflict, not a new terminal fact. The SC-to-CA restriction follows
TF-2b C.8 Figure C.8-1; CA after evidenced IP is parked pending reconciliation.
Absence of an earlier SC does not itself prove processing began.

### Custody and matching before AA

1. Bound and durably store the original complete message with receipt ID,
   receive time, raw digest, driver and retained configuration version/hash.
2. Validate the LAW structure and all AWOS groups. Resolve OBR-2 EI.1 using
   the installed machine and retained manager namespace, then link to the
   original machine-order row, immutable item index, reserved app order
   version and exact test coverage. Use retained work, not a mutable current
   cache entry or a guessed sample/test/time match. Preserve ignored OBR-2
   authority content only as original evidence, as the existing matcher does.
   Check specimen/container/service consistency under TF-2b B.7.6 and C.6.
   Apply existing processing-mode and patient/QC separation gates: training,
   debugging or unknown-context evidence cannot update production work.
3. In one transaction, append the interpretation, receipt-to-item links,
   source ORC-1/ORC-5, supplied times/ORC-27 estimate, scoped notes and
   disposition, and update the affected projections. No result revision or
   result outbox row is allocated. A sparse notice touches only named AWOSs;
   unrelated items retain their state and history.
4. Return ACK^R22 AA only after both raw custody and that transaction commit.
   Persist enough disposition evidence to reproduce the acknowledgment after
   restart. A capacity refusal, commit failure or expired handling deadline
   cannot lead to AA; use the existing LAW error policy and stop visibly.

Unknown/ambiguous AWOS, conflicting specimen/container/test or contradictory
groups reject the **whole message** with AR and located ERR under B.7.6.
Retain raw bytes and a specific parked interpretation, but apply no partial
status updates or result deliveries. Other syntax/unsupported-content errors
retain the existing specific AE/AR policy; durable raw storage alone never
justifies AA. A valid mixed message must commit all status and result outcomes
under the same message acceptance boundary.

Keep source times exactly as supplied; record host receipt/commit order
separately. For ORC-27 follow TF-2b C.8 Table C.8-9: date or date/time, no
independent offset or precision component, with its zone taken from MSH-7.
Retain that derivation explicitly; if the header cannot establish an offset,
do not invent one. ORC-27 is an estimate, not proof of completion or a command
timeout. No estimated time is invented. An absent estimate does
not erase retained estimate evidence; an explicit source null and a changed
estimate are recorded distinctly. Ambiguous estimate ordering is shown as
such rather than silently choosing a clinical time.

### Analyzer cancellation is not cancellation confirmation

An unsolicited CA reports what the analyzer says happened to this AWOS. It
does not withdraw the app order, cancel sibling AWOSs, delete results, select
another machine or create CancelRequested. Show AnalyzerCancelled prominently
in the admin order detail and an actionable cancellation list, with request
origin and receipt evidence. This is an operational condition, not a fabricated
failed/cancelled observation and not a parked parsing failure.

If an app cancellation is already outstanding, retain that association but
leave CancelRequested pending its correlated LAB-28 ORL response. Only the
existing explicit CR acceptance path confirms that request; UC remains a
refusal and restores retained pre-cancel command state. An incompatible CA
and UC pair requires reconciliation, preserving both facts; neither is erased.
Timeout stays Uncertain. Repeated CA cannot clear command uncertainty.

The execution fact must guard subsequent command admission: never dispatch a
new cancel or new work automatically for an AWOS known analyzer-cancelled,
even though its last command state remains Accepted. Preserve an app's later
cancel intent for visible reconciliation; do not manufacture a CR. An already
admitted command may have left the host; do not assume receipt of CA undid it.

No notification, retry, replay, reprocess or recovery path sends work, requests
a rerun, or issues a cancellation. Any new machine command remains a separately
authorized, durably reserved operation through the existing order path. This
proposal also does not implement LAW's multi-analyzer redundant-work policy.

### Repeats, ordering and recovery

Every physical receipt remains evidence. Within the configured machine/source
scope, a repeated trusted message identity with identical bytes reuses its
committed disposition: no repeated state effect or operator task. Different
bytes under that identity are a specific conflict. A digest alone is not a
semantic identity. A new message identity repeating SC, IP or CA appends its
own evidence but does not repeat the lifecycle effect. A changed IP estimate
can be new evidence without being a result correction.

Receive order is not analyzer event order. The projection never regresses
InProgress to Scheduled or reopens AnalyzerCancelled/completed work. A trusted
exact retransmission can receive its original AA even after later progress.
For a different message, a backwards status is retained as superseded only
when comparable source evidence proves it is older. Otherwise park it as an
ordering conflict and reject the whole message; do not sort by MSH-7 alone or
guess missing offsets. A terminal result does not disappear because an old
IP arrives. New forward status after terminal cancellation is a conflict.

After crash before interpretation commit, recover the stored raw receipt as
pending interpretation and finish it without machine commands. After commit
but before AA, a retransmission finds the committed disposition and is safe to
acknowledge again. Rebuild projections from committed interpretation history;
restart never resets them or cancellation ownership. Restore retains the
existing send pause and reconciliation gate. Reprocessing creates a new
interpretation version with audit, never another automatic command or a
silent rewrite of the original history.

### App contract and operator visibility

Initially expose status-only notifications **only on the protected admin page**:
per-item latest execution status, full receipt-linked history, receive/source
times, estimate with uncertainty, and cancellation origin/pending request.
Use bounded pagination; normal repeated progress is history, not one operator
task per message. An analyzer cancellation condition is one actionable item
per AWOS, with later evidence attached. Existing uncertainty remains visible.
No sample IDs, source text or raw messages enter ordinary logs or external
operational alert payloads.

Do not send empty `/results`, invent a result identity, or send an `accepted`
or `cancel_confirmed` `/order-events` document to disguise the new fact.
[Contract v1](../contract/v1.md#post-order-events) has a closed state vocabulary
and lacks AWOS/item provenance. Unknown JSON fields being ignored does not
allow a new state value or endpoint to be assumed supported.

No v1 change or optional event is introduced by this proposal. If apps later
need AWOS progress, prefer an explicitly negotiated, optional AWOS event
capability with its own documented envelope, receipt/item provenance and
stable delivery ID/immutable retry bytes. If support must be mandatory or
existing state meanings change, that requires contract v2 alongside v1 under
ADR 0008. Either route requires a separate contract decision and receiver
tests before sending. A v1-only app receives existing command/result events
unchanged and cannot see these additional operational facts remotely; the
lab must accept that limitation and assign an admin responder before use.

ADR 0011 remains solely responsible for observation revisions, including
unverified-to-final progression and corrections. AWOS IP is not observation
preliminary; CA is not result withdrawal. CM without the required RESULT
structure remains parked; it cannot mark Done. Even legal CM with observations
does not prove that every result is final/reportable. Completion policy and
per-result delivery continue to require their existing independent evidence.

## Consequences and implementation gate

This avoids adding InProgress/AnalyzerCancelled to the shared command enum
and its frozen wire copy. The cost is two clearly labelled facts on the admin
page and no remote status delivery yet. Reusing CancelConfirmed for unsolicited
CA, synthesizing a result, or forwarding a novel enum to v1 apps would blur
evidence; continuing to park ordinary progress would bury real exceptions.

Unknown work (including unsupported analyzer-created/reflex work without an
authorized retained match), migrated unknown coverage, malformed notices,
unsupported text/content, identity conflicts, illegal progression and
unsupported CM-only messages remain parked with distinct reasons. No evidence
level is raised. Existing `UnsupportedLawStatusOnly` stays until implementation
and qualification replace it; old parked receipts need explicit audited
reprocessing, not automatic bulk acceptance.

Implementation needs bounded store migrations and a notification-specific
transaction, not a call to command-exchange completion. Preserve references
and holds under ADR 0013; initially keep these receipts while any linked work,
cancellation or conflict is open, and do not make admin viewing equivalent to
app acknowledgment for retention. Do not purge status-only receipts until an
explicit retention eligibility rule for this interpretation is implemented
and tested. Disk reserve and visible capacity refusal still apply.

Before enabling any LAW machine, record source-named synthetic tests and an
official simulator conversation covering SC/IP/CA without observations,
estimates, mixed groups, repeats, same-ID conflicts, unknown/mismatched AWOS,
partial-item notices, backwards/ambiguous order, result-after-progress and
progress-after-result, analyzer CA versus requested cancel/CR/UC races, lost
AA, restart on either side of commit, full disk, restore pause and retention
holds. Assert exact ACK/error bytes, durable evidence and **zero machine
commands and zero synthetic result deliveries** from notification handling.
Check contract v1 remains byte-compatible. Review the restriction, checklist,
plan claims and pilot instructions when implementation actually changes them;
this documents-only PR changes none of those claims.
