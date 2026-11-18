# POCT decisions

The Basic Profile conformance gate remains open. These are scope and host
policy choices; mandatory message rules remain in [checklist.md](checklist.md).

## 1. Implement a DML Observation Reviewer over TCP

- **what**: Build the reviewer side of the Basic Profile, with the Device
  represented by test peers; do not claim Access Point or ORI implementation.
- **why**: The bridge needs the instrument-to-reviewer observation exchange.
  Physical docking and the separate upstream HL7 interface would add different
  responsibilities that the TCP receiver and HTTPS app contract do not fulfill.
- **source**: POCT01-A2 Appendix B §§4–4.1, p.76;
  [ADR 0007](../../adr/0007-standards-first.md), explicit interface scope.
- **test**: none yet.

## 2. Do not enable every topic the Device advertises

- **what**: Keep device events, list updates, directives, keep-alive and
  continuous-mode workflows outside advertised support until qualified.
  Recognize structurally valid KPA.R01 and answer it with a correlated OTH
  Escape only. The standard lets a participant that is not prepared for Keep
  Alive reply this way, and the reply completes the exchange, so the
  conversation continues in its current phase. The bytes stay in the receipt
  journal; nothing is parked, and the reviewer sends no probes of its own.
  Unknown roots and malformed headers remain rejected and retained.
- **why**: Device capability is only half of an agreement. Enabling a topic
  because its objects parse would promise host workflow, authorization and
  recovery behavior that has not been implemented end to end.
- **source**: POCT01-A2 Appendix B §4.1.3, p.78; §4.1, p.76; §3.5,
  pp.74–75; [ADR 0007](../../adr/0007-standards-first.md), selected options.
- **test**: none yet.

The unsupported Keep Alive fallback is tested by
`unsupported_keep_alive_gets_correlated_escape` (Appendix B 3.5,
4.1.9 and 6.9).

## 3. Close on expired configured waits instead of waiting indefinitely

- **what**: Require bounded host waits, use the Device's declared timeout in
  admission, and close an expired conversation without replaying its send.
- **why**: The standard leaves the reviewer latitude to wait, terminate or
  disconnect. An unattended bridge needs a finite failure outcome so a stalled
  peer cannot retain connection resources forever.
- **source**: POCT01-A2 Appendix B §3.6, p.75;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded waits.
- **test**: `minimum_capability_peer_and_hello_deadline`.

## 4. Never guess acknowledgment correlation for a malformed Hello

- **what**: Retain an ambiguous Hello without extracting a guessed control ID
  from body text to construct an error acknowledgment.
- **why**: The startup error response assumes the receiver can identify what
  it is answering. Guessing that identity could acknowledge another message
  while hiding the malformed header that caused the failure.
- **source**: POCT01-A2 Appendix B §4.1.2, p.78;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), keep evidence and never invent data.
- **test**: `ambiguous_hello_header_parked_without_guessed_ack`.

## 5. Refuse DTD resolution in the XML reader

- **what**: Reject document DTD declarations instead of resolving entities
  or fetching schemas while parsing a receipt.
- **why**: Resolution would let a machine's XML trigger file or network I/O
  and amplify input beyond configured parsing bounds. Protocol structure is
  checked locally, with the raw rejected XML retained for diagnosis.
- **source**: [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), host I/O
  ownership; [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded input.
- **test**: `rejects_dtd_without_resolving_it`.

## 6. End a conversation when its control-ID budget fills

- **what**: Keep the bounded conversation-local ID history without eviction;
  stop admitting further data when its budget is exhausted.
- **why**: Evicting old IDs would allow a later reused ID to appear new.
  Ending the conversation preserves duplicate detection while keeping memory
  bounded; reconnect establishes a fresh protocol namespace.
- **source**: POCT01-A2 Appendix B §5.13, p.99;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded input and retained identities.
- **test**: `control_id_budget_ends_conversation_keeping_ids`.

## 7. Return ownership of deferred input to the host

- **what**: Apply bounded backpressure and return unconsumed input instead
  of silently dropping it or building an unbounded dialog queue.
- **why**: Only the host controls transport reads and durable storage. An
  explicit ownership handoff lets it stop reading until custody capacity is
  available, without losing a message between the transport and dialog.
- **source**: [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), layer
  ownership; [ADR 0003](../../adr/0003-clinical-safety-rules.md), Never drop data.
- **test**: `backpressure_returns_unconsumed_input`.

## 8. Include interpretation outcome in the custody gate

- **what**: Gate the observation acknowledgment on both complete raw storage
  and its retained interpretation outcome.
- **why**: Raw storage alone does not tell the host whether a result is ready
  for delivery or requires operator action. Keeping that outcome with custody
  prevents an acknowledged semantic failure from disappearing into a parse log.
- **source**: POCT01-A2 Appendix B §3.2, pp.71–72;
  [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), POCT row.
- **test**: `poct_stores_before_ack_and_parks_unresolved`.

## 9. Default to unresolved result identity

- **what**: The pure mapping path leaves identity unresolved; resolved
  delivery requires an explicit installed identity policy and its evidence.
- **why**: A valid observation message does not establish how a particular
  Device distinguishes a rerun from a repeated upload. Inferring that policy
  could suppress a new result or publish a duplicate as new.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities;
  [ADR 0002](../../adr/0002-config-first.md), machine configuration.
- **test**: `poct_stores_before_ack_and_parks_unresolved`.

## 10. Keep durable result history across conversations

- **what**: Use the common store's result identity and revision history in
  addition to the dialog's conversation-local duplicate checks.
- **why**: The Device can reconnect after a lost ACK and upload the same
  observation under a new conversation. A fresh dialog must not erase the
  knowledge needed to suppress that retransmission or retain a correction.
- **source**: POCT01-A2 Appendix B §3.2, p.72;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), revision policy.
- **test**: `lost_ack_restart_and_correction_deliver_once`.

## Conservative edited-observation handling

POCT patient context is retained in protected raw receipts but is not delivered
downstream today. Method, range qualifiers and other source context are also
not fully represented by the common content comparison.

When EDT identifies an existing result and its common content equals the latest
revision, park it with the reason "edited POCT observation with no detectable
common-content change; source context may have changed". Keep the raw receipt,
create no revision and send no delivery. Each repeated EDT receipt is parked
separately, including retries after a lost ACK. Changed common content follows
the normal correction path. Identical NEW/RES uploads remain retransmissions.
This policy changes no stored format and never puts editable patient identity
in the immutable result key. A precise protocol-context digest requires a
future design note (ADR), including old evidence and retention behavior.

Source: Appendix B 3.2, 5.15, 5.18, 5.21 Table 49, 5.22; project-lead decision
2026-11-16. Test:
`patient_and_metadata_edits_are_not_retransmissions`.
