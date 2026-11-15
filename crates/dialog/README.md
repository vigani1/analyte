# ASTM link dialog

Evidence: **documented**, from CLSI LIS01-A2 sections 6 and 8 and the
Appendix A state diagram (visually inspected). No real analyzer or official
simulator has been exercised.

`Dialog::step(now, event)` does no I/O and never reads a clock. Feed every
received byte, regardless of serial read or TCP packet boundaries. TCP packets
never frame ASTM. Each call returns a bounded batch of ordered actions. Execute
writes promptly and wake at the returned absolute deadline. Configure reply
budgets to leave room for actual transport latency; time here denotes issuance
of a write, not proof that its last byte has reached the wire.

Before ENQ can be accepted, the host must reserve storage and report capacity.
A frame produces a `Commit` request. Only its matching successful host response
permits ACK (or the optional EOT interrupt). Protect raw action payloads like the
raw store; never log them. Store driver/config versions alongside the raw frame.
`Epoch` must be unique for each connection incarnation, including after restart;
commit identities expose its value, transfer ordinal and request ordinal for
journaling. Failed writes consume request identities too. Late responses are
visibly refused. Timeouts do not prove that storage or a machine operation did
not happen. The host must reconcile uncertain outcomes from its journal.

The machine keeps only the immediately preceding committed frame for exact
retransmission detection. It discards incomplete record assembly on EOT or
expiry, without instructing the host to delete any evidence. Rejected bytes,
conflicting frames and unsolicited frames are returned for protected evidence.
Ordinary garbage never refreshes a deadline. One outgoing message is accepted
at a time; bounded records are encoded as separate frames and never combined.
Send failure returns the outcome; the host retains the original message. A new
attempt always requires an explicit `Send`, so replay cannot resend an order.
Contention yields report an interrupted send attempt rather than automatically
resuming it after the peer releases the line.

Configuration validates nonzero budgets and ceilings; `Limit` validates the
wire bound. Standard defaults are 15/20/15/30/15 seconds for establishment,
computer contention, frame reply, receive and durable commit. Busy retry,
instrument contention and honored-interrupt holdoff are minimum 10/1/15 seconds.
Receiver interruption remains active until explicitly cleared. Ignoring an
incoming interrupt still accepts its frame, advancing to the next frame.

`step` rejects reversed or unrepresentable time without mutation. Other input
failures appear as `Refused` actions, so an expired deadline's effects are not
lost when a stale response arrives in the same call. Consume every action even
when a later action refuses the host event.

## Checklist evidence

Synthetic tests in `tests/link.rs` cover these serial sections; the paired TCP
rules in section 8 use exactly the same machine.

| LIS01 checklist sections | Test suffix / behavior |
| --- | --- |
| 6.1, 6.2.1-7 / 8.1, 8.2.1-7 | neutral_readiness_and_busy; sender_six_failures_and_eot |
| 6.2.7.1 / 8.2.7.1 | contention_both_roles |
| 6.3.1, 6.3.1.2 / 8.3.1, 8.3.1.2 | large_records_rollover_separate_records; etb_timeout_and_split_merge |
| 6.3.2 / 8.3.2 | large_records_rollover_separate_records; commit_duplicate_and_conflict |
| 6.3.3 / 8.3.3 | reject_checksum_sequence_and_controls; large_records_rollover_separate_records (shared encoder) |
| 6.3.4 / 8.3.4 | sender_six_failures_and_eot |
| 6.3.5 mandatory and optional / 8.3.5 | interrupt_honor_ignore_and_issue |
| 6.4 / 8.4 | sender_six_failures_and_eot; reject_checksum_sequence_and_controls |
| 6.5.1 / 8.5.1 | reject_checksum_sequence_and_controls; commit_duplicate_and_conflict |
| 6.5.1.2 / 8.5.1.2 | sender_six_failures_and_eot |
| 6.5.2.1, 6.5.2.3 / 8.5.2.1, 8.5.2.3 | busy_and_reply_timeouts |
| 6.5.2.2 / 8.5.2.2 | contention_both_roles |
| 6.5.2.4 / 8.5.2.4 | etb_timeout_and_split_merge |
| 6.5.2.5 / 8.5.2.5 | failed_and_unanswered_commit_never_ack; late_commit_and_failed_retry_have_distinct_identity |
| 6.5.2.6 optional / 8.5.2.6 | one_message_buffer_and_validated_vendor_values |
| 6.6 / 8.6 | reject_checksum_sequence_and_controls; shared framing codec tests |
| Appendix A | above transition tests; named enum states map idle/bid/receive/save/transmit/retry/interrupt/timeout paths |
| ADR 0003 | failed/unanswered/late commits, exact duplicates, conflicts, and two random-input properties |

Not implemented: ports, clocks, durable storage/restart replay, syntax parsing,
MLLP/HL7 dialogs, vendor polling, multi-message outgoing queues or clinical order
lifecycle decisions. Those belong to their owning layers or later work.
