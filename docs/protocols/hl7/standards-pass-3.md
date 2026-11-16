# HL7 standards pass 3 implementation evidence

This change keeps the documented evidence level. No analyzer or simulator
qualification is claimed. Source documents remain outside the repository.

## Findings and regressions

| Finding | Source checked | Implemented behavior | Regression (test-name suffix) |
| --- | --- | --- | --- |
| H7 | 2.9.1 2.8.3.2 | Enhanced application ACK uses NE/NE; immediate accept ACK and original mode retain their own rules. | `application_ack_declares_ne_without_recursion` |
| H8 | 2.9.1 2.8.3.2 and its channel note | Config refuses enabling enhanced application responses; receiver commits and parks requests without interpreting or writing on the initiating channel. | `enhanced_application_ack_requires_separate_channel`, `separate_application_channel_cannot_be_enabled` |
| H9 | 2.5.1 7.4.3.17–19 | Point collection remains supported. Ranges or nested endpoint precision are refused specifically before scalar interpretation. | `collection_range_is_not_ts_precision`; existing point-collection source-time regression |
| H10 | 2.3.1 7.3.2; 2.5.1/2.6/2.8.2/2.9.1 7.4.1 and 7.4.2 | Missing OBR-4/25 or OBX-3/11, unsupported statuses, missing value type for populated values, and missing modern report time produce findings and prevent delivery. | `missing_or_unknown_report_context_parks`, host `unsafe_reports_are_parked` |
| H11 | 2.3.1 7.2.1; 2.5.1 7.3.1/7.3.7 and later ORU structures | Bounded grammar checks group transitions before hierarchy construction, including unfinished groups at EOF. | `invalid_known_groups_are_refused` |
| H12 | 2.8.2 2.A.50/7.4.2.4; 2.9.1 2A.2.50/7.4.2.4 | Refuses populated enhanced OG components before selector-based identity mapping; legacy OG.1 remains supported. | `enhanced_og_requires_full_grouping_contract` |
| H13 | 2.3.1 7.2.1/7.3.2; compare 2.5.1 7.4.2.19 | Native 2.3.1 refuses specimen/container extensions and does not interpret OBX-19. LAW retains its separately selected analysis-time contract. | `later_fields_do_not_gain_meaning`, existing LAW compatible-VID regression |
| H14 | 2.3.1 4.2.1.1(d), 3.3.2 | Known-patient new ORM without mandatory name is refused before reservation regardless of optional PID flag; pure writer also refuses identifier-only new patient work. | `required_pid_rejects_before_reservation_without_inventing_name`, `explicit_group_position_and_identifier_do_not_invent_context` |
| H15 | 2.5.1 2.10.1/2.10.2.2; later continuation equivalents | Refuses MSH-14 and DSC fragments independently, including standalone final fragments. Sequence-number negotiation is also explicitly unsupported. | `final_fragment_with_msh14_is_never_a_complete_result`, `unnegotiated_sequence_number_is_refused`, host `unsafe_reports_are_parked` |
| H16 | 2.5.1/2.6 3.4.2 and order response structures; 2.8.2 3.3.2 | Present PID requires identifier and surname independently of modern identifier-type requiredness. Malformed reply remains unsuccessful. | `required_pid_content_in_order_reply`, `present_reply_pid_requires_identifier_and_name` |

These are bounded profile checks, not a general HL7 schema engine. Unsupported
states remain evidence; they are never converted to final results.

## Changed expectations and fixtures

Six existing mapping expectations now require parking because their inputs omit
required report context. Their exact value, decoded value, timestamp, ownership
and replacement-unit assertions remain:

- Modern exact-value cases: renamed from delivery to retained, parked evidence;
  missing OBR-4/25 and modern OBR-22 prevent qualification.
- Modern UTC-offset cases: preserved time interpretation does not establish
  missing report context.
- 2.6 exact value/time/identity cases: missing OBR-4/25 prevent qualification.
- Selected identifier/escape cases: correct decoding does not establish OBR context.
- OUL ownership case: correct ancestry does not establish required report fields.
- Same-key OBX replacement case: correct grouping does not establish report context.

Value-kind and QC conflict tests keep their original pass/fail expectations. An
explicit synthetic report-context helper supplies OBR-4/25 so those tests still
isolate their intended value/QC rule rather than passing because of unrelated
missing report fields.

The native ORM context test now explicitly refuses known-patient identifier-only
work, and uses NonPatient for its successful group/position assertions. Omitting
PID for known-patient new work was the defect, not a valid positive expectation.

The 2.6 response fixture and host response helper now supply PID-3/5; bare PID
cannot be a valid accepted reply. Their original order outcome expectations stay.

Four host synthetic report fixtures (shared support, receive, interpretation,
admin action) and the synthetic HL7 recording now supply OBR-4/25. Their existing
AA, delivery, replay, audit and reprocessing expectations remain. The recording
is a hand-authored synthetic JSONL fixture: only its incoming OBR bytes changed;
its expected response and response masks remain unchanged. No malformed-input
fixture was repaired to conceal its intended failure.

The host safety regression verifies exact retained MLLP bytes, parked evidence,
AE instead of AA and an empty delivery queue for unsafe reports/fragments.
