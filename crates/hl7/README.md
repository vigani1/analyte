# HL7 syntax crate

Evidence: **documented**, using HL7 2.3.1 chapters 2, 4 and 7; HL7 2.5.1
chapters 2/2A, 4, 5, 7 and 13; and the LAW references identified below.
Fixtures are original synthetic data.

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

Not implemented here: complete LAW, MLLP, general application message structures, unprofiled clinical
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

- Full LAW conformance beyond the implemented Appendix C field profiles:
  cross-message enhanced-content relationships, lifecycle transitions and every
  message-group predicate. Specimen-level observations remain separate from
  AWOS results; related-observation sending and reference resolution are absent.
- Related observations and full capability/profile negotiation.
  Related-observation and pooling message workflows are not implemented;
  the outside-pooling field predicates can be validated explicitly.
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

## LAW patient and visit fields covered (group 2)

- **PID:** 3, 5, 7, 8, 10 and 35. Fields 3/5 are required singletons,
  with an explicit null permitted when not applicable. CX.1 retains the
  patient ID (50 characters); CX.4 is an available HD with namespace and
  universal identity limits. HD.3 is required when HD.2 is present and X
  otherwise. A mixed local/universal authority must match a trusted
  `AuthorityBinding`; unresolved associations return an error. XPN supports
  surname (FN.1), given names, additional given names, suffix and mandatory
  legal-name marker XPN.7. The 20#/10# name primitives are kept in full.
  PID-7 accepts a date or a date/time to seconds without a timezone. PID-8
  accepts the LAW sex subset. PID-10 CE and PID-35 CWE check their required
  identifiers/namespaces, optional descriptions and component limits using
  the coding systems profiled by C.9.6/C.9.7.
- **PV1:** 2 and 3. Patient class is a required singleton from the C.10
  subset. Location is an available singleton; only PL.2 (room) is supported
  and is required when the field is valued. Its 16# text is kept in full.

Other PID/PV1 fields (including PID-11/XAD), unlisted composite components
and unlisted FN subcomponents are X: omitted on send, ignored but retained
on receive. Unsupported whole patient/visit segments follow the same rule
when the patient option is disabled. Required/available usage, cardinality,
primitive/nested datatype shape, nulls and character lengths are checked by
`Profile::validate_segment`. `Profile::for_peer` selects send options;
`Profile::receive` supports enhanced incoming content without assuming peer
capabilities. The profile-aware work/results/ORL APIs accept trusted
host-supplied authority bindings; the existing APIs fail visibly when an
association cannot be established. No patient-record reconciliation is done.
The positive/negative source cases are in `tests/law_patient.rs`.

## LAW specimen and container fields covered (group 3)

- **SPM:** 1, 2, 3, 4, 7, 8, 9, 11, 13, 16, 17, 18 and 27. Set ID is
  a positive four-character SI and is checked against message order. The
  available specimen ID and repeatable parent IDs use EIP.1/EI: required
  entity ID, namespace or universal-ID/type pair, primitive limits and the
  80-character enclosing field limit. Other EIP components/subcomponents
  are X. Type and role are mandatory CWEs with required identifier/system
  and available description; type is explicit null for control/unknown
  roles, and an analyzer may also report a null type when not applicable.
  Manager patient/pooled work requires a valued type. Unknown role U belongs
  only to the restricted negative query message, using IHELAW. The
  LAW_SPECIMEN fields 7/8/9/16/27 check singleton CWEs with the C.12
  standard/vendor namespaces and component limits. Fields 17/18 check
  collection DR.1/TS.1 and receipt TS.1 at seconds precision, without zone
  or precision components. The source-site modifier and risk are singletons
  as specified by C.12-1. SPM-3 and SPM-13 resolve outside-pooling R/X
  predicates, including required role L and positive integral grouped count.
- **SAC:** 3, 4, 9, 10, 11, 13, 14, 15, 21, 22, 24 and 29. Container,
  parent, carrier and tray identities use EI.1 with its non-truncatable
  50-character limit. Container or parent must be supplied; a parent-only
  manager send also needs a carrier/tray location. Analyzer container ID
  may be explicit null when unavailable. Manager carrier/tray IDs require
  their corresponding NA position; absent IDs make those position fields X.
  NA validates its required integer first axis, optional numeric axes, each
  16-character primitive and the 80-character total. Location is a repeatable
  CE with required identifier/vendor system and available description;
  manager sends require a carrier or tray. LAW_CONTAINER gates the available
  carrier-type CE, numeric container/specimen volumes, volume-unit CE and
  dilution SN. Units require description, and UCUM exactly when an identifier
  is valued. Dilution supports only the profiled 1:n ratio with a positive
  denominator; no multiplier is inferred or applied.

Other SPM/SAC fields and unlisted components are X. The sender role matters:
SPM-7/8/9/16/17/18/27 and SAC-9/21/22/24/29 are X in the analyzer column, so
receive validation preserves and ignores them even when the manager option
is supported. Analyzer carrier/tray/position/location fields are RE rather
than the manager's stronger send predicates. SPM-3 is X in an outside-pooling
LAB-29 (section 2.2.4.17); OML/ORL still require its parent list. Outside-pooling
patient groups are X. The content validator can select that option explicitly
with `Profile::with_pooling_outside`; it does not add pooling authorization or
pooling support to the work builder. Full pooling message workflows remain
unimplemented.

`Profile::validate_message` resolves the restricted negative-query exceptions
only for MSH/SPM/SAC/ORC with role U and order control DC, retaining each query
scope's exact available identity/location echoes rather than fabricating an ID.
Work, negative-work, results and ORL paths apply profile validation. Specimen
and container option gating does not suppress required base identity/location
information. All supported hard lengths count characters, with ignored X
positions excluded on receive and encoded trailing positions counted on send.
There are no additional # lengths in C.11/C.12. Positive/negative source tests
are in `tests/law_specimen.rs`, including all six negative-query scopes and
actual build/receive cases. These field validators do not establish full LAW
message-group, lifecycle or simulator conformance.

## LAW order, timing and test-detail fields covered (group 4)

The exact supported positions from Appendix C.6/C.8/C.13/C.14 are:

- **ORC:** 1, 2, 4, 5, 8, 9, 21 and 27. Order control is a closed
  `OrderControl` enum with transaction/direction restrictions; DC belongs only
  to the restricted negative query. Analyzer order status is a closed
  `OrderStatus` enum, with the ORL control/status/request matrix checked
  exhaustively. The manager excludes 2/5/8/27. Analyzer ORL requires EI.1 in
  field 2; OUL excludes it. Group EIP.1/EI.1 is available and singleton;
  analyzer reflex parents use the same supported identity and may repeat.
  Other EI/EIP components are X. Manager event time is mandatory TS.1 to
  seconds; available analyzer field 27 permits a date or seconds timestamp.
  Both exclude timezone and precision. Available ordering facility uses
  XON.1; its 50# name is retained in full. `Awos::with_transaction_time`
  supplies an earlier initiating event; otherwise the caller asserts that
  the response timestamp also identifies the initiating event.
- **OBR:** 2, 3, 4, 11, 16 and 49. Required AWOS ID and available analyzer
  filler ID use EI.1 with a hard 50-character limit; explicit null AWOS is
  accepted only from an analyzer. Mandatory test CE supports required
  identifier, description and coding system (LN, JC10 or a vendor namespace).
  Reflex action is analyzer-only, available when selected and restricted to
  G. Available manager ordering provider supports XCN.1, FN.1 in XCN.2,
  XCN.3/4/5 and nested HD in XCN.9. Hard ID/authority limits, HD.3's
  universal-ID predicate and trusted mixed-authority bindings are checked;
  name primitives marked # are preserved beyond their nominal length.
  Result handling is absent or RE. C.6.1's manager-column RE for OBR-49
  conflicts with the subsequent prose's X; this implementation follows the
  manager column. Analyzer field 49 depends on LAW_RESULT_EXT.
- **TQ1:** 9 only. LAW_PRIORITY gates the whole segment. Its mandatory CWE
  supports required identifier/system, available description and the C.14
  limits. The standard system is HL70485 with R/S; vendor systems may use
  their own code. An explicitly selected receive priority option requires
  timing in each reported order group; manager timing remains available.
- **TCD:** 1, 2, 3, 5, 6, 7, 8, 9, 10 and 11. Required test CE uses the
  OBR test subset and must match OBR-4 on send or OBX-3 on receive. Dilution
  fields 2/3/5 check the profiled SN components and positive 1:n denominator;
  only manager field 2 also permits deferred + or - with no denominator.
  Fields 3/5 are manager-only. LAW_DILUTIONS gates these fields and field 11,
  whose CWE has a required identifier/vendor system and optional description.
  Explicit selection requires a TCD with factor or type. When both are
  present, a trusted test-specific `DilutionBinding` must establish their
  consistency; unknown associations and conflicts fail with a located error.
  Bindings preserve exact factor spellings and do not derive a number from
  a dilution code. Manager rerun/reflex permissions (6/7) require Y/N when
  LAW_AM_RR_CONTROL is selected. Repeat status (8) requires the C.13
  O/R/D/F CE subset when LAW_AM_RR is selected. These manager fields are X
  on receive. C.13.1's R columns govern selected 6/7/8 despite the following
  descriptions using RE. Available manager consumption (9) checks CQ.1 NM
  and CQ.2 CE with required unit identifier, optional description and UCUM.
  Analyzer pooling option selects mandatory positive integral NM field 10.

Other fields and unsupported components/subcomponents are X: omitted on send,
ignored but retained on receive. All listed fields are singletons except
ORC-8. Required/available usage, nulls, primitive datatypes, character limits,
nested components and encoded send ceilings are validated at the existing
parser boundary in both directions. The supported SN and CQ shapes are typed
through `TestDetail`; no observation value is calculated. Original numeric
spelling and dilution-type bytes remain accessible. `DilutionMeaning`
distinguishes current-run requests, rerun requests, prediluent content and
reported factors. `ResultAdjustment` records AlreadyApplied, NotApplied or
Unknown from the host's explicit analyzer contract, including type-only
reports. Unknown reported adjustment is flagged for host reconciliation.
The presence of a dilution factor never proves that a result was multiplied.

Work builders, ORL acceptance and received results apply these profiles.
`Profile::receive` accepts supported enhanced content without assuming an
unprovided option negotiation; explicit option setters enforce selected
requirements. Pooling field validation adds no pooling workflow. Positive and
negative source cases and build/receive integrations are in
`tests/law_order.rs`. The earlier group fixture containing an EI namespace in
ORC-4 was corrected: C.8.4 supports only EIP.1/EI.1 there. Evidence remains
**documented**; complete message structure, lifecycle and simulator
conformance are still outside the field coverage claimed here.

## LAW observation, comment and inventory fields covered (group 5)

All five segment groups now have field validators. This is documented field
coverage, not a qualified LAW implementation or a simulator result. The exact
supported group-5 positions from Appendix C.2/C.5/C.7 are:

- **OBX:** 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 18, 19, 21 and 29.
  Set ID is positive SI with order-local sequence checks. Value type is
  mandatory for a valued observation and X for explicit null. Observation
  CWE has required primary identifier/description/system, optional first and
  second alternate identifiers, and text/system required when each alternate
  is supplied; other components are X. Standard/vendor namespaces and the
  IHELAW supplemental-code subset are checked. Analyzer OG run ID is mandatory;
  manager OG is available. Positive group/sequence components have five-character
  limits and become required for repeated primary observation identities within
  an order. Descriptions do not change that primary identity.
- **OBX-5:** mandatory value or explicit null, never a deletion instruction.
  CE, ED, EI, NM, NA, RP, SN, ST and TX have their C.7 component shapes and
  character limits checked. ED includes namespace-only source HD, datatype,
  subtype and ASCII/Hex/Base64 encoding checks; RP includes bounded pointer,
  nested HD and optional vendor type/subtype. No pointer is retrieved.
  SN follows C.7-11's required comparator column (its empty-comparator examples
  conflict with that column); numeric components are at most 15 characters.
  Only IHELAW S_RAW/S_OTHER supplemental values can repeat. The 99999-character
  enclosing value ceiling and parser's configured byte/repetition bounds apply.
  `Value::raw` preserves every original encoded byte. NM accepts the profiled
  decimal grammar; `<0.1`, `1,234`, text and error tokens remain exact ST/TX
  data when declared as text. Invalid NM is rejected while the original message
  remains available, rather than guessed or normalized.
- **Remaining OBX fields:** units (6) are mandatory for NM/SN and otherwise X;
  explicit null represents unitless numeric data. Unit description is required,
  identifier is available and UCUM is required exactly when the identifier is
  valued. Original units are retained without conversion. Available range (7)
  is a bounded ST. Interpretation (8) is a repeatable CWE or explicit null.
  C.7-1's five-character length cannot hold C.7-17's required CWE namespace;
  the explicit CWE component profile governs. Analyzer-only probability (9)
  is available under LAW_MASS_SPEC and checked in [0,1] using exact decimal
  spelling, without floating-point conversion. Result status (11) is the
  exhaustive X/P/R/F/C `ResultStatus`; only declared C means Corrected.
  Changed values do not infer correction. Manager-only collection time (14)
  and required analysis time (19) use seconds TS.1 without timezone/precision.
  Required observer (16) repeats XCN.1, permitting explicit null observer ID.
  Required equipment (18) has at least model/manufacturer and serial/manufacturer
  repeats; only the first may carry UID and ISO type. Further vendor repeats
  have optional entity/namespace and excluded UID components. Available instance
  ID (21) supports EI.1 only. Observation type (29) checks AOE/ASC/RSLT/SCI.
- **NTE:** 1, 2, 3 and 4. Set ID is positive four-character SI, starting at one
  in each consecutive comment block. Source is A for manager output and Z for
  analyzer input. Comment is one required FT; its 120# length permits retaining
  longer comments within the message bound. Available comment-type CE requires
  identifier/vendor namespace and permits description. C.5-3's extended 99zzz...
  namespace is accepted up to 12 characters. Notes remain human-readable data.
- **INV:** 1, 2, 3, 4, 12 and 16, analyzer-only. Required substance identifier,
  status and type use CE with required identifier/system and optional description.
  Available container CE uses a vendor namespace; expiry uses seconds TS.1;
  lot number is available ST. Section 2.2.3's key conformance limits govern:
  INV-1's identifier and INV-16's lot number are each at most 50 characters,
  overriding C.2's datatype lengths. Standard status/type codes and vendor codes
  remain distinguished by namespace. A control-container INV requires CO in
  HL70384; contributing inventory attaches to its result. Explicitly disabled
  contributing-substance content remains retained as raw data.

All other fields and unlisted components are X: omitted on manager send,
ignored and preserved on receive. Nulls, repetitions, hard component lengths,
send composite ceilings and option predicates are validated with located errors.
Received results and work-note builders use the same boundary rules. The general
profile API also validates manager-authored OBX segments; there is still no
related-observation work builder. The 28 source-named positive/negative tests in
`tests/law_observation.rs` cover field rules, value/unit custody, explicit
corrections, composite boundaries, inventory ownership and actual note building.
No earlier test fixture or expectation was changed in this round.

The [LAW checklist](../../docs/protocols/ihe-law.md) distinguishes implemented
field evidence from partial or planned workflow tests. Still absent are a
qualified Gazelle run, complete option/profile negotiation, related-observation
sending/reference resolution, full pooling workflows, external-pointer retention
integration, and LAW-specific host/dialog durability/recovery/lifecycle evidence.
Evidence remains **documented**.

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
Base SPM-2 retains its declared 80-byte limit independently of LAW profile
direction, option and authority predicates.
Sample spelling, case, leading zeros and literal delimiters survive escaping.
Named synthetic positive/negative and property tests cover wire placement,
identity conflicts, specimen/container ownership, bounds and unknown controls.
Evidence remains documented; no simulator or real-machine run is claimed.

This is the single-specimen, single-window subset expressible by the current
order types. Multiple returned specimens per order, repeated timing/TQ2 and
associated observation groups are rejected explicitly. Additional returned fields
are preserved as raw views for host interpretation, not claimed as fully validated
clinical content. The APIs do not store, resend or change lifecycle state.
Checklist G is not fully complete; its audited scope and remaining work are in
[the HL7 checklist](../../docs/protocols/hl7.md#g-orders-and-lifecycle).


## Order/query checklist audit

The G/H rows in [the protocol notes](../../docs/protocols/hl7.md) now identify
passing tests for checked declarations and list the remainder for unchecked rows.
Conditional checks cover only selected capabilities. The generic order API is
new/cancel work, one specimen/test per order and one timing window; LAW's broader
profile coverage is documented separately above and in its own checklist.

Implemented evidence includes the two ORM/ORR editions, non-LAW O21/O22,
message/order/specimen correlation, explicit controls/outcomes, retained literal
sample IDs, legacy Q02/Q03 field validation and the separate LAW query contract.
Unsupported families, subscriptions, enhanced query forms and continuation are
rejected by the typed interfaces without constructing a successful empty answer.
The new LAW field-rule error variants retain their original detailed causes in
`order::Error::FieldRule`; no LAW validation implementation is copied into the
non-LAW module. Duplicate/extra legacy query segments report their own known
segment location rather than incorrectly calling every extra segment DSC.

Still open: generic parent/group/change workflows, full optional order-content
predicates and associated O21 observation/instruction groups, immediate Q01,
general non-LAW query conformance statements and quantitative/tabular forms,
continuation/fragmentation, and host tests for uncertain delivery, deadlines,
cancellation/restart and durable recovery. Unselected TQ2, O35/O36 and query
subscription/replay capabilities are not implied by the supported subset.
Pure replay tests prove deterministic bytes and stable identities; they do not
prove lost-ACK or restart behavior. Additional display units/escaping and local
filter interpretation remain as described above.

No simulator or real-analyzer evidence was added. Evidence remains
**documented**; the checked rows are not a whole-HL7 or whole-system conformance
claim.
