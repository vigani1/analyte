# Mapping mutation triage

All 185 baseline misses are listed below. 178 real gaps became caught; 7 survivors are equivalent under validated inputs. Production code is unchanged.

The main [quality report](quality.md) records commands, run counts, source reads and limits. Test names identify the governing protocol section or clinical rule. Each caught row was checked against a recorded failing-test log; it is not inferred from coverage. The latest focused replay supersedes the full rerun only for the same mutation name.

| Baseline missed mutant (location after main merge) | Decision and evidence |
| --- | --- |
| `lib.rs:54:5: replace is_law -> bool with false` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `lib.rs:299:13: replace &#124;&#124; with && in Output::requires_parking` | Real gap, now caught by `mutation::adr0003_unmapped_evidence_alone_requires_parking`. |
| `lib.rs:313:32: replace > with == in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:313:32: replace > with >= in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:334:35: replace &#124;&#124; with && in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:334:18: replace > with == in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:334:18: replace > with >= in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:334:57: replace > with == in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:334:57: replace > with >= in map_astm` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:378:5: replace map_law -> Result<Output, Error> with Ok(Default::default())` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `lib.rs:378:8: delete ! in map_law` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `lib.rs:388:5: replace check_hl7 -> Result<(), Error> with Ok(())` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:396:45: replace &#124;&#124; with && in check_hl7` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:396:28: replace > with == in check_hl7` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:396:28: replace > with >= in check_hl7` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:396:75: replace > with == in check_hl7` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:396:75: replace > with >= in check_hl7` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `lib.rs:522:31: replace > with == in identity_run` | Real gap, now caught by `mutation::rule7_identity_run_accepts_exact_source_text_ceiling_and_rejects_larger`. |
| `lib.rs:522:31: replace > with >= in identity_run` | Real gap, now caught by `mutation::rule7_identity_run_accepts_exact_source_text_ceiling_and_rejects_larger`. |
| `identity.rs:118:5: replace law_observation -> Result<Option<ObservationIdentity>, Error> with Ok(None)` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `identity.rs:122:69: replace == with != in law_observation` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `identity.rs:148:22: replace == with != in law_observation` | Real gap, now caught by `mutation::law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry`. |
| `map.rs:18:22: replace match guard source.syntax == FieldSyntax::AstmLis02 with true in processing` | Equivalent. HL7 Msh::new rejects processing Q before source mapping (hl7/src/views.rs); ASTM Q still enters the same arm. No reachable behavior changes. |
| `map.rs:18:22: replace match guard source.syntax == FieldSyntax::AstmLis02 with false in processing` | Real gap, now caught by `mutation::lis02_6_12_quality_control_mode_is_retained_and_never_deliverable_as_production`. |
| `map.rs:18:36: replace == with != in processing` | Real gap, now caught by `mutation::lis02_6_12_quality_control_mode_is_retained_and_never_deliverable_as_production`. |
| `map.rs:41:27: replace > with == in message` | Real gap, now caught by `mutation::rule7_astm_mapping_rechecks_independent_limits_inclusively`. |
| `map.rs:41:27: replace > with >= in message` | Real gap, now caught by `mutation::rule7_hl7_mapping_rechecks_independent_limits_inclusively`. |
| `map.rs:61:64: replace == with != in message` | Real gap, now caught by `mutation::rule5_hl7_note_text_and_field_position_are_retained`. |
| `map.rs:251:38: replace match guard translation.is_some() with true in message` | Real gap, now caught by `mutation::adr0003_rule2_unknown_test_with_source_namespace_never_gets_resolved_identity`. |
| `orders.rs:154:31: replace &#124;&#124; with && in astm` | Real gap, now caught by `rule7_outbound_test_count_is_nonempty_and_inclusive`. |
| `orders.rs:154:52: replace > with == in astm` | Real gap, now caught by `rule7_outbound_test_count_is_nonempty_and_inclusive`. |
| `orders.rs:154:52: replace > with >= in astm` | Real gap, now caught by `rule7_outbound_test_count_is_nonempty_and_inclusive`. |
| `orders.rs:207:31: replace > with == in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:207:31: replace > with >= in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:235:9: replace &#124;&#124; with && in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:234:21: replace > with == in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:234:21: replace > with >= in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:235:57: replace > with == in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:235:57: replace > with >= in astm` | Real gap, now caught by `rule7_outbound_encoded_record_and_message_limits_are_inclusive`. |
| `orders.rs:251:23: replace &#124;&#124; with && in set` | Equivalent. Validated config guarantees 1 <= selected.component <= syntax.components (config/src/orders.rs order_field). Both original disjuncts are always false. |
| `orders.rs:251:36: replace > with == in set` | Real gap, now caught by `rule7_outbound_selected_component_at_configured_ceiling_is_preserved`. |
| `orders.rs:251:36: replace > with >= in set` | Real gap, now caught by `rule7_outbound_selected_component_at_configured_ceiling_is_preserved`. |
| `orders.rs:264:5: replace wire_text -> Result<(), OrderMappingError> with Ok(())` | Real gap, now caught by `rule5_outbound_sample_reserved_characters_are_refused_without_rewriting`. |
| `orders.rs:266:9: replace &#124;&#124; with && in wire_text` | Real gap, now caught by `rule5_outbound_sample_reserved_characters_are_refused_without_rewriting`. |
| `orders.rs:265:9: replace &#124;&#124; with && in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes. The remaining byte-repertoire check already rejects every non-ASCII byte, so dropping the redundant ASCII predicate changes nothing. |
| `orders.rs:264:20: replace > with == in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes (model/src/text.rs and config/src/orders.rs). Neither 4096 equality nor overflow is reachable. |
| `orders.rs:264:20: replace > with >= in wire_text` | Equivalent. All callers supply identifiers or configured components bounded to 256 bytes (model/src/text.rs and config/src/orders.rs). Neither 4096 equality nor overflow is reachable. |
| `orders.rs:268:46: replace &#124;&#124; with && in wire_text` | Real gap, now caught by `rule5_outbound_sample_reserved_characters_are_refused_without_rewriting`. |
| `orders.rs:280:5: replace patient -> Result<&'a str, OrderMappingError> with Ok("")` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:280:5: replace patient -> Result<&'a str, OrderMappingError> with Ok("xyzzy")` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:284:18: replace != with == in patient` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:296:8: delete ! in vidas` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:300:22: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:300:22: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:300:22: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:303:36: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:303:36: replace > with < in vidas` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:303:36: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:313:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:312:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:311:9: replace &#124;&#124; with && in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:311:30: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:311:30: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:311:30: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:312:30: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:312:30: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:312:30: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:313:30: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:313:30: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:313:30: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:317:8: delete ! in vidas` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:320:33: replace == with != in vidas` | Real gap, now caught by `adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample`. |
| `orders.rs:336:29: replace == with != in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:351:33: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:351:33: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_reports_exact_host_identity_bounds`. |
| `orders.rs:351:33: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:357:24: replace > with == in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:357:24: replace > with < in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:357:24: replace > with >= in vidas` | Real gap, now caught by `rule7_vendor_download_count_and_aggregate_bounds_are_independent`. |
| `orders.rs:371:8: delete ! in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:377:22: replace > with == in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:377:22: replace > with < in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:377:22: replace > with >= in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:387:9: delete match arm "S" in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:388:9: delete match arm "U" in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:394:34: replace == with != in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:415:42: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:415:24: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:415:38: replace > with == in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:415:38: replace > with < in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:415:38: replace > with >= in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:415:45: delete ! in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:421:8: delete ! in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:426:9: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:425:9: replace &#124;&#124; with && in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:425:30: replace > with == in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:425:30: replace > with < in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:425:30: replace > with >= in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:426:30: replace > with == in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:426:30: replace > with < in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:426:30: replace > with >= in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:435:29: replace == with != in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:443:32: replace > with == in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `orders.rs:443:32: replace > with < in autolyser` | Real gap, now caught by `autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved`. |
| `orders.rs:443:32: replace > with >= in autolyser` | Real gap, now caught by `rule7_autolyser_download_refuses_missing_context_and_bounds_before_output`. |
| `result.rs:54:39: replace != with == in part` | Real gap, now caught by `mutation::rule2_conflicting_explicit_qc_classification_is_unknown_and_parked`. |
| `result.rs:71:13: delete match arm "H" in part` | Real gap, now caught by `mutation::lis02_6_14_8_4_7_9_10_9_12_9_13_timestamps_keep_role_and_original`. |
| `result.rs:72:13: delete match arm "O" in part` | Real gap, now caught by `mutation::lis02_6_14_8_4_7_9_10_9_12_9_13_timestamps_keep_role_and_original`. |
| `result.rs:73:13: delete match arm "R" in part` | Real gap, now caught by `mutation::lis02_6_14_8_4_7_9_10_9_12_9_13_timestamps_keep_role_and_original`. |
| `result.rs:74:13: delete match arm "MSH" in part` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:75:13: delete match arm "OBR" in part` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:77:13: delete match arm "SPM" in part` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:88:25: delete match arm "Y" in part` | Real gap, now caught by `mutation::hl7_251_2a77_declared_precision_cannot_invent_missing_digits`. |
| `result.rs:89:25: delete match arm "L" in part` | Real gap, now caught by `mutation::hl7_251_2a77_declared_precision_cannot_invent_missing_digits`. |
| `result.rs:91:25: delete match arm "H" in part` | Real gap, now caught by `mutation::hl7_251_2a77_declared_precision_cannot_invent_missing_digits`. |
| `result.rs:92:25: delete match arm "M" in part` | Real gap, now caught by `mutation::hl7_251_2a77_declared_precision_cannot_invent_missing_digits`. |
| `result.rs:93:25: delete match arm "S" in part` | Real gap, now caught by `mutation::hl7_251_2a77_declared_precision_cannot_invent_missing_digits`. |
| `result.rs:182:30: replace == with != in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:190:13: delete match arm "H" in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:191:13: delete match arm "O" in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:192:13: delete match arm "R" in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:193:13: delete match arm "MSH" &#124; "OBX" in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:194:13: delete match arm "OBR" in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:195:13: delete match arm "ORC" in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:201:65: replace &#124;&#124; with && in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:201:43: replace &#124;&#124; with && in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:201:36: replace == with != in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:201:56: replace == with != in source_path` | Real gap, now caught by `mutation::rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times`. |
| `result.rs:201:78: replace == with != in source_path` | Real gap, now caught by `mutation::rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences`. |
| `result.rs:229:16: replace match guard !astm with true in status` | Real gap, now caught by `mutation::lis02_9_9_retransmitted_and_warning_statuses_never_take_hl7_meanings`. |
| `result.rs:230:22: replace match guard !astm with true in status` | Real gap, now caught by `mutation::lis02_9_9_retransmitted_and_warning_statuses_never_take_hl7_meanings`. |
| `result.rs:231:16: replace match guard !astm with true in status` | Real gap, now caught by `mutation::lis02_9_9_retransmitted_and_warning_statuses_never_take_hl7_meanings`. |
| `source.rs:127:22: replace > with == in Source<'_>::original_text` | Real gap, now caught by `mutation::rule7_value_source_text_bound_is_inclusive_in_both_syntaxes`. |
| `source.rs:127:22: replace > with >= in Source<'_>::original_text` | Real gap, now caught by `mutation::rule7_value_source_text_bound_is_inclusive_in_both_syntaxes`. |
| `source.rs:212:61: replace > with == in Source<'_>::repetitions` | Real gap, now caught by `mutation::rule7_value_source_text_bound_is_inclusive_in_both_syntaxes`. |
| `source.rs:212:61: replace > with >= in Source<'_>::repetitions` | Real gap, now caught by `mutation::rule7_value_source_text_bound_is_inclusive_in_both_syntaxes`. |
| `source.rs:273:47: replace && with &#124;&#124; in Source<'_>::selection_position` | Real gap, now caught by `mutation::rule2_missing_group_relative_sample_reports_its_actual_owner_position`. |
| `source.rs:273:28: replace == with != in Source<'_>::selection_position` | Real gap, now caught by `mutation::rule2_missing_group_relative_sample_reports_its_actual_owner_position`. |
| `source.rs:273:60: replace == with != in Source<'_>::selection_position` | Real gap, now caught by `mutation::rule2_missing_group_relative_sample_reports_its_actual_owner_position`. |
| `source.rs:360:16: replace match guard msh.version() == analyte_hl7::Version::V251 with true in hl7` | Real gap, now caught by `mutation::rule2_hl7_mapping_refuses_unqualified_structure_without_losing_original`. |
| `source.rs:519:13: delete match arm "PV1" &#124; "PV2" in hl7_segments` | Real gap, now caught by `mutation::rule3_hl7_visit_context_does_not_discard_the_result`. |
| `source.rs:420:22: replace match guard index == 0 with true in hl7_segments` | Equivalent. Message::parse rejects any MSH after segment zero (hl7/src/syntax.rs); the mapper receives an immutable parsed Message. |
| `source.rs:482:22: replace match guard specimen.is_some() with true in hl7_segments` | Real gap, now caught by `mutation::rule2_hl7_mapping_refuses_unqualified_structure_without_losing_original`. |
| `source.rs:491:47: replace match guard s > o with true in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oru_specimen_observations_keep_their_specimen_owner`. |
| `source.rs:491:47: replace match guard s > o with false in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oru_specimen_observations_keep_their_specimen_owner`. |
| `source.rs:491:49: replace > with == in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oru_specimen_observations_keep_their_specimen_owner`. |
| `source.rs:491:49: replace > with < in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oru_specimen_observations_keep_their_specimen_owner`. |
| `source.rs:491:49: replace > with >= in hl7_segments` | Equivalent. Only the >= replacement is equivalent: specimen and order are different source nodes (SPM versus OBR), so their indices can never be equal. |
| `source.rs:508:25: replace &#124;&#124; with && in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `source.rs:507:25: delete ! in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `source.rs:508:28: delete ! in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `source.rs:510:53: replace == with != in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `source.rs:534:43: replace && with &#124;&#124; in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `source.rs:534:26: replace && with &#124;&#124; in hl7_segments` | Real gap, now caught by `mutation::hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group`. |
| `time.rs:27:42: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:27:28: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:27:37: replace > with == in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:27:37: replace > with >= in parse` | Real gap, now caught by `mutation::rule5_numeric_offset_evidence_keeps_reported_components_without_inferring_zone`. |
| `time.rs:27:53: replace > with == in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:27:53: replace > with >= in parse` | Real gap, now caught by `mutation::rule5_numeric_offset_evidence_keeps_reported_components_without_inferring_zone`. |
| `time.rs:33:68: delete - in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:46:9: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:47:58: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:47:43: replace &#124;&#124; with && in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:47:54: replace > with < in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:64:9: delete match arm (4, None) in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:65:9: delete match arm (6, None) in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:66:9: delete match arm (8, None) in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `time.rs:67:9: delete match arm (10, None) in parse` | Real gap, now caught by `mutation::hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing`. |
| `value.rs:29:38: replace == with != in values` | Real gap, now caught by `mutation::rule2_operator_reason_distinguishes_uninterpreted_formatting_from_escape_agreement`. |
| `value.rs:52:17: delete match arm "" in values` | Real gap, now caught by `mutation::hl7_251_7_4_2_2_status_only_and_missing_populated_value_type_stay_distinct`. |
| `value.rs:51:23: replace match guard original.is_empty() with true in values` | Real gap, now caught by `mutation::hl7_251_7_4_2_2_status_only_and_missing_populated_value_type_stay_distinct`. |
| `value.rs:51:23: replace match guard original.is_empty() with false in values` | Real gap, now caught by `mutation::hl7_251_7_4_2_2_status_only_and_missing_populated_value_type_stay_distinct`. |
| `value.rs:98:9: delete match arm "<=" in comparator` | Real gap, now caught by `mutation::hl7_251_2a70_supported_numeric_comparators_and_ranges_keep_components`. |
| `value.rs:99:9: delete match arm ">" in comparator` | Real gap, now caught by `mutation::hl7_251_2a70_supported_numeric_comparators_and_ranges_keep_components`. |
| `value.rs:100:9: delete match arm ">=" in comparator` | Real gap, now caught by `mutation::hl7_251_2a70_supported_numeric_comparators_and_ranges_keep_components`. |
| `value.rs:138:63: replace &#124;&#124; with && in coded` | Real gap, now caught by `mutation::adr0003_coded_values_without_unambiguous_code_and_namespace_are_parked`. |
| `value.rs:175:9: replace &#124;&#124; with && in structured` | Real gap, now caught by `mutation::rule2_uninterpreted_numeric_comparator_never_becomes_equality`. |
| `value.rs:174:9: delete ! in structured` | Real gap, now caught by `mutation::hl7_251_2a70_supported_numeric_comparators_and_ranges_keep_components`. |
| `value.rs:174:36: replace != with == in structured` | Real gap, now caught by `mutation::rule2_uninterpreted_numeric_comparator_never_becomes_equality`. |
| `value.rs:219:25: replace && with &#124;&#124; in check_kind` | Real gap, now caught by `mutation::rule2_configured_coded_kind_refuses_empty_or_numeric_text_without_guessing`. |
| `value.rs:218:25: replace && with &#124;&#124; in check_kind` | Real gap, now caught by `mutation::rule2_configured_coded_kind_refuses_empty_or_numeric_text_without_guessing`. |
| `orders/display.rs:76:17: replace &#124;&#124; with && in map_display` | Real gap, now caught by `rule7_display_text_bound_is_inclusive_and_refuses_delimiter_injection`. |
| `orders/display.rs:75:28: replace > with == in map_display` | Real gap, now caught by `rule7_display_text_bound_is_inclusive_and_refuses_delimiter_injection`. |
| `orders/display.rs:75:28: replace > with >= in map_display` | Real gap, now caught by `rule7_display_text_bound_is_inclusive_and_refuses_delimiter_injection`. |
