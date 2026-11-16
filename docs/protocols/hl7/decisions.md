# HL7 decisions

## Keep the modern receiving profile bounded without losing source distinctions

- **what**: Receive the same ORU/R01 core as the 2.6 profile for 2.8.2 and 2.9.1,
  using their actual CWE/DTM, withdrawal and acknowledgment rules. Refuse
  withdrawn fields visibly, including OBR-14; it no longer supplies a specimen
  receipt time. Leave unselected participation, device and specimen groups
  outside this profile. Legacy query is refused; LAW remains separate.
- **coding**: Accept all 22 CWE components under the explicit 16,384-byte
  operational composite bound. Treat conformance lengths as receiver capacities,
  not universal maxima; enforce the normative coding-system name bound. Retain
  OID, value-set and alternate-tuple metadata raw, without registry lookup or
  inventing a named namespace. Do not reject a coded value merely because value-set
  metadata is present or absent. The named primary tuple is the current mapping
  contract; OID-only or alternate-only identities stay unresolved for an operator.
  Named coding systems retain the explicit-version policy selected for 2.6.
- **time**: For 2.9.1 `-0000`, retain the full original time and precision but
  leave the optional parsed local time and civil offset empty. The source
  establishes UTC without establishing the sender's civil offset; the current
  common model must not claim one. `+0000` keeps its known-zero interpretation.
  2.8.2 treats both spellings as zero-offset times. No data-model field is added.
- **control**: 2.9.1 requires paired MSH-15/16 requests; this profile refuses
  explicit nulls rather than treating them as an empty pair. Control identifiers
  retain the existing delimiter-free contract, extended to the declared truncation
  marker. Generic data text supports literal `\P\`; a sender truncation marker
  still parks the receipt. Outgoing values are never shortened.
- **source**: 2.8.2 §§2.A.13/.22/.88 pp.24–32, 38–39, 89–94;
  §§2.5.5.3–4 pp.13–14; §§2.14.8–9 pp.57–62; §7.4.1 pp.36–47.
  2.9.1 §§2A.2.13/.22/.88 pp.25–33, 39–40, 92–96;
  §§2.4.5.3–4 pp.15–16; §§2.13.8–9 pp.67–72; §7.4.1 pp.49–57.
- **tests**: The source-named rows in versions.md link protocol `modern.rs`,
  mapping `mapping/modern.rs`, config edition tests and the dialog custody test.
  Evidence remains documented. Order sending is qualified for 2.6/2.8.2 below;
  2.9.1 remains blocked at its source gate.

## Receive results without reviving withdrawn original queries

- **what**: Both modern profiles receive results and refuse legacy queries. Order
  sending is qualified separately below for 2.8.2; 2.9.1 stays blocked.
  A legacy-query configuration fails before activation with `WithdrawnHl7Query`;
  the diagnostic names the edition and its withdrawing section. This rule does
  not depend on whether the result profile has finished qualification.
  In 2.6, keep the bounded original-query exchange: chapter 5 retains it for
  backward compatibility and does not withdraw it.
- **why**: The newer editions withdraw the original query messages from 2.7.
  No replacement contract is selected or needed. LAW's QBP contract remains
  a separate modern-query path and does not pass through this legacy gate.
- **source**: 2.6 §5.10.2 pp.95–96; 2.8.2 §5.10.2 p.104;
  2.9.1 §5.9.2 p.80.
  Result-only scope and no replacement query design were selected by the owner.
- **tests**: config `hl7_2_8_2_s5_10_2_withdrawn_original_query_fails_config_validation`,
  `hl7_2_9_1_s5_9_2_withdrawn_original_query_fails_config_validation`, and
  `hl7_2_6_s5_10_2_original_queries_retained_for_backward_compatibility`.

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
- **remaining**: 2.8.2/2.9.1 now have bounded result profiles. Their original queries are withdrawn
  (2.8.2 §5.10.2; 2.9.1 §5.9.2); a modern query contract is not implicitly
  selected. The 2.6 and 2.8.2 bounded order writers are qualified below;
  2.9.1 stays blocked. Evidence: documented.

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
  `adr0012_s3_typed_supported_config_editions_preserve_exact_wire_identity`
  now checks all eight qualified receiving editions. The earlier unsupported-profile
  regression was replaced as the final two received their typed paths; modern
  legacy-query and unqualified-writer refusal tests remain active.

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

## Report order-writer qualification separately from receiving

Config obtains the edition-specific sending blocker from the protocol registry.
Receiving support and receiving compatibility do not imply writing support
(ADR 0012 section 3; HL7 2.5.1 section 2.8). Unavailable historical definitions
and unfinished modern implementation are different blockers. The modern source
material remains available for follow-up; no source absence is claimed there.
The version audit lists the remaining work without enabling any edition.
Evidence: `adr0012_s3_config_reports_the_registry_writer_blocker`.

## Qualify 2.6 laboratory sending with explicit coding revisions

The selected transaction is OML/O21 with ORL/O22, not legacy ORM. Preserve the
existing bounded order model and conservative identity/code caps, but use CWE
for OBR-4 and scalar DTM for timing. A test revision is carried in component 7;
priority table 0485 and specimen coding revisions are supplied separately.
Requiring revisions for all named systems avoids guessing table ownership from
an HL7 prefix. Sources and the field matrix are in versions.md; protocol
`edition_orders.rs`, config edition tests and program `hl7_231_251_26_282_s4_4_7_send_ahead_accepted_rejected_and_mixed_items` establish
the bounded behavior. Evidence remains documented; no retransmission is added.

## Keep the 2.8.2 writer on the patient-required O22 response

Select section 4.4.7.1 ORL_O22, not section 4.4.7.2 ORL_O41. Order decisions
without that response group are refused. ORC-4 uses EIP with the placer authority
as EI subcomponents (4.5.1.4). Caller-supplied patient content needs an explicit
CX.5 identifier type (2.A.14.5); no type is inferred from a namespace. The
contract-v1 mapper still omits the optional request PID when no name is supplied.
The shared writer keeps conservative caps and explicit coding revisions.
Evidence: edition order and host send/recovery tests listed in versions.md.

## Stop 2.9.1 sending at the ORC-4 source conflict

Keep 2.9.1 blocked rather than choosing separators for its group authority.
Chapter 4 (September 2024) 4.5.1 and 4.5.1.4 declare EI while the field's
component description uses subcomponent separators. The exact conflict and
required clarification are in versions.md. This is a source gate, not a claim
that 2.9.1 lacks laboratory ordering. The existing O22/O53 distinction remains
separate. Evidence: the source-named config refusal test and registry blocker.

## Supported timezone offsets

HL7 2.5.1 section 2.A.22 (inherited by TS section 2.A.77.1) restricts offsets to legally defined timezones. Our explicit support policy accepts -1200 through +1400, with minutes 00, 30 or 45 only. LAW MSH-7 (TF-2b C.4-6) uses the same check. Exact spelling, including signed zero, remains unchanged; no zone is inferred and no conversion is performed. Unsupported offsets are located failures and the raw receipt remains available.

LAW universal IDs currently support ISO OIDs only (section 2.A.33.2); other schemes are refused until their lexical rules are implemented. Local-only identities remain supported.

## Park reports until required context is established

- **what**: Park results with missing OBR-4/25 or OBX-3/11, an unsupported
  report or observation status, or a populated value without its value type.
  Require OBR-22 report time in the selected 2.8.2/2.9.1 report profile.
  Keep original fields and any mapped evidence; do not supply a final status.
  A real machine that omits OBR-25 or OBX-11 will have its results parked until
  a named machine departure allows that omission.
- **why**: Successful value decoding or identity selection does not establish
  report completion. The generic profile must not infer omitted context or
  silently apply a machine-specific default.
- **source**: HL7 2.3.1 section 7.3.2; 2.5.1/2.6/2.8.2/2.9.1 sections
  7.4.1 and 7.4.2, especially 7.4.1.25 and 7.4.2.11; modern 7.4.1.22;
  ADR 0003, retaining and parking unqualified input.
- **tests**: `hl7_s7_4_2_11_s7_4_1_25_missing_or_unknown_report_context_parks`,
  `hl7_251_s2_10_2_2_s7_4_2_11_s7_4_1_25_unsafe_reports_are_parked`.

## Refuse continuation and sequence protocols that are not negotiated

- **what**: Refuse populated MSH-14 and DSC fragments independently, including
  a final fragment received alone after reconnect. Refuse populated MSH-13;
  the generic receiver does not negotiate sequence-number processing. Retain
  exact receipts with specific reasons instead of delivering partial reports.
- **why**: Fragment-local message identities do not establish a complete logical
  report. A header option cannot activate assembly, sequencing or correlation
  state that Analyte has not implemented and selected.
- **source**: HL7 2.5.1 sections 2.10.1 and 2.10.2.2; 2.6/2.8.2 section
  2.10.2; 2.9.1 section 2.9.2; ADR 0003, original evidence and result identity.
- **tests**: `hl7_251_s2_10_2_2_final_fragment_with_msh14_is_never_a_complete_result`,
  `hl7_251_s2_10_1_unnegotiated_sequence_number_is_refused`,
  `hl7_251_s2_10_2_2_s7_4_2_11_s7_4_1_25_unsafe_reports_are_parked`.

## Refuse enhanced 2.9.1 application exchanges without a separate channel

- **what**: Refuse configuration that enables enhanced 2.9.1 application
  acknowledgments. Commit and park incoming requests for that exchange without
  interpreting them or writing an application response on the receiving channel.
  Keep original-mode and enhanced accept-only exchanges in the selected scope.
- **why**: The required separate application-response channel has no owner in
  the generic receiver. Sending the response on the initiating channel would
  claim support for a different exchange.
- **source**: HL7 2.9.1 section 2.8.3.2 and its channel note; ADR 0007,
  explicit supported scope.
- **tests**: `hl7_291_s2_8_3_2_separate_application_channel_cannot_be_enabled`,
  `hl7_291_s2_8_3_2_enhanced_application_ack_requires_separate_channel`.

## Refuse enhanced observation grouping until its identity contract is selected

- **what**: In 2.8.2/2.9.1, accept legacy OG.1 but refuse populated enhanced
  OBX-4 grouping components before selector-based identity mapping.
- **why**: Selecting only a group, or no sub-ID, could collapse distinct
  group/sequence observations into one replacement identity. The bounded
  profile does not select the full enhanced grouping contract.
- **source**: HL7 2.8.2 sections 2.A.50 and 7.4.2.4; 2.9.1 sections
  2A.2.50 and 7.4.2.4; ADR 0003, result identity.
- **test**: `hl7_282_s2_a_50_291_s7_4_2_4_enhanced_og_requires_full_grouping_contract`.

## Keep native 2.3.1 interpretation within its own field inventory

- **what**: Refuse native 2.3.1 specimen/container extensions and leave OBX-19
  raw rather than assigning later analysis-time meaning. LAW retains its
  separately selected compatible-VID analysis-time contract.
- **why**: An unknown tail field or familiar segment cannot acquire clinical
  meaning from a later edition without an evidenced, named machine departure.
- **source**: HL7 2.3.1 sections 7.2.1 and 7.3.2; compare 2.5.1 section
  7.4.2.19; ADR 0007, standards before machine departures.
- **tests**: `hl7_231_s7_2_1_s7_3_2_later_fields_do_not_gain_meaning`,
  `law_tf2b_c4_and_3_29_4_compatible_vid_keeps_law_analysis_time`.

## Refuse new patient-related ORM when mandatory PID cannot be supplied

- **what**: For native 2.3.1 new patient work, require PID regardless of the
  optional config flag. Contract-v1 lacks the mandatory name, so reject before
  reservation or Sending. The pure writer also refuses identifier-only new
  patient work. Keep nonpatient work and cancellations separate.
- **why**: A known patient cannot become unspecified merely because required
  patient content is unavailable. Inventing a name or omitting PID would hide
  the missing contract information.
- **source**: HL7 2.3.1 sections 4.2.1.1(d) and 3.3.2; ADR 0003,
  order lifecycle and no invented data.
- **tests**: `hl7_231_3_3_2_required_pid_rejects_before_reservation_without_inventing_name`,
  `hl7_231_s4_2_1_1_d_patient_requiredness_distinguishes_new_and_cancel`,
  `hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context`.

## Validate patient content whenever an order reply includes PID

- **what**: Require a present reply PID to contain a non-null identifier and
  surname independently of modern identifier-type requiredness. Modern CX.5 is
  an additional check, not the switch for all patient validation. A malformed
  reply does not establish machine acceptance.
- **why**: An optional group does not make its required fields optional when
  present. Order correlation cannot substitute for patient content, and the
  receiver must not infer demographics to complete a reply.
- **source**: HL7 2.5.1/2.6 sections 3.4.2 and 4.4.7; 2.8.2 sections
  3.3.2 and 4.4.7.1; ADR 0003, uncertain order outcomes.
- **tests**: `hl7_251_26_s4_4_7_required_pid_content_in_order_reply`,
  `hl7_251_s3_4_2_present_reply_pid_requires_identifier_and_name`.

## Refuse collection intervals the selected time mapping cannot represent

- **what**: Preserve point-collection handling, but specifically refuse SPM-17
  ranges or nested endpoint precision before scalar time interpretation.
- **why**: A DR endpoint is not a scalar TS precision indicator. The current
  mapping cannot faithfully represent those interval forms; keeping and parking
  the receipt avoids assigning the end time as the start time's precision.
- **source**: HL7 2.5.1 sections 7.4.3.17-19; ADR 0003, exact source values.
- **tests**: `hl7_251_s7_4_3_17_collection_range_is_not_ts_precision`,
  `rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`.

## Validate the selected bounded result grammar before assigning ownership

- **what**: Check group order, selected cardinality and unfinished groups before
  constructing ORU/OUL ownership. Refuse unselected or misplaced segments,
  including an ORU header NTE; preserve OUL's separately defined header note.
- **why**: A whitelist and most-recent-parent pointers can assign valid-looking
  observations to malformed groups. A bounded grammar is sufficient for the
  selected profile and does not imply a general HL7 schema engine.
- **source**: HL7 2.3.1 section 7.2.1; 2.5.1 sections 7.3.1 and 7.3.7;
  later ORU section 7.3.1; ADR 0007, explicit supported scope.
- **tests**: `hl7_231_s7_2_1_251_s7_3_1_s7_3_7_invalid_known_groups_are_refused`,
  `plan6_notes_keep_parent_paths_even_without_a_result`.

## Control boundary regressions (standards pass 2)

Response timestamps use the calendar and precision parser in every edition,
including TS editions (2.5.1 §2.15.9.7 and §2.A.77). The selected ACK output
profile writes the time component only; it does not emit a TS precision component.

ERR-3 uses table 0357; zero requires informational severity (2.5.1 §2.15.5.3–4).
The error-only ACK renderer refuses zero. Fatal severity starts with the selected
2.6 profile; code 104 starts with the selected 2.8.2 profile. Unknown codes are
refused, with local application codes carried separately. Tests are in
[standards_pass2.rs](../../../crates/protocols/hl7/tests/standards_pass2.rs).

The strict modern withdrawal profile refuses explicit null as well as other
nonempty encodings in withdrawn MSA/ERR fields (2.8.2 §2.14.8 and 2.9.1 §2.13.8).
This is the declared profile's refusal policy, not a claim that explicit null
carries clinical content. View regressions and the O22 response regressions in
[edition_orders.rs](../../../crates/protocols/hl7/tests/edition_orders.rs) cover it.
