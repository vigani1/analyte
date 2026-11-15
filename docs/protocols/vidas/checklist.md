# VIDAS checklist

Checked rows are synthetic regression coverage only, not machine qualification.

- [x] Put checksum before ETX; select Literal or Alternate packet boundary explicitly. Test: `vidas_bci_a_3_checksum_precedes_etx`.
- [x] Require suffix and checksum-span/case settings; enforce 80-byte chunks and 1920-byte payload ceiling. Test: `vidas_bci_a_3_suffixes_and_literal_boundary`.
- [x] Stop a decoder after malformed checksum or excessive input. Test: `vidas_bci_a_3_bad_checksum_and_bounds_stop`.
- [x] Preserve sample IDs, unknown tags and exact values; select 12 or 17 characters explicitly. Test: `vidas_bci_a_4_exact_identity_unknown_tags_and_values`.
- [x] Wait for durable storage before packet ACK; preserve incomplete transfer evidence. Test: `vidas_bci_a_3_adr0003_store_gate_and_incomplete_transfer`.
- [x] Never resend a work list after a lost ACK; park the uncertain outcome. Test: `vidas_bci_a_5_download_never_resends_after_lost_ack`.
- [x] Keep qn, ql, nc, y3 and dilution as exact source text, without numeric conversion. Test: `vidas_bci_a_4_exact_identity_unknown_tags_and_values`.
- [x] Treat completion as transport completion, not clinical or application acceptance. Test: `vidas_bci_a_3_storage_failure_and_successful_sessions`.
- [x] Leave result identity unresolved and park it; do not manufacture a run identity from sample ID. Test: `vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery`.
- [ ] Support one request per download session; exclude XML/FTP and other VIDAS families. Test: none yet.

- [ ] Qualify installed settings against a de-identified real conversation. Test: none yet.
