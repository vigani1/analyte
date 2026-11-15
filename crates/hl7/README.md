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

## LAW implementation status and simulator evidence

The `law` module provides a borrowed LAB-27 QBP query view and RSP rendering.
It checks the message triple, first transaction profile, NE/AL mode, segment
order/cardinality, query scopes, required/forbidden parameters, bounded IDs,
query tags and immediate response mode. Tests use synthetic data and proptest.
This is partial implementation, not LAW conformance: field/component usages,
exact query descriptions, location coding, option negotiation and complete
header validation still need coverage. Do not use this view as a complete
profile acceptance decision.

Not yet covered: restricted negative
LAB-28; AWOS downloads, cancellations and ORL correlation/outcomes; LAB-29
whole-message validation, typed results and scoped enhanced content; distinct
LAW error classes and response rendering; full Appendix C predicates; capability
negotiation; lifecycle and storage integration scenarios. None of the grouped
LAW checklist rows is claimed complete by the query tests alone.

For a future Gazelle run, retain a de-identified recording with exact message
bytes, direction, relative timing and outcomes. Record the tool build, validation
profile identifier and revision, PaLM TF revision, Analyzer Manager actor,
selected options and peer configuration. List each scenario and its validation
report: all query forms, genuine empty work versus unavailable lookup, multi-AWOS
acceptance/rejection, duplicate AWOS after uncertain acceptance, cancellation
races, unsolicited results, corrections/reruns, inconsistent result rejection,
selected enhanced content, lost ACK, disconnect, restart and storage failure.
Record changes to the validation profile between runs. Network exchanges belong
to the host/dialog integration, not this pure module. No Gazelle run has been
performed; evidence remains documented and partial.

### Step 1: LAB-27 responses

`query_response` renders correlated RSP/K11 with exact QPD echo and error
locations, reversing addresses through the existing acknowledgment renderer.
Positive responses require stored-and-interpreted responsibility. Work and a
genuine empty cache both acknowledge query processing with OK; the explicit
followup distinguishes a work download from the restricted negative LAB-28.
Unavailable lookups use an error response and never request a negative download.
Output is bounded before each append. Tests cover these decisions, correlation,
header reversal, error locations, storage refusal and output limits.

### Step 2: LAB-28 work and response handling

`work_order` builds one or more AWOS with caller-supplied stable IDs, specimen,
container and test/battery. Duplicate IDs within one download are rejected.
Replicates use separate AWOS. Optional priority, dilution, manager repeat and
automatic repeat/reflex permissions come only from explicit instructions and
require matching peer capabilities. `negative_work` emits only MSH/SPM/SAC/ORC,
with NULL specimen type, unknown role, query identification echoes and DC.
`WorkResponse` separates message acceptance from per-AWOS outcomes, checks
correlation, duplicate/unknown/missing responses and control/status consistency.
It does not mutate prior AWOS acceptance. Complete schema predicates remain
part of the profile-validation step.
