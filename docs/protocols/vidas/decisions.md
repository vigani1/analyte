# VIDAS decisions

Evidence level: documented. Sources below distinguish vendor rules from host
safety policy. Missing original-source verification remains explicit.

## 1. Put checksum before ETX; select Literal or Alternate packet boundary explicitly.

- **what**: Put checksum before ETX; select Literal or Alternate packet boundary explicitly.
- **why**: The two ACK boundaries are different conversations.
- **source**: not found.
- **test**: `vidas_bci_a_3_checksum_precedes_etx`.

## 2. Require suffix and checksum-span/case settings; enforce 80-byte chunks and 1920-byte payload ceiling.

- **what**: Require suffix and checksum-span/case settings; enforce 80-byte chunks and 1920-byte payload ceiling.
- **why**: Installed settings cannot be inferred from ASTM.
- **source**: not found.
- **test**: `vidas_bci_a_3_suffixes_and_literal_boundary`.

## 3. Stop a decoder after malformed checksum or excessive input.

- **what**: Stop a decoder after malformed checksum or excessive input.
- **why**: Continuing could lose framing boundaries.
- **source**: not found.
- **test**: `vidas_bci_a_3_bad_checksum_and_bounds_stop`.

## 4. Preserve sample IDs, unknown tags and exact values; select 12 or 17 characters explicitly.

- **what**: Preserve sample IDs, unknown tags and exact values; select 12 or 17 characters explicitly.
- **why**: Changing case, punctuation or decimal notation changes evidence.
- **source**: not found.
- **test**: `vidas_bci_a_4_exact_identity_unknown_tags_and_values`.

## 5. Wait for durable storage before packet ACK; preserve incomplete transfer evidence.

- **what**: Wait for durable storage before packet ACK; preserve incomplete transfer evidence.
- **why**: ACK transfers custody.
- **source**: ADR 0010, Decision, VIDAS row; vendor source: not found.
- **test**: `vidas_bci_a_3_adr0003_store_gate_and_incomplete_transfer`.

## 6. Never resend a work list after a lost ACK; park the uncertain outcome.

- **what**: Never resend a work list after a lost ACK; park the uncertain outcome.
- **why**: Timeout does not prove the PC failed to insert it.
- **source**: ADR 0003, safety rules 7-8; vendor source: not found.
- **test**: `vidas_bci_a_5_download_never_resends_after_lost_ack`.

## 7. Keep qn, ql, nc, y3 and dilution as exact source text, without numeric conversion.

- **what**: Keep qn, ql, nc, y3 and dilution as exact source text, without numeric conversion.
- **why**: The research records contradictory value descriptions and locale-dependent formats.
- **source**: not found.
- **test**: `vidas_bci_a_4_exact_identity_unknown_tags_and_values`.

## 8. Treat completion as transport completion, not clinical or application acceptance.

- **what**: Treat completion as transport completion, not clinical or application acceptance.
- **why**: The PC can reject an assay after transport ACK.
- **source**: not found.
- **test**: `vidas_bci_a_3_storage_failure_and_successful_sessions`.

## 9. Leave result identity unresolved and park it; do not manufacture a run identity from sample ID.

- **what**: Leave result identity unresolved and park it; do not manufacture a run identity from sample ID.
- **why**: Neither a sample identifier nor a value distinguishes reruns and corrections.
- **source**: ADR 0003, Three identities; vendor source: not found.
- **test**: `vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery`.

## 10. Support one request per download session; exclude XML/FTP and other VIDAS families.

- **what**: Support one request per download session; exclude XML/FTP and other VIDAS families.
- **why**: Those formats and multi-packet downloads have no qualified implementation here.
- **source**: not found.
- **test**: none yet.

