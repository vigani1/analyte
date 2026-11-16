# AUTOLYSER checklist

Checked rows are synthetic regression coverage only, not machine qualification.

- [x] Initiate a serialized STX/ACK exchange for each download or poll. Tests: `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`, `autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r`.
- [x] Preserve fifteen-byte patient codes and four-byte assay codes, including spaces. Test: `autolyser_rev18_p179_checksum_is_decimal_sum`.
- [x] Encode routine/STAT, serum/urine, clone and cup position explicitly. Test: `autolyser_rev18_section23_1_p175_explicit_worklist_fields`.
- [x] Preserve insertion success position and specific error byte. Test: `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`.
- [x] Serialize distinct R and L requests; refuse A during corrupt-report recovery. Test: `autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r`.
- [x] Use an explicit controls-enabled layout and preserve seven-byte result text. Test: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.
- [x] Sum payload bytes modulo 256 and encode three decimal digits. Test: `autolyser_rev18_p179_checksum_is_decimal_sum`.
- [x] Store every complete response, including NAK, before interpretation. Tests: `autolyser_rev18_p177_empty_queue_still_requires_storage`, `autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r`, `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`.
- [x] Block further R and A after corrupt data until valid L recovery; empty L cannot resolve corruption. Tests: `autolyser_rev18_p177_l_empty_does_not_resolve_corruption`, `autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r`.
- [x] Require configured deadlines and byte bounds; store partial or oversized evidence and close. Tests: `autolyser_rev18_p177_overlong_and_short_reports_are_stored_before_rejection`, `autolyser_rev18_section23_2_p177_required_bounds_and_independent_deadlines`.
- [x] Keep a lost download response uncertain without automatic retry. Test: `autolyser_rev18_p175_lost_download_reply_is_uncertain_without_retry`.
- [x] Preserve pending Store completion after disconnect without reopening the dialog. Test: `autolyser_rev18_p175_disconnect_during_commit_cannot_reopen_connection`.
- [x] Park unresolved immutable run identity and retain the exact result value with no inferred dilution. Test: `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l`.
- [x] Follow the controls field layout rather than contradictory example spacing/checksum. Test: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.

- [ ] Qualify installed settings against a de-identified real conversation. Test: none yet.
