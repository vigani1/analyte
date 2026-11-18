# Mapping mutation triage

All 185 baseline misses are listed below. 178 real gaps became caught; 7 survivors are equivalent under validated inputs. Production code is unchanged.

The main [quality report](quality.md) records commands, run counts, source reads and limits. A comment above each test names the governing protocol section or clinical rule. Each caught row was checked against a recorded failing-test log; it is not inferred from coverage. The latest focused replay supersedes the full rerun only for the same mutation name.

| Baseline missed mutant (location after main merge) | Decision and evidence |
| --- | --- |
| `lib.rs:54:5: replace is_law -> bool with false` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `lib.rs:299:13: replace &#124;&#124; with && in Output::requires_parking` | Real gap, now caught by `mutation::unmapped_evidence_alone_requires_parking`. |
| `lib.rs:313:32: replace > with == in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:313:32: replace > with >= in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:334:35: replace &#124;&#124; with && in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:334:18: replace > with == in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:334:18: replace > with >= in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:334:57: replace > with == in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:334:57: replace > with >= in map_astm` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `lib.rs:378:5: replace map_law -> Result<Output, Error> with Ok(Default::default())` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `lib.rs:378:8: delete ! in map_law` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `lib.rs:388:5: replace check_hl7 -> Result<(), Error> with Ok(())` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:396:45: replace &#124;&#124; with && in check_hl7` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:396:28: replace > with == in check_hl7` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:396:28: replace > with >= in check_hl7` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:396:75: replace > with == in check_hl7` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:396:75: replace > with >= in check_hl7` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `lib.rs:522:31: replace > with == in identity_run` | Real gap, now caught by `mutation::identity_run_ceiling_is_inclusive`. |
| `lib.rs:522:31: replace > with >= in identity_run` | Real gap, now caught by `mutation::identity_run_ceiling_is_inclusive`. |
| `identity.rs:118:5: replace law_observation -> Result<Option<ObservationIdentity>, Error> with Ok(None)` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `identity.rs:122:69: replace == with != in law_observation` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `identity.rs:148:22: replace == with != in law_observation` | Real gap, now caught by `mutation::law_result_keeps_awos_and_observation_group`. |
| `map.rs:18:22: replace match guard source.syntax == FieldSyntax::AstmLis02 with true in processing` | Equivalent. HL7 Msh::new rejects processing Q before source mapping (hl7/src/views.rs); ASTM Q still enters the same arm. No reachable behavior changes. |
| `map.rs:18:22: replace match guard source.syntax == FieldSyntax::AstmLis02 with false in processing` | Real gap, now caught by `mutation::qc_mode_is_kept_and_never_production`. |
| `map.rs:18:36: replace == with != in processing` | Real gap, now caught by `mutation::qc_mode_is_kept_and_never_production`. |
| `map.rs:41:27: replace > with == in message` | Real gap, now caught by `mutation::astm_mapping_rechecks_limits_inclusively`. |
| `map.rs:41:27: replace > with >= in message` | Real gap, now caught by `mutation::hl7_mapping_rechecks_limits_inclusively`. |
| `map.rs:61:64: replace == with != in message` | Real gap, now caught by `mutation::hl7_note_text_and_position_are_kept`. |
| `map.rs:251:38: replace match guard translation.is_some() with true in message` | Real gap, now caught by `mutation::unknown_test_never_gets_resolved_identity`. |
| `orders.rs:154:31: replace &#124;&#124; with && in astm` | Real gap, now caught by `test_count_is_nonempty_and_inclusive`. |
| `orders.rs:154:52: replace > with == in astm` | Real gap, now caught by `test_count_is_nonempty_and_inclusive`. |
| `orders.rs:154:52: replace > with >= in astm` | Real gap, now caught by `test_count_is_nonempty_and_inclusive`. |
| `orders.rs:207:31: replace > with == in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:207:31: replace > with >= in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:235:9: replace &#124;&#124; with && in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:234:21: replace > with == in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:234:21: replace > with >= in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:235:57: replace > with == in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:235:57: replace > with >= in astm` | Real gap, now caught by `record_and_message_limits_are_inclusive`. |
| `orders.rs:251:23: replace &#124;&#124; with && in set` | Equivalent. Validated config guarantees 1 <= selected.component <= syntax.components (config/src/orders.rs order_field). Both original disjuncts are always false. |
| `orders.rs:251:36: replace > with == in set` | Real gap, now caught by `component_at_configured_ceiling_is_kept`. |
| `orders.rs:251:36: replace > with >= in set` | Real gap, now caught by `component_at_configured_ceiling_is_kept`. |
| `orders.rs:264:5: replace wire_text -> Result<(), OrderMappingError> with Ok(())` | Real gap, now caught by `reserved_sample_characters_are_refused_unchanged`. |
| `orders.rs:266:9: replace &#124;&#124; with && in wire_text` | Real gap, now caught by `reserved_sample_characters_are_refused_unchanged`. |
| `orders.rs:265:9: replace &#124;&#124; with && in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes. The remaining byte-repertoire check already rejects every non-ASCII byte, so dropping the redundant ASCII predicate changes nothing. |
| `orders.rs:264:20: replace > with == in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes (model/src/text.rs and config/src/orders.rs). Neither 4096 equality nor overflow is reachable. |
| `orders.rs:264:20: replace > with >= in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes (model/src/text.rs and config/src/orders.rs). Neither 4096 equality nor overflow is reachable. |
| `orders.rs:268:46: replace &#124;&#124; with && in wire_text` | Real gap, now caught by `reserved_sample_characters_are_refused_unchanged`. |
| `orders.rs:280:5: replace patient -> Result<&'a str, OrderMappingError> with Ok("")` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:280:5: replace patient -> Result<&'a str, OrderMappingError> with Ok("xyzzy")` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:284:18: replace != with == in patient` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:296:8: delete ! in vidas` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:300:22: replace > with == in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:300:22: replace > with < in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:300:22: replace > with >= in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:303:36: replace > with == in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:303:36: replace > with < in vidas` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:303:36: replace > with >= in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:313:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:312:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:311:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:311:30: replace > with == in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:311:30: replace > with < in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:311:30: replace > with >= in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:312:30: replace > with == in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:312:30: replace > with < in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:312:30: replace > with >= in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:313:30: replace > with == in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:313:30: replace > with < in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:313:30: replace > with >= in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:317:8: delete ! in vidas` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:320:33: replace == with != in vidas` | Real gap, now caught by `vidas_download_keeps_exact_patient_and_sample`. |
| `orders.rs:336:29: replace == with != in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:351:33: replace > with == in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:351:33: replace > with < in vidas` | Real gap, now caught by `vidas_download_reports_exact_identity_bounds`. |
| `orders.rs:351:33: replace > with >= in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:357:24: replace > with == in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:357:24: replace > with < in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:357:24: replace > with >= in vidas` | Real gap, now caught by `vidas_download_count_and_size_bounds_are_independent`. |
| `orders.rs:371:8: delete ! in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:377:22: replace > with == in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:377:22: replace > with < in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:377:22: replace > with >= in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:387:9: delete match arm "S" in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:388:9: delete match arm "U" in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:394:34: replace == with != in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:415:42: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:415:24: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:415:38: replace > with == in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:415:38: replace > with < in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:415:38: replace > with >= in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:415:45: delete ! in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:421:8: delete ! in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:426:9: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:425:9: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:425:30: replace > with == in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:425:30: replace > with < in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:425:30: replace > with >= in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:426:30: replace > with == in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:426:30: replace > with < in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:426:30: replace > with >= in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:435:29: replace == with != in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:443:32: replace > with == in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `orders.rs:443:32: replace > with < in autolyser` | Real gap, now caught by `autolyser_download_keeps_patient_and_context`. |
| `orders.rs:443:32: replace > with >= in autolyser` | Real gap, now caught by `autolyser_download_refuses_missing_context_or_overflow`. |
| `result.rs:54:39: replace != with == in part` | Real gap, now caught by `mutation::conflicting_qc_classification_is_parked`. |
| `result.rs:71:13: delete match arm "H" in part` | Real gap, now caught by `mutation::astm_timestamps_keep_role_and_original`. |
| `result.rs:72:13: delete match arm "O" in part` | Real gap, now caught by `mutation::astm_timestamps_keep_role_and_original`. |
| `result.rs:73:13: delete match arm "R" in part` | Real gap, now caught by `mutation::astm_timestamps_keep_role_and_original`. |
| `result.rs:74:13: delete match arm "MSH" in part` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:75:13: delete match arm "OBR" in part` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:77:13: delete match arm "SPM" in part` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:88:25: delete match arm "Y" in part` | Real gap, now caught by `mutation::declared_precision_never_invents_digits`. |
| `result.rs:89:25: delete match arm "L" in part` | Real gap, now caught by `mutation::declared_precision_never_invents_digits`. |
| `result.rs:91:25: delete match arm "H" in part` | Real gap, now caught by `mutation::declared_precision_never_invents_digits`. |
| `result.rs:92:25: delete match arm "M" in part` | Real gap, now caught by `mutation::declared_precision_never_invents_digits`. |
| `result.rs:93:25: delete match arm "S" in part` | Real gap, now caught by `mutation::declared_precision_never_invents_digits`. |
| `result.rs:182:30: replace == with != in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:190:13: delete match arm "H" in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:191:13: delete match arm "O" in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:192:13: delete match arm "R" in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:193:13: delete match arm "MSH" &#124; "OBX" in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:194:13: delete match arm "OBR" in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:195:13: delete match arm "ORC" in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:201:65: replace &#124;&#124; with && in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:201:43: replace &#124;&#124; with && in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:201:36: replace == with != in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:201:56: replace == with != in source_path` | Real gap, now caught by `mutation::hl7_source_path_keeps_statuses_and_times`. |
| `result.rs:201:78: replace == with != in source_path` | Real gap, now caught by `mutation::astm_source_path_keeps_statuses_and_sequences`. |
| `result.rs:229:16: replace match guard !astm with true in status` | Real gap, now caught by `mutation::astm_statuses_never_take_hl7_meanings`. |
| `result.rs:230:22: replace match guard !astm with true in status` | Real gap, now caught by `mutation::astm_statuses_never_take_hl7_meanings`. |
| `result.rs:231:16: replace match guard !astm with true in status` | Real gap, now caught by `mutation::astm_statuses_never_take_hl7_meanings`. |
| `source.rs:127:22: replace > with == in Source<'_>::original_text` | Real gap, now caught by `mutation::value_text_bound_is_inclusive`. |
| `source.rs:127:22: replace > with >= in Source<'_>::original_text` | Real gap, now caught by `mutation::value_text_bound_is_inclusive`. |
| `source.rs:212:61: replace > with == in Source<'_>::repetitions` | Real gap, now caught by `mutation::value_text_bound_is_inclusive`. |
| `source.rs:212:61: replace > with >= in Source<'_>::repetitions` | Real gap, now caught by `mutation::value_text_bound_is_inclusive`. |
| `source.rs:273:47: replace && with &#124;&#124; in Source<'_>::selection_position` | Real gap, now caught by `mutation::missing_sample_reports_owner_position`. |
| `source.rs:273:28: replace == with != in Source<'_>::selection_position` | Real gap, now caught by `mutation::missing_sample_reports_owner_position`. |
| `source.rs:273:60: replace == with != in Source<'_>::selection_position` | Real gap, now caught by `mutation::missing_sample_reports_owner_position`. |
| `source.rs:360:16: replace match guard msh.version() == analyte_hl7::Version::V251 with true in hl7` | Real gap, now caught by `mutation::unqualified_hl7_structure_is_refused_intact`. |
| `source.rs:519:13: delete match arm "PV1" &#124; "PV2" in hl7_segments` | Real gap, now caught by `mutation::visit_context_keeps_the_result`. |
| `source.rs:420:22: replace match guard index == 0 with true in hl7_segments` | Equivalent. Message::parse rejects any MSH after segment zero (hl7/src/syntax.rs); the mapper receives an immutable parsed Message. |
| `source.rs:482:22: replace match guard specimen.is_some() with true in hl7_segments` | Real gap, now caught by `mutation::unqualified_hl7_structure_is_refused_intact`. |
| `source.rs:491:47: replace match guard s > o with true in hl7_segments` | Real gap, now caught by `mutation::oru_specimen_observations_keep_their_owner`. |
| `source.rs:491:47: replace match guard s > o with false in hl7_segments` | Real gap, now caught by `mutation::oru_specimen_observations_keep_their_owner`. |
| `source.rs:491:49: replace > with == in hl7_segments` | Real gap, now caught by `mutation::oru_specimen_observations_keep_their_owner`. |
| `source.rs:491:49: replace > with < in hl7_segments` | Real gap, now caught by `mutation::oru_specimen_observations_keep_their_owner`. |
| `source.rs:491:49: replace > with >= in hl7_segments` | Equivalent. Only the >= replacement is equivalent: specimen and order are different source nodes (SPM versus OBR), so their indices can never be equal. |
| `source.rs:508:25: replace &#124;&#124; with && in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `source.rs:507:25: delete ! in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `source.rs:508:28: delete ! in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `source.rs:510:53: replace == with != in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `source.rs:534:43: replace && with &#124;&#124; in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `source.rs:534:26: replace && with &#124;&#124; in hl7_segments` | Real gap, now caught by `mutation::oul_details_and_notes_stay_in_observation`. |
| `time.rs:27:42: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:27:28: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:27:37: replace > with == in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:27:37: replace > with >= in parse` | Real gap, now caught by `mutation::offsets_are_kept_without_inferring_zone`. |
| `time.rs:27:53: replace > with == in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:27:53: replace > with >= in parse` | Real gap, now caught by `mutation::offsets_are_kept_without_inferring_zone`. |
| `time.rs:33:68: delete - in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:46:9: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:47:58: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:47:43: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:47:54: replace > with < in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:64:9: delete match arm (4, None) in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:65:9: delete match arm (6, None) in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:66:9: delete match arm (8, None) in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `time.rs:67:9: delete match arm (10, None) in parse` | Real gap, now caught by `mutation::partial_and_invalid_times_keep_evidence`. |
| `value.rs:29:38: replace == with != in values` | Real gap, now caught by `mutation::reason_tells_formatting_from_escape`. |
| `value.rs:52:17: delete match arm "" in values` | Real gap, now caught by `mutation::status_only_and_missing_value_type_differ`. |
| `value.rs:51:23: replace match guard original.is_empty() with true in values` | Real gap, now caught by `mutation::status_only_and_missing_value_type_differ`. |
| `value.rs:51:23: replace match guard original.is_empty() with false in values` | Real gap, now caught by `mutation::status_only_and_missing_value_type_differ`. |
| `value.rs:98:9: delete match arm "<=" in comparator` | Real gap, now caught by `mutation::sn_comparators_and_ranges_keep_components`. |
| `value.rs:99:9: delete match arm ">" in comparator` | Real gap, now caught by `mutation::sn_comparators_and_ranges_keep_components`. |
| `value.rs:100:9: delete match arm ">=" in comparator` | Real gap, now caught by `mutation::sn_comparators_and_ranges_keep_components`. |
| `value.rs:138:63: replace &#124;&#124; with && in coded` | Real gap, now caught by `mutation::ambiguous_coded_values_are_parked`. |
| `value.rs:175:9: replace &#124;&#124; with && in structured` | Real gap, now caught by `mutation::unknown_comparator_never_becomes_equality`. |
| `value.rs:174:9: delete ! in structured` | Real gap, now caught by `mutation::sn_comparators_and_ranges_keep_components`. |
| `value.rs:174:36: replace != with == in structured` | Real gap, now caught by `mutation::unknown_comparator_never_becomes_equality`. |
| `value.rs:219:25: replace && with &#124;&#124; in check_kind` | Real gap, now caught by `mutation::coded_kind_parks_empty_or_numeric_text`. |
| `value.rs:218:25: replace && with &#124;&#124; in check_kind` | Real gap, now caught by `mutation::coded_kind_parks_empty_or_numeric_text`. |
| `orders/display.rs:76:17: replace &#124;&#124; with && in map_display` | Real gap, now caught by `display_bound_is_inclusive_and_refuses_delimiters`. |
| `orders/display.rs:75:28: replace > with == in map_display` | Real gap, now caught by `display_bound_is_inclusive_and_refuses_delimiters`. |
| `orders/display.rs:75:28: replace > with >= in map_display` | Real gap, now caught by `display_bound_is_inclusive_and_refuses_delimiters`. |
