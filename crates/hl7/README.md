# HL7 syntax crate

Evidence: **documented**, using HL7 2.3.1 chapter 2 and HL7 2.5.1 chapter 2
and the control datatypes in chapter 2A. Fixtures are original synthetic data.

`Message::parse` checks bounded wire syntax, delimiters, version and connection
encoding. It retains exact bytes; `serialize` returns those bytes. Hierarchy
views borrow from the message and preserve trailing positions. Segment iteration
order (including repeated identifiers) provides the positions a later message
structure layer needs. No group ownership is inferred here.

Structural parsing is separate from `Msh::new`, `Msa::new` and `ErrView::new`.
These views expose control fields; they do not establish message/profile
conformance. Unknown fields and segments remain available. An error never owns
or logs payloads: callers must retain the original input in the protected store.

Connection encoding must agree with MSH-18 (absent means ASCII). Supported
encodings are ASCII, ISO 8859-1 and UTF-8. Repeated charset declarations and
MSH-20 schemes are explicitly unsupported. Character-switch escapes remain
raw; `decode_text` refuses to interpret them. Hex, local and formatting escapes
are preserved rather than executed. Only delimiter escapes decode to characters.
Strict CR is the default; LF and CRLF require explicit selection. A fifth MSH-2
character is rejected: truncation is not part of the selected editions.

ACK rendering requires an explicit outcome and responsibility state. CA requires
storage; AA requires storage and interpretation. Conditions independently suppress
accept/application replies. The caller supplies a new ID and timestamp. The API
cannot verify durable storage itself. Generic ACK helpers do not implement
functional query/order responses, acknowledgment timers or replay handling.

Not implemented here: complete LAW, MLLP, general application message structures, clinical
datatypes, clinical mapping, legacy deferred MCF, sequence or continuation dialogs,
I/O, storage or fault recovery. These are not implied by structural parse success.

Run `cargo test -p analyte-hl7` for source-named cases covering encoding, hierarchy,
escapes, limits, charset failures, control validation, edition-specific errors,
ACK conditions and property tests. Repository-wide verification uses the checks
in AGENTS.md. No real analyzer or external lab app is used by tests.

## LAW implementation status

Evidence remains **documented and partial** (PaLM TF Rev 11.0). This module is
pure: it performs no I/O, storage, order replay, clinical decision or external
pointer retrieval. The host must retain complete original bytes, establish
storage responsibility and own the analyzer-specific deadline.

Covered by source-named synthetic tests and a query property test:

- Six LAB-27 query scopes; QPD predicates; RSP correlation and exact QPD echo;
  work/no-work/unavailable decisions with distinct followups and error locations.
- LAB-28 work grouped by specimen/container, unique supplied AWOS IDs, explicit
  priority/dilution/repeat/reflex instructions gated by peer capabilities, and
  the restricted negative form with query identity echoes.
- Outgoing app-supplied patient/visit, specimen, container and order details,
  plus explicitly scoped notes, gated by peer capabilities.
- ORL/O34 using ORL_O42; message acceptance separate from request outcomes;
  correlation and missing/unknown/duplicate responses; matching new/cancel
  outcomes and control/status pairs. Decisions never retract prior acceptance.
- Atomic LAB-29 interpretation; unsolicited null AWOS; source specimen/container,
  OBX-3/4, statuses, values, units, ranges, flags, notes and equipment retained.
  Original segments retain additional source fields. TCD identity inconsistency
  rejects the whole message. ACK/R22 uses the existing responsibility checks.
- Shared usage/cardinality/length checks, supported field positions, UTF-8 and
  profile headers, timestamp precision/timezone, numeric units, status/datatype
  subsets, equipment multiplicity, SPM/OBX sequencing and repeated-value rules.
  LAW's extended control-ID limit is scoped; base HL7 limits remain unchanged.
- Recovery classes for malformed triggers, content rejection, invalid ACK,
  inconsistent ORL and host query rejection. Failed responses do not start ACK
  loops. Unnamespaced local diagnostics remain text rather than invented codes.

Not yet complete:

- Every Appendix C field/component and option predicate and its individual test.
  The shared validator is not a complete LAW conformance validator.
- Complete typed validation of all value composites and enhanced-content
  relationships and every message-group predicate. Specimen-level observations
  are retained separately from AWOS results.
- Related observations and full capability/profile negotiation.
  Related-observation and pooling manager options are not selected or implemented.
- Qualified simulator evidence and host integration cases (durability failures,
  restart, lost replies, cancellation races, timing and external-data retention).

Do not interpret a successful view construction or passing local tests as full
LAW conformance. Tests do not establish the clinical reconciliation/lifecycle
policy. The host must preserve uncertain prior acceptance and stable identities.

## Recording a Gazelle run

Retain a de-identified recording with exact bytes, direction, relative timing
and outcomes. Record the tool build, validation profile identifier and revision,
PaLM TF revision, Analyzer Manager actor, selected options and peer configuration.
List each scenario and its validation report: every query form, genuine empty
work versus unavailable lookup, multi-AWOS decisions, duplicates after uncertain
acceptance, cancellation races, unsolicited results, corrections/reruns,
inconsistent result rejection, selected enhanced content, lost ACK, disconnect,
restart and storage failure. Record validation-profile changes between runs.
Network exchanges belong to host/dialog integration. No Gazelle run has been
performed and the evidence level has not been raised.

## Legacy deferred query implementation status

The `query` module is an initial, documented-only implementation, not complete
coverage of checklist H. It accepts a single deferred display QRY/Q02 in either
edition, preserves QRD/QRF filters, renders QCK status, renders bounded complete
DSR display lines, and reads correlated application ACK/Q03 replies. QAK-1 is
empty for original-style queries. Lookup failure produces AE, never NF.

Outstanding before integration: complete QRD/QRF datatype, table and predicate
validation; typed filter interpretation; continuation and scheduling contracts;
additional quantity units and escaped/non-ASCII display text; comprehensive
negative fixtures. The constructor checks the supported envelope and selected
required fields, not full HL7 conformance. The host must interpret every filter
under the agreed interface before declaring its cache answer trusted. Display
rows have no built-in laboratory meaning. These pure functions do not store,
schedule, transmit, retry or change order states.

ORM/O01, ORR/O02, non-LAW OML/O21 and ORL/O22 are not implemented by this change.
Checklist G and full required-field validation remain outstanding.

