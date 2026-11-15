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
- Complete group-1 control field validation in receive and send directions; exact
  MSH/MSA/ERR coverage is listed below. Component failures carry the segment,
  field and component position.
- Recovery classes for malformed triggers, content rejection, invalid ACK,
  inconsistent ORL and host query rejection. Failed responses do not start ACK
  loops. Unnamespaced local diagnostics remain text rather than invented codes.

Not yet complete:

- Completion of groups 2-5: PID/PV1, SPM/SAC, ORC/OBR/TQ1/TCD and OBX/NTE/INV.
- Every remaining Appendix C field/component and option predicate and its individual test.
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

## LAW control fields covered (group 1)

`law::parse_message` shares the bounded syntax parser and additionally accepts
LAW's compatible version declarations beginning with `2.5`. The general
`Message::parse` edition policy is unchanged. `law::validate_control` applies
Appendix C.1/C.3/C.4 and section 2.2 with an explicit `ControlDirection`:
receivers ignore X content while retaining its bytes; senders must omit it.
`validate_segment` checks an individual segment using the send policy; it
cannot establish cross-segment predicates. Transaction receive APIs and all
LAW builders apply the matching direction automatically.

The exact covered field positions are:

- **MSH:** 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 15, 16, 18 and 21.
  Separators are fixed by LAW. Fields 3-6 are RE singletons with HD.1
  namespace limits; HD.1 is R in 3-5 and RE in 6. Field 7 checks TS.1's
  calendar, seconds precision and timezone. Field 9 checks all three required
  MSG components and the LAW code/event/structure combinations. Field 10
  checks a non-null ST identity up to 50 characters. Fields 11 and 12 check
  PT.1 (production) and VID.1 (compatible version). Fields 15/16 resolve to
  mandatory NE/AL on QBP, OML and OUL triggers, and X on RSP, ORL and ACK
  responses. Field 18 is one UTF-8 declaration. Every repetition of 21
  checks required EI.1/EI.2 and their limits, with the transaction/IHE pair
  first. Other MSH fields and unlisted composite components are X.
- **MSA:** 1 and 2. Both are mandatory singletons: the application outcome
  is AA/AE/AR, and the correlation ST is non-null and at most 50 characters.
  Receive transaction APIs compare MSA-2 with the sent MSH-10; builders echo
  the original ID. Other MSA fields are X.
- **ERR:** 2, 3, 4, 5 and 8. Field 2 is RE and repeatable: ERL.1 and ERL.2
  are required, ERL.3-6 are RE; segment names, positive occurrence/position
  numbers and component limits are checked. Field 3 is a mandatory singleton
  CWE: required supported error identifier, RE description, required HL70357
  namespace. Field 4 is a mandatory singleton restricted to E. Field 5 is
  an RE singleton CWE with required identifier/vendor namespace and optional
  description. Field 8 is RE singleton TX; its 250# conformance length allows
  retaining longer text, so it is preserved in full within the parser's
  configured message bound. Other ERR fields and CWE components are X.

These rules include datatype shape, empty/null handling, character lengths,
cardinality and supported component usages. Composite field length ceilings
are checked on output (including empty trailing positions); tighter supported
component limits apply on input, with ignored X components excluded. RE availability belongs to the caller:
missing means unavailable, supplied content is validated without guessing a
replacement. Messages require one MSH; response messages require one MSA;
ERR belongs only to AE/AR responses, which require error detail. Reply headers
reverse supported address components and do not retransmit ignored content.
Source-named tests in `tests/law_control.rs` provide positive and negative
cases for each listed rule, plus receive/build integration cases. The evidence
level remains documented; this completes group 1, not the entire LAW profile.

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