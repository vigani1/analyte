# POCT01-A2 Observation Reviewer

Evidence level: **documented**. The crate implements the mandatory Basic Profile
as a pure library: bounded XML syntax and typed messages, reviewer builders,
conversation state machine, observation duplicate comparison and common-model
mapping. Tests use independently written synthetic messages. There is no real
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
  `outbound_message_limit()` bounds to `build`. Failed writes/builds must cause a
  disconnect; they never imply successful transmission. Write latency must fit
  inside the issued action's deadline. Builders escape XML and validate output
  with the same structure rules as received messages.
- The dialog requests observations after the status ACK, even if the announced
  count is zero. On `TopicComplete`, the host can request the topic again or end
  normally. Expired deadlines close the connection without replaying a send.
- A closed/dropped dialog does not cancel store tasks. Reconcile unresolved
  tickets from `Close` with durable storage. Reconnection needs a fresh host
  session ID. A late completion never produces an ACK in the old conversation.

## Identity and mapping

A POCT analyte code identifies the test, not its individual run. Service sequence
numbers can wrap. The mapper therefore returns **unresolved** identity rather than
inventing a key from a patient, timestamp, ordinal, value or receipt ID. Host
reconciliation must supply sufficient trusted source evidence for the common
model's observation identity before result recording/delivery. The store assigns
revisions atomically from retained identity/content history under ADR 0003; this
crate never derives a revision from a source status or service sequence.

`DuplicateIndex` keys on configured device plus trusted observation identity and
revision. Comparison evidence omits message headers, retransmission reason and
sibling observations, while retaining the observation and its service context.
Attribute order and object indentation do not create new observations. Changed
content under the same trusted identity/revision is a conflict and never replaces
prior evidence. New revisions and independently identified reruns remain separate.

The index is bounded and never evicts history. The host must compare and persist
identity/evidence atomically with the result before reporting custody. Rebuild
from durable records before accepting observations after restart; an empty cache
is not evidence that old results are new. A full index requires backpressure or
a protected-store identity lookup, not silently forgetting records. The pure
index itself does not authorize an ACK or provide durable storage.

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

Section references are to Appendix B of POCT01-A2. Tests are in `tests/`.

| Checklist row | Implementation and tests |
| --- | --- |
| Mandatory receives and reviewer sends, §4.4 | `messages::poct01_b_6_mandatory_message_bodies_and_control_ids`; `builders::poct01_b_6_reviewer_builders_round_trip_and_correlate` |
| Hello accept/reject and ordered startup, §4.1.2 | `dialog::poct01_b_4_1_2_startup_and_rejection_in_sequence`; `poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline` |
| Capabilities and repeated topics, §§4.1.3,5.6–5.7 | `Body::Hello`, `Capabilities`; `dialog::poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination` |
| Patient/nonpatient, zero/multiple observations and EOT, §§4.1.4–4.1.5 | `messages::poct01_b_6_10_patient_and_nonpatient_values_are_explicit`; dialog zero/multiple transfer tests; mapping tests |
| Custody gate and independent observation identity, §3.2 / ADR 0003 | `dialog::poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`; all `duplicate` tests |
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
Interface (Appendix C) are outside this crate's claim. Host TCP/store integration,
Linux execution and fuzz-target CI are separate integration gates;
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

On Windows, the four required workspace checks pass with the pinned Rust 1.100
compiler. Workspace tests also pass with the declared Rust 1.95 minimum. Linux
and fuzz-target execution have not been run locally. No dependencies beyond the
approved quick-xml, thiserror, analyte-model, jiff and test-only proptest are used.
