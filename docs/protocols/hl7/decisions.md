# HL7 decisions

## Bound the 2.6 receiving profile and refuse ambiguous intent

- **what**: Qualify the existing ORU/R01 core and deferred display-query
  exchange for 2.6. Interpret selected CWE and DTM fields and retain exact
  lexical values. Refuse nonempty XCN-17 and QRF-9, specimen/container groups
  and unselected ORU segments. Keep backward-compatibility OBR fields,
  deprecated MSA information and OBX-21 raw. Refuse nonempty OBX-22.
- **why**: Nested deprecated DR cannot be represented fully. Ignoring a TQ
  qualifier could answer a different query. OBX-21 has no established laboratory
  identity contract here, while mood can change whether an observation is an
  event or an intention. These fields must not silently change clinical meaning
  or start scheduling work. Coding-system versions come from the sender.
  For this bounded 2.6 profile, any populated CWE coding-system component
  requires its version component, even without an identifier. The standard
  permits omission for HL7-defined tables but requires versions for user-defined
  tables; Analyte does not infer table ownership from an `HL7` prefix. Requiring
  an explicit version is a stricter profile choice, not an HL7-wide requirement.
- **source**: HL7 2.6 §§2.A.13/.20/.86, pp.2-115–119, 2-128–129, 2-190–193;
  §5.10.4.2.9, p.102; §§7.3.1, 7.4.2.21–22, pp.7-12–14, 7-50–51.
  Bounded scope is Analyte's selection under ADR 0007, rather than a claim
  that the standard forbids all optional structures.
- **tests**: protocol `receiving.rs`; mapping
  `hl7_2_6_s7_3_1_s7_4_2_3_5_19_21_receiving_keeps_values_times_and_raw_identity`
  and `hl7_2_6_s2_a_6_77_and_s7_4_2_22_unsafe_results_never_map`.
- **remaining**: 2.8.2/2.9.1 typed profiles and their query-scope decision stay
  unfinished and fail config validation. Their original queries are withdrawn
  (2.8.2 §5.10.2; 2.9.1 §5.9.2); a modern query contract is not implicitly
  selected. Step 5 order writers remain a separate PR. Evidence: documented.

## Bound compatibility to the established legacy receiving prefix

- **what**: Receive ORU/R01 and the selected legacy query exchange in 2.3,
  2.4 and 2.5 under the later receiver contract. Preserve the exact edition and
  original bytes. Use the common datatype/query prefix; apply the explicitly
  dated MSH-7, MSG and ERR changes. Enable receiving/query config only with
  tested typed paths and replies; keep command sending gated until implemented.
- **why**: Compatibility guarantees existing meanings, not a chronology of
  every addition. Unknown extensions must not gain later clinical meanings.
  Keep OBR interpreter composites and OBX-19 raw; park unsupported CWE values;
  refuse unverified URI/UTF-8 codes, extended query filters and specimen groups.
  The exact limitations and per-edition sources are in `versions.md`.
- **source**: HL7 2.5.1 §§2.8–2.8.2 pp.2-20–22, §2.15.9.7 p.2-76,
  §2.A.44 pp.2-180–181, §2.15.5.1 p.2-67; 2002-04-08 2.3.1/2.4 errata,
  TS, sequencing, XCN-17 and OBR-32–35 items (unpaginated).
- **test**: protocol `tests/compatibility.rs`, mapping
  `tests/mapping/compatibility.rs`, the errata sequencing datatype test,
  `config/tests/hl7_editions.rs` and dialog `tests/hl7_compatibility.rs`.

## Edition recognition precedes typed qualification

- **what**: Recognize the eight explicit edition IDs in one registry. Keep each
  typed field profile optional until its actual rules have been implemented;
  a missing profile returns `EditionRules` with the edition's static name.
  Do not fill a new row by copying 2.5.1's interpretation.
  Configuration validation refuses a recognized edition without typed support
  using `UnsupportedHl7Edition`; such a machine must not start and park all input.
  Enabling an edition in config is part of implementing its typed support.
- **why**: An exhaustive enum is infrastructure, not conformance evidence.
  This lets registry and configuration work remain buildable while later
  commits add compatibility, message/datatype rules and writer support.
- **source**: ADR 0007 and ADR 0012 §3; HL7 2.5.1 §2.8, pp.2-20–22.
- **test**: the eight edition cases in `version/tests.rs`; existing 2.3.1 and
  2.5.1 conformance tests continue to guard their implemented field rules.
  `adr0012_s3_recognized_editions_without_typed_support_fail_config_validation`
  checks that all six other editions fail before activation with an actionable
  diagnostic naming the edition and the supported alternatives.

## Sender-marked truncation

- **what**: Preserve the complete received spelling and expose whether a primitive
  ends in the declared truncation character. Park messages containing such a
  marker before clinical mapping; never reconstruct the omitted suffix. A literal
  character escaped with `\P\` is not a truncation marker. Analyte never truncates
  outgoing values and escapes literal occurrences when declaring the character.
- **why**: The standards describe both rejection and limited use of truncated
  information, but do not establish that a truncated laboratory value or identity
  is safe to deliver. Parking preserves the evidence for an operator.
- **source**: HL7 2.8.2 §§2.5.5.2–3, pp.12–13, §§2.7.1–2, pp.20–21;
  HL7 2.9.1 §§2.4.5.2–3, pp.14–15, §§2.6.1–2, pp.22–23;
  project safety rules 2 and 5. The 2.8.2 text's ambiguous unescaped-literal
  example does not override its escape requirement or justify guessing.
- **evidence**: documented. `tests/truncation.rs` checks declarations, exact
  retention, marker flags, escaping and size refusal; mapping regression
  `hl7_2_9_1_s2_4_5_2_truncated_input_never_reaches_result_mapping` checks that
  marked input produces a located error before results can be mapped.

The [checklist](checklist.md) owns conformance requirements; the
[version page](versions.md) owns edition support. These entries explain
choices within that scope, not additional promises of conformance.

## 1. Declare laboratory transactions rather than claim all of HL7

- **what**: Implement the laboratory transaction set declared in the scope;
  do not infer support for other transactions from a working segment parser.
- **why**: Parsing a segment does not implement its workflow or response.
  An explicit transaction boundary prevents a machine from being credited
  with support that has only been tested at the grammar level.
- **source**: [ADR 0007](../../adr/0007-standards-first.md), scope definition.
- **test**: none yet.

## 2. Act as the receiving application for store-and-forward

- **what**: Original-mode AA means Analyte durably retained and interpreted
  the message; it does not wait for the destination lab app to receive it.
- **why**: Waiting for an external app would tie the analyzer's response
  deadline to an unrelated network outage. Analyte instead accepts custody
  at its own durable boundary and owns subsequent delivery.
- **source**: HL7 2.5.1 §2.9.2.2, pp.2-26–2-27;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), receiving responsibility.
- **test**: `hl7_251_2_9_2_aa_only_after_store_and_interpret`.

## 3. Separate enhanced custody from semantic acceptance

- **what**: Use raw durable storage for the accept phase and interpretation
  for the application phase; do not treat CA as permission to send AA.
- **why**: Stored but uninterpretable input is recoverable evidence, not a
  successfully processed result. Combining the phases would erase that
  distinction precisely when the sender has already released responsibility.
- **source**: HL7 2.5.1 §2.9.3–2.9.3.3, pp.2-27–2-28;
  [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), HL7 row.
- **test**: `hl7_251_2_9_3_independent_conditions`.

## 4. Agree on encoding instead of guessing it from payload bytes

- **what**: Support configured ASCII, Latin-1 or UTF-8 and check the declared
  character set against the connection setting.
- **why**: Byte sequences can be valid under several encodings with different
  meanings. A successful heuristic decode is insufficient evidence for
  changing a patient's identifier or a textual result.
- **source**: HL7 2.3.1 §2.24.1.18, pp.2-108–2-109;
  HL7 2.5.1 §2.15.9.18, pp.2-78–2-80.
- **test**: `hl7_251_2_15_9_18_charset_is_lossless_and_connection_checked`.

## 5. Require a named departure for legacy UTF-8

- **what**: Accept the later UTF-8 character-set code in 2.3.1 only when the
  explicit compatibility flag has a matching named vendor departure.
- **why**: The older table does not define that specific code. A global
  tolerance would make one vendor's extension indistinguishable from standard
  behavior and hide which installation relies on it.
- **source**: HL7 2.3.1 §2.24.1.18, pp.2-108–2-109;
  HL7 2.5.1 §2.15.9.18, pp.2-79–2-80;
  [ADR 0002](../../adr/0002-config-first.md), machine configuration ownership.
- **test**: `hl7_231_2_24_1_18_utf8_requires_named_vendor_departure`.

## 6. Keep vendor extensions out of the base grammar's meaning

- **what**: Put repurposed fields and other vendor departures in the machine
  definition, rather than changing the common interpretation of a field.
- **why**: HL7 allows local extensions, but their agreement is local. Giving
  one installation's convention a global meaning could silently remap another
  analyzer's standard data.
- **source**: HL7 2.5.1 §2.11–2.11.1, p.2-37;
  [ADR 0002](../../adr/0002-config-first.md), Decision.
- **test**: none yet.

## 7. Bound custody and interpretation separately

- **what**: Give pending storage, interpretation and application-reply waits
  bounded lifetimes, and reject a late completion after its deadline.
- **why**: A disk or interpreter stall must not hold the connection forever.
  A late success also cannot retroactively justify a positive reply after the
  dialog already reported failure.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), bounded waits;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), dialog deadlines.
- **test**: `hl7_251_2_9_deadline_precedes_late_commit`.

## 8. Do not automatically resend an uncertain machine order

- **what**: Keep acceptance and application-response phases distinct; a lost
  response after sending an order leaves an uncertain outcome without resend.
- **why**: HL7 leaves some retry decisions to the application. For a machine
  order, repeating bytes could schedule another test even though the first
  command succeeded, so delivery-style retries are inappropriate.
- **source**: HL7 2.5.1 §2.9.2.2, p.2-26;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle rule.
- **test**: `hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends`.

## 9. Retain malformed input before parking

- **what**: Preserve the raw message even when a usable semantic message or
  successful acknowledgment cannot be constructed.
- **why**: Rejecting a parse must not destroy the bytes needed to diagnose a
  wrong encoding, repair a mapping or demonstrate what the machine sent.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Never drop data
  and Keep the original.
- **test**: `adr_0003_malformed_message_is_committed_before_parking`.

## 10. Assign revisions from trusted identity and history

- **what**: Use the common store's revision policy instead of treating a
  transmission identifier, segment position or changed value as a new result.
- **why**: A correction can arrive before its original, and a normal status
  progression can change content without being a correction. Only the retained
  identity and history can distinguish those cases from a retransmission.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), Three identities;
  [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `adr0003_revisions_new_retransmission_correction_conflict_runs_and_restart`.
