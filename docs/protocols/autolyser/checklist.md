# AUTOLYSER checklist

Checked rows are synthetic regression coverage only, not machine qualification.

- [x] Initiate a serialized STX/ACK exchange for each download or poll. Tests: `download_serializes_and_keeps_binary_reply`, `raw_first_and_l_recovers_before_next_r`.
- [x] Preserve fifteen-byte patient codes and four-byte assay codes, including spaces. Test: `checksum_is_decimal_sum`.
- [x] Encode routine/STAT, serum/urine, clone and cup position explicitly. Test: `worklist_fields_are_explicit`.
- [x] Preserve insertion success position and specific error byte. Test: `download_serializes_and_keeps_binary_reply`.
- [x] Serialize distinct R and L requests; refuse A during corrupt-report recovery. Test: `raw_first_and_l_recovers_before_next_r`.
- [x] Use an explicit controls-enabled layout and preserve seven-byte result text. Test: `controls_explicit_and_values_stay_written`.
- [x] Sum payload bytes modulo 256 and encode three decimal digits. Test: `checksum_is_decimal_sum`.
- [x] Store every complete response, including NAK, before interpretation. Tests: `empty_queue_still_requires_storage`, `raw_first_and_l_recovers_before_next_r`, `download_serializes_and_keeps_binary_reply`.
- [x] Block further R and A after corrupt data until valid L recovery; empty L cannot resolve corruption. Tests: `l_empty_does_not_resolve_corruption`, `raw_first_and_l_recovers_before_next_r`.
- [x] Require configured deadlines and byte bounds; store partial or oversized evidence and close. Tests: `bad_length_reports_stored_before_rejection`, `required_bounds_and_independent_deadlines`.
- [x] Keep a lost download response uncertain without automatic retry. Test: `lost_download_reply_is_uncertain_without_retry`.
- [x] Preserve pending Store completion after disconnect without reopening the dialog. Test: `disconnect_during_commit_never_reopens`.
- [x] Park unresolved immutable run identity and retain the exact result value with no inferred dilution. Test: `autolyser_corrupt_reply_blocks_r_until_valid_l`.
- [x] Follow the controls field layout rather than contradictory example spacing/checksum. Test: `controls_explicit_and_values_stay_written`.

- [x] Preserve the patient code as the sample association and leave it absent for a blank or fallback code. Test: `autolyser_keeps_report_code_without_inventing_sample_or_run`.
- [x] Expose report list, medium and the QC report fields; ordinary reports do not acquire control material from placeholders. Test: `autolyser_projects_controls_layout_and_leaves_normal_placeholders_raw`.

- [ ] Qualify installed settings against a de-identified real conversation. Test: none yet.
