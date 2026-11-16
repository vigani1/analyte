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

This ASTM module owns no ports, clocks, durable storage or syntax parsing.
The host implements storage/recovery and order lifecycles; the HL7 dialog is
described below. Multiple outgoing message queues remain unsupported.

# HL7 over MLLP dialog

Evidence: **documented**. `hl7::Dialog` uses the shared MLLP R1 decoder and
HL7 2.3.1/2.5.1 acknowledgment builder. Feed bytes individually and execute
all returned actions in order. Supply monotonic time and wake at `Deadline`.
The configurable wait bounds partial framing, commit, interpretation, response
metadata and outgoing acknowledgment waits. Garbage never extends a wait.

Each connection needs a unique host epoch. Journal every byte before feeding
it; commit actions contain exact complete MLLP blocks. Parked, rejected,
unmatched and queue-full actions contain evidence for protected storage,
never ordinary logs. Syntax failures cannot safely be correlated and receive
no acknowledgment. Duplicate IDs are never deduplicated here: each arrival
gets a new receipt so the host can reconcile duplicates and changed content.

After commit, original mode requests interpretation. Enhanced mode requests
fresh metadata for CA first, then interpretation; the shared builder applies
MSH-15/16 suppression independently. Supply a newly generated MSH-10 and current
time for **each** ResponseNeeded, including suppressed responses. The host owns
global response ID uniqueness. NotInterpretable selects AR for rejection or AE
for content errors and carries a bounded ERR code/location. Commit failure or
expiry selects AR/CR; interpretation expiry selects AR. Response metadata expiry
closes the dialog without a fabricated response. Late replies are refused.

The interpreter must distinguish generic ACK transactions from functional
order/query exchanges: those replies require their own later transaction layer,
not Interpreted as a substitute for a functional response. This module does not
implement query/order semantics, deferred MCF, application-ACK conversations,
restart replay, durable duplicate detection or clinical order lifecycle. A
received MSA is reported without generating another ACK. Sender outcomes retain
the six codes and MSA-2 correlation; timeout/disconnect means uncertain, never
an automatic retry. An enhanced CA means durable acceptance only, not completion
of a later application exchange. Use a new dialog after disconnect.

Checklist evidence in `tests/hl7.rs`: E original/enhanced selection, independent
AL/NE/ER/SU conditions and suppression, AA/AE/AR and CA/CR responsibility gates,
response IDs and MSA-2 correlation, all six MSA outcomes, structured 2.5.1 ERR,
late replies/store failures; K split and merged framing, bounded queue overflow,
partial EOF/deadline, invalid trailer, no byte handshake, sender uncertainty.
The shared framing/syntax crates own repertoire, encoding, full error rendering
and parser limits. Restart reconciliation and real durability latency tests
are covered by host tests in [receive.rs](../../analyte/tests/receive.rs) and
[HL7 orders](../../analyte/tests/orders/hl7_send.rs); no simulator or real machine
claim is made. The random-event property guards uncommitted positive responses.

## LAW connections

`law::Dialog` owns MLLP decoding, first-byte and per-reply send budgets,
transaction routing, QBP response/work ordering, and result ACK choice. The
same actions drive persistent links and both sides of independent links.
`law::WorkExchange` retains the original command and starts the ORL budget
only after transport issuance; transferring it between host connection owners
preserves its clock and reservation identity.

The host executes `Commit`, cache lookup, interpretation, transport write and
reservation cleanup actions. It supplies metadata and factual interpretation
outcomes, never an ACK code. AA requires both raw custody and successful durable
interpretation. A response never starts an ACK loop. Unconfirmed complete blocks,
partial frames and unread tails stay protected on cancellation or refusal.

Evidence remains **documented** (PaLM TF-2b Rev 11 B.6/B.7 and sections
3.27?3.29). `tests/law.rs` covers custody-before-ACK, error choices, deadline
boundaries, availability-before-work, independent-link rejection and uncertain
issuance without replay. Host integration tests cover both link topologies,
restart, failed storage and cache lookup, corrections, cancellation and ORL
correlation. No real-machine or simulator evidence is added by this refactor.
