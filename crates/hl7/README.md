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

QRD/QRF validation covers field presence, edition-specific lengths and fields,
repetitions, closed query tables and the referenced numeric, timestamp, coded,
person, range and timing composites (231 sections 2.24.4–5; 251 sections
5.10.5.3–4, with their datatype definitions). Named synthetic negative fixtures
cover the rules, alongside boundary and preservation tests. Earlier synthetic
subject fixtures now supply their required XCN source table. Local codes,
namespaces, repeated filters, escaped text, date precision and offsets stay as
received. Explicit null criteria are rejected rather than silently ignored.

The supported transaction remains a single deferred display query; record and
tabular formats, immediate queries and continuation require separate transaction
support. Outstanding before integration: typed interpretation of local filters;
continuation and scheduling contracts; additional display quantity units and
escaped/non-ASCII display output. The host must interpret every filter, including
local codes, timestamps under its agreed timezone and requested timing, before
declaring its cache answer trusted. Structural validation cannot establish that
the host can satisfy those criteria. Display rows have no built-in laboratory
meaning. These pure functions do not store, schedule, transmit, retry or change
order states. Passing these tests does not establish whole-HL7 conformance or
raise the documented evidence level.

## Legacy order implementation status

The `order` module builds ORM/O01 and reads correlated ORR/O02 in both editions
(231 sections 4.2.1–2 and 4.3.1; 251 sections 4.4.1–2 and 4.5.1). Evidence remains
documented. It supports NW/CA requests and OK/UA/CR/UC responses. Unknown controls,
notifications, request codes in replies and wrong request/reply pairs are located
errors. Message acceptance and each order decision remain separate. A missing
decision is `NotReported`, including for MSA AA; it never authorizes resending.

Caller-supplied data includes namespaced placer/filler IDs, coded tests/batteries,
specimen source, sample identity, priority, a non-repeating time window and explicit
patient/nonpatient context. One ORM represents one patient context and multiple
orders, each with its own identity and one test/battery. Cancellation has no
invented test, specimen or patient details. Repeating schedules, parent/group
relationships and other order controls are outside this API's current scope.

ORM has no standard barcode field. The caller must explicitly select an agreed
OBR-18 or OBR-19 placer-field mapping in machine configuration (231 sections
7.3.1.18–19; 251 sections 7.4.1.18–19). Neither field is given an implicit sample
meaning here. Literal sample bytes are escaped, retain leading zeros and case,
and are never substituted for order identities. Namespace metadata, including
universal IDs where supplied, is preserved. Duplicate local or universal order
identities are rejected before returning an outgoing message.

ORR parsing checks MSA-2 against the sent ORM and correlates each returned order,
including ORC/OBR identity agreement, filler IDs and duplicate replies. Optional
OBR, order notes and coded rejection reasons are retained with their order;
original error/patient/header context remains in the complete borrowed message.
Order status is retained as the peer's snapshot without changing the request's
outcome or any host lifecycle. Unsupported response segment families are rejected,
not flattened. This is a declared laboratory subset, not a complete ORR profile
validator or support for other dietary/pharmacy/supply order details.

Source-named synthetic tests cover both editions, positive and negative outcomes,
missing decisions, malformed/cross-order identities, unknown controls, byte-exact
sample identities, output bounds and replay purity. There is no storage, transport,
automatic retry or lifecycle machinery in these APIs. No simulator/real-machine
evidence is claimed. The host must retain uncertain delivery and prior acceptance.

### Non-LAW OML/O21 and ORL/O22

`order::oml` reuses the same caller-supplied `Order`, identity, sample, code,
priority and timing types for HL7 2.5.1 sections 4.4.6–7. It renders one SPM per
new order, standard SPM-2 sample identity and TQ1 timing before OBR. Legacy OBR
sample/source/timing fields are not populated; `Specimen::field` applies only to
ORM. Cancellation needs no invented specimen or container. Containers are not
generated from sample IDs. Source codes requiring a CWE version must receive it
through `Code::with_version`; CE destinations explicitly reject that metadata
rather than discard it.

`OrderResponse::new` selects ORR/O02 or ORL/O22 from the private family of the
sent message; it cannot fall back to another transaction or to LAW O34. It reuses
the same exhaustive controls, outcomes, message/order correlation, duplicate
checks and `NotReported` behavior. O22 decisions follow its patient response
group. Timing stays with its order, specimens stay with their order and returned
SAC containers stay with their specimen. A returned specimen must match the sent
placer specimen identity where one was provided; no namespace is inferred from
MSH. Extra specimen context in cancellation replies stays peer-reported data.
Original header context and additional raw fields remain available to the host.

The implementation reuses the borrowed segment/control views used by LAW and its
generic `FieldRule` engine, without applying the LAW profile's restrictions.
For example, base SPM-2 retains its 80-byte limit rather than LAW's extension.
Sample spelling, case, leading zeros and literal delimiters survive escaping.
Named synthetic positive/negative and property tests cover wire placement,
identity conflicts, specimen/container ownership, bounds and unknown controls.
Evidence remains documented; no simulator or real-machine run is claimed.

This is the single-specimen, single-window subset expressible by the current
order types. Multiple returned specimens per order, repeated timing/TQ2 and
associated observation groups are rejected explicitly. Additional returned fields
are preserved as raw views for host interpretation, not claimed as fully validated
clinical content. The APIs do not store, resend or change lifecycle state.
Checklist G is not yet claimed complete; its audit is separate work.

