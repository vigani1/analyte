# AUTOLYSER decisions

Evidence level: documented. Sources below distinguish vendor rules from host
safety policy. Missing original-source verification remains explicit.

## 1. Initiate a serialized STX/ACK exchange for each download or poll.

- **what**: Initiate a serialized STX/ACK exchange for each download or poll.
- **why**: The host owns initiation in this protocol.
- **source**: Operator's Manual Rev18 section 23.1, p.175.
- **test**: `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`.

## 2. Preserve fifteen-byte patient codes and four-byte assay codes, including spaces.

- **what**: Preserve fifteen-byte patient codes and four-byte assay codes, including spaces.
- **why**: Fixed field widths carry meaning and padding is not trimming.
- **source**: Operator's Manual Rev18 section 23.1, p.175.
- **test**: `autolyser_rev18_p179_checksum_is_decimal_sum`.

## 3. Encode routine/STAT, serum/urine, clone and cup position explicitly.

- **what**: Encode routine/STAT, serum/urine, clone and cup position explicitly.
- **why**: They are distinct work-list attributes.
- **source**: Operator's Manual Rev18 section 23.1, pp.175-176.
- **test**: `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`.

## 4. Preserve insertion success position and specific error byte.

- **what**: Preserve insertion success position and specific error byte.
- **why**: A transport exchange can finish with rejected insertion.
- **source**: Operator's Manual Rev18 section 23.1, pp.175-176.
- **test**: `autolyser_rev18_p175_download_serializes_and_preserves_binary_reply`.

## 5. Expose R, L and A separately; do not automatically replay R or restart with A.

- **what**: Expose R, L and A separately; do not automatically replay R or restart with A.
- **why**: They move or revisit the report cursor in different ways.
- **source**: Operator's Manual Rev18 section 23.2, p.177; ADR 0010, Autolyser row.
- **test**: `autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r`.

## 6. Use an explicit controls-enabled layout and preserve seven-byte result text.

- **what**: Use an explicit controls-enabled layout and preserve seven-byte result text.
- **why**: Control fields change offsets; result tokens must not become guessed numbers.
- **source**: Operator's Manual Rev18 section 23.2, pp.177-178.
- **test**: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.

## 7. Sum payload bytes modulo 256 and encode three decimal digits.

- **what**: Sum payload bytes modulo 256 and encode three decimal digits.
- **why**: The checksum protects the transmitted data; exclusion of framing is an implementation interpretation of the consistent layouts.
- **source**: Operator's Manual Rev18 section 23.3, p.179 and section 23.1-23.2, pp.175-177.
- **test**: `autolyser_rev18_p179_checksum_is_decimal_sum`.

## 8. Store every complete response, including NAK, before interpretation.

- **what**: Store every complete response, including NAK, before interpretation.
- **why**: An empty queue response is still custody evidence.
- **source**: ADR 0010, Autolyser row; Operator's Manual Rev18 section 23.2, p.177.
- **test**: `autolyser_rev18_p177_empty_queue_still_requires_storage`.

## 9. Block further R and A after corrupt data until valid L recovery; empty L cannot resolve corruption.

- **what**: Block further R and A after corrupt data until valid L recovery; empty L cannot resolve corruption.
- **why**: A missing previous report must not be silently overwritten by cursor advance.
- **source**: ADR 0010, Autolyser row; recovery policy source: not found.
- **test**: `autolyser_rev18_p177_l_empty_does_not_resolve_corruption`.

## 10. Require configured deadlines and byte bounds; store partial or oversized evidence and close.

- **what**: Require configured deadlines and byte bounds; store partial or oversized evidence and close.
- **why**: The source does not establish safe timing defaults.
- **source**: ADR 0003, safety rule 7; vendor timing source: not found.
- **test**: `autolyser_rev18_p177_overlong_and_short_reports_are_stored_before_rejection`.

## 11. Keep a lost download response uncertain without automatic retry.

- **what**: Keep a lost download response uncertain without automatic retry.
- **why**: Insertion may already have happened.
- **source**: ADR 0003, safety rule 8; vendor retry source: not found.
- **test**: `autolyser_rev18_p175_lost_download_reply_is_uncertain_without_retry`.

## 12. Preserve pending Store completion after disconnect without reopening the dialog.

- **what**: Preserve pending Store completion after disconnect without reopening the dialog.
- **why**: Closing a connection does not cancel the need to retain its bytes.
- **source**: ADR 0003, Keep the original.
- **test**: `autolyser_rev18_p175_disconnect_during_commit_cannot_reopen_connection`.

## 13. Do not infer units, immutable run identity or timestamps absent from the layout.

- **what**: Do not infer units, immutable run identity or timestamps absent from the layout.
- **why**: The bridge cannot create evidence the analyzer did not send.
- **source**: Operator's Manual Rev18 section 23.2, pp.177-178; ADR 0003, Three identities.
- **test**: `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l`.

## 14. Follow the controls field layout rather than contradictory example spacing/checksum.

- **what**: Follow the controls field layout rather than contradictory example spacing/checksum.
- **why**: The example and layout cannot both define the same record.
- **source**: Operator's Manual Rev18 section 23.2, p.178.
- **test**: `autolyser_rev18_p178_controls_are_explicit_and_values_stay_written`.

