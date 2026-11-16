# POCT01-A2 Observation Reviewer

Evidence level: **documented**. The crate provides Basic Profile components
as a pure library: bounded XML syntax and typed messages, reviewer builders
and observation duplicate comparison. Conversation state and deadlines live in
`analyte-dialog::poct`; stream boundaries live in `analyte-framing::poct`.
The `Dialog` and action names below refer to that dialog module. Common-model
conversion lives in `analyte-mapping`. Tests use independently written synthetic messages. There is no real
machine recording or official simulator evidence, so this is not a simulated or
proven driver.

## Host contract

- Supply complete XML messages within configured byte limits. Transport,
  stream-message boundary handling, clocks and durable storage belong to the host.
- Journal every raw receipt in the protected store, including invalid, deferred
  and unresolved input. Record driver/config versions there. Parser errors and
  Debug output do not contain document payloads.
- Call `Message::parse`, then `Dialog::step` with monotonic elapsed time. Inspect
  Hello capabilities before supplying `HelloDecision`. Optional topics are read
  and preserved, not automatically enabled. Unknown device timeout/receive limits
  cause Hello rejection; an omitted DCP uses an explicit host-configured budget.
- Execute actions in order. `Store` contains a session-scoped commit ticket and
  the exact message. Report `Stored` only after the raw receipt and semantic
  outcome are durable. A parked semantic failure can report `ApplicationError`;
  a disk/reserve failure reports `Failed`. Never report success merely because a
  store operation was started. Hello, Device Status and remote END also use this
  custody gate.
- Honor `ReadEnabled(false)` using bounded transport buffers and TCP flow control.
  `Deferred` returns ownership of input that could not be accepted. Do not drop
  it. Out-of-order commit completions never reorder ACKs. EOT is not ACKed and
  topic completion waits for the preceding custody decisions.
- Build each `Send` with its emitted control ID and a host-supplied creation
  timestamp with UTC offset. Pass the smaller of local and
  `outbound_message_limit()` bounds to `encode`. Failed writes/builds must cause a
  disconnect; they never imply successful transmission. Write latency must fit
  inside the issued action's deadline. Builders escape XML and validate output
  with the same structure rules as received messages.
- The dialog requests observations after the status ACK, even if the announced
  count is zero. On `TopicComplete`, the host can request the topic again or end
  normally. Expired deadlines close the connection without replaying a send.
- A closed/dropped dialog does not cancel store tasks. Reconcile unresolved
  tickets from `Close` with durable storage. Reconnection needs a fresh host
  session ID. A late completion never produces an ACK in the old conversation.

Invalid Hello syntax with a complete, unique header control ID is parked before
an error ACK and abnormal Terminate (Appendix B section 4.1.2, p.78). Recovery
never guesses an ID from body text or an ambiguous header. Uninterpretable
capabilities follow the same retained-error-ACK-Terminate order; a plain host
policy rejection remains distinct. The dialog waits for the correlated final
ACK or its existing deadline. Program tests:
`poct01_b4_1_2_program_rejected_hello_ack_then_terminate_keeps_raw` and
`poct01_b4_1_2_program_ambiguous_hello_header_is_parked_without_guessed_ack`.

Received control IDs are conversation-local protocol identities (Appendix B
section 5.13, printed p.99). The dialog retains all of them within an aggregate
ID-byte budget equal to `DialogConfig.message_bytes`; nonempty IDs also bound
the entry count and collection overhead. It never evicts IDs. Reuse is parked
before Escape and abnormal Terminate, without mapping a second result. Budget
exhaustion is separately parked and reports inability to complete the topic.
A unique correlated terminal ACK remains usable at capacity; bad traffic cannot
restart termination indefinitely. A new connection has a new index. Tests:
`poct01_b5_13_program_reused_control_id_parks_evidence_and_escapes`,
`poct01_b5_13_reused_id_before_commit_is_retained_not_accepted`, and
`poct01_b5_13_control_id_budget_ends_conversation_without_forgetting_ids`.
This set does not replace the store's independent result identity history.

## Identity and mapping

The pure mapper returns unresolved identity because it has no installed machine
policy or retained Hello. The program opts in explicitly through
`poct.identity = { sequence_number = true, processing = "Production" }`.
Omitting that table keeps the conservative unresolved behavior; both fields are
required when the table is present. Processing mode is an explicit installation
setting, never inferred from a missing source field.

The configured identity is scoped by machine and the device ID from its complete,
parsed, durably stored Hello (Appendix B section 5.5, p.95). The source analyte
code/system and the exact `SVC.observation_dttm` identify the observation; when
`sequence_number = true`, `SVC.sequence_nbr` is also required. Ordered run values
use the shared mapping layer's bounded, length-prefixed encoding. Sequence is
never used alone: it may wrap (section 5.21, Table 46, p.105). The common model's
namespaced scope slot holds `poct-device` and the device ID; it does not assert
that the device sent an order. Missing declared evidence stays unresolved and
parked. The original XML and source positions remain protected evidence.

The program uses only the store's transactional identity/revision mechanism:
same identity and clinical content is a retransmission; changed content with
`SVC.reason_cd = EDT` is a correction candidate; changed content without that
evidence is a parked conflict. `RES` is a resend candidate, not permission to
change content. These reason codes are Table 49, p.105; Table 48's
`SVC.status_cd` describes normal/override conditions. Neither reason nor receipt
headers creates clinical content; source offsets locate retained evidence and
are excluded from clinical comparison. Older stored digests are compared against
retained typed evidence on a miss, bounded by `page_rows`, without rewriting it.

The standalone, bounded `DuplicateIndex` remains a library API for other hosts;
it is not durable storage and the program does not use it as a second mechanism.
The program recovers Hello context from complete interpreted receipts, excluding
transport-journal fragments, for both live receive and operator reprocessing.

Program/reference-receiver regression:
`poct01_b3_2_program_lost_ack_reconnect_restart_and_correction` in
`crates/analyte/tests/vendor_receive.rs` proves a real delivery, lost-ACK resend,
reconnect, restart, edited revision, changed-original conflict, and missing
time/declared sequence/device evidence. Library duplicate tests alone do not
prove those program behaviors (Appendix B section 3.2, pp.71–72; ADR 0003).

Mapping preserves original numbers, codes, code systems/versions, units, flags
and source times. Source paths retain one-based XML element positions, including
distinct positions for repeated services and observations; these are provenance,
never run identity. Patient ID is separate from specimen ID. Patient, liquid QC,
electronic QC, calibration, calibration verification, proficiency and unknown
roles remain explicit. Common-model summaries are conservative: POCT acceptance
is not clinical finality; an unknown role stays unknown; dilution application is
unknown unless separately established. Null values remain typed POCT nulls and
are represented as unsupported common-model values requiring parking. No unit
conversion or clinical interpretation is performed. Raw XML and the typed
message retain metadata beyond the common model's summary.

## Basic Profile checklist evidence

This is a source-to-test index, not a complete conformance claim. The audited
completion gates and outstanding evidence are in
[the protocol checklist](../../../docs/protocols/poct/README.md). Component tests prove
only their own boundaries; program tests are identified explicitly.

Section references are to Appendix B of POCT01-A2. Tests are in `tests/`.

| Checklist row | Implementation and tests |
| --- | --- |
| Mandatory receives and reviewer sends, §4.4 | `messages::poct01_b_6_mandatory_message_bodies_and_control_ids`; `builders::poct01_b_6_reviewer_builders_round_trip_and_correlate` |
| Hello accept/reject and ordered startup, §4.1.2 | `dialog::poct01_b_4_1_2_startup_and_rejection_in_sequence`; `poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline` |
| Capabilities and repeated topics, §§4.1.3,5.6–5.7 | `Body::Hello`, `Capabilities`; `dialog::poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination` |
| Patient/nonpatient, zero/multiple observations and EOT, §§4.1.4–4.1.5 | `messages::poct01_b_6_10_patient_and_nonpatient_values_are_explicit`; dialog zero/multiple transfer tests; mapping tests |
| Custody gate and independent observation identity, §3.2 / ADR 0003 | `dialog::poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`; program `poct01_b3_2_program_lost_ack_reconnect_restart_and_correction` |
| Error ACK, Escape and termination, §§3.4,4.1.11–4.1.12 | `dialog::poct01_b_3_4_error_ack_keeps_topic_but_storage_failure_escapes`; Escape correlation, remote END and normal termination tests |
| Application timeout without replay, §3.6 | `dialog::poct01_b_3_6_no_resend_on_application_timeout`; Hello policy latency test |
| Required objects, order, cardinality, codes and correlation, §§5–6 | `structure.rs`; missing/duplicate/order/role tests in `messages`; correlation tests in `builders` and `dialog` |
| POCT coded/quantity/time/null/encapsulated types, §8 | `data.rs`; quantity-spelling property, null/code/translation, offset/precision, interval and encapsulated metadata tests |
| Local normative structure handling, §12 | Executable rules in `structure.rs` and fixed data-type metadata checks; auxiliary metadata, invalid fixed attributes and interval-choice tests in `messages` |
| Asynchronous observation order and bounded backpressure, §9 | `dialog::poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`; `poct01_b_9_bounded_backpressure_returns_unconsumed_input` |
| Project failure gate | Minimum-capability startup, malformed XML/typed-input properties, backpressure, timeout, store failure, disconnected/unresolved tickets, lost-ACK/restart duplicate restoration and same-ID conflict tests |

The source's message models in §6 define object order/containment; §8 supplies
value semantics and §12 supplies element/data-type metadata. In particular,
`OBS.critical_lo-hi_limit` uses the declared element spelling, consistent with
the patient model and the data-element declarations. No DTD, standard example or
vendor manual is distributed here.

## Optional extensions

**None implemented or advertised:** Device Events; complete/incremental patient
and operator lists; directives; keep-alive; continuous mode; set time; vendor
message topics/directives. Reading advertised capabilities or preserving an
encapsulated vendor field does not claim those extensions. The Basic Profile
engine tolerates asynchronous observations without requiring a profile flag to
enable it; this does not enable continuous/unsolicited operation.

Physical Access Point (Appendix A), the Device actor and HL7 Observation Reporting
Interface (Appendix C) are outside this crate's claim. Host TCP/store integration
is tested in `crates/analyte/tests/vendor_receive.rs`; Linux and fuzz execution
are tracked by CI;
the crate tests themselves perform no analyzer or lab-app I/O.

## XML policy

Bounds cover complete-message bytes, nesting depth, total elements and encoded
attribute values. Parsing and containment walks do not recurse. DOCTYPE is
rejected; unresolved entities are rejected; external references are inert values
and never resolved. Base64 and compressed content stay encoded without expansion.
UTF-8 XML 1.0 (including a BOM) is supported. The vocabulary uses ASCII names and
no namespace prefixes, except the standard `xml:lang` metadata attributes. Exact
raw bytes are kept beside decoded values, including their entity spelling.

## Local verification

The local verification gate is formatting, strict workspace Clippy, all-feature
workspace tests and cargo-deny with the pinned toolchain. CI separately runs
Linux, the minimum supported Rust version and fuzz targets; local Windows checks
do not prove those runs or interoperability. No dependencies beyond the
approved quick-xml, thiserror, analyte-model, jiff and test-only proptest are used.


## Program receive integration

The `analyte` program drives this crate over configured connections. Incoming
bytes are journaled with driver/config versions before the dialog sees them;
complete receipts and interpretations are then stored through the protected store.
Unresolved result identities remain unresolved, are parked with their reason,
and create no app delivery. Inspect them with `analyte parked list`.
The program uses only the configured composite device/time/analyte identity
described above, never sample/value/receipt heuristics or a timestamp alone.

VIDAS and Autolyser work-list sending is implemented; see
[program orders](../../analyte/ORDERS.md).
POCT optional order/list extensions remain outside the supported profile.

Synthetic TCP tests in `crates/analyte/tests/vendor_receive.rs` exercise custody,
parking, operator visibility, storage failure, rejected Hello responses and
control-ID reuse. The POCT program/reference-receiver identity test proves real
delivery, retransmission suppression and an edited revision. These tests do not
raise the documented evidence level or close the unchecked conformance gates.

Common-model observation conversion and configured code/identity mapping now live
in `analyte-mapping` (`poct::map_observations` and `map_poct`). The mapping tests
are in `crates/steps/mapping/tests/poct/mod.rs`, with their original assertions unchanged.
This crate retains POCT syntax and source-evidence duplicate comparison.

Conversation state and deadline policy live in `analyte-dialog`, in `src/poct.rs`.
Conversation tests live in `crates/steps/dialog/tests/poct.rs`.
