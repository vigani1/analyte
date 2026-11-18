# VIDAS checklist

Checked rows are synthetic regression coverage only, not machine qualification.

- [x] Put checksum before ETX; select Literal or Alternate packet boundary explicitly. Test: `checksum_precedes_etx`.
- [x] Require suffix and checksum-span/case settings; enforce 80-byte chunks and 1920-byte payload ceiling. Test: `every_suffix_and_boundary_round_trips`.
- [x] Stop a decoder after malformed checksum or excessive input. Test: `bad_checksum_or_oversize_is_refused`.
- [x] Preserve sample IDs, unknown tags and exact values; select 12 or 17 characters explicitly. Test: `keeps_exact_identity_unknown_tags_and_values`.
- [x] Wait for durable storage before packet ACK; preserve incomplete transfer evidence. Test: `ack_waits_for_store_and_keeps_incomplete_transfer`.
- [x] Never resend a work list after a lost ACK; park the uncertain outcome. Test: `download_never_resends_after_lost_ack`.
- [x] Keep qn, ql, nc, y3 and dilution as exact source text, without numeric conversion. Test: `keeps_exact_identity_unknown_tags_and_values`.
- [x] Treat completion as transport completion, not clinical or application acceptance. Test: `store_failure_parks_and_good_sessions_close`.
- [x] Leave result identity unresolved and park it; do not manufacture a run identity from sample ID. Test: `vidas_stores_before_ack_and_parks_unresolved`.
- [ ] Support one request per download session; exclude XML/FTP and other VIDAS families. Test: none yet.

- [ ] Qualify installed settings against a de-identified real conversation. Test: none yet.
