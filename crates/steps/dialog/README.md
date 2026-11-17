# dialog

**Kind: step.** One file per protocol where it has a role here (`astm.rs`, `hl7.rs` and so on).

**What it does:** Tracks each conversation: who speaks next, which reply is allowed and when time runs out.

**Where it sits:** Receives protocol events and host storage outcomes; returns actions for the host to carry out.

**What it must never do:** Read connections, store data or make clinical decisions; acceptance must wait for the required durable storage.

**Main parts:**

- `astm`: ASTM conversation rules.
- `hl7`: HL7 conversation rules.
- `law`: IHE LAW conversation rules.
- `poct`: POCT conversation rules.
- `vidas`: VIDAS conversation rules.
- `autolyser`: Autolyser conversation rules.

[Technical details and safety guidance](DETAILS.md).

Every connection uses `Clock` with host-supplied monotonic elapsed time, starting
at zero. Supply fresh time to `step(now, event)` for input, storage completion,
ticks and disconnects. Each dialog owns its deadlines and reports the next
absolute elapsed wake time through `next_deadline()`; `None` means wait for an
event or cancellation. The host waits until that deadline rather than polling
the protocol. Order-cache polling remains a separate host operation.

ASTM returns its decoder's partial bytes on disconnect. VIDAS schedules its
session, byte-gap, acknowledgment and commit timers internally; byte activity
cannot extend the session or commit budget. Its named timer arm/cancel reports
are observer effects; hosts use `step` and `next_deadline`, and do not schedule
those reports themselves.

## Shared dialog vocabulary

All six dialogs use `step(now, event)`. These names describe the same host
concept wherever that concept occurs; payload shapes remain protocol-specific.

| Concept | Name | Host contract |
| --- | --- | --- |
| Durable storage request | `Action::Store` | Retain exact evidence and driver/config versions before reporting completion. |
| Successful storage completion | `Event::Stored` | Echo the outstanding request identity; VIDAS has one implicit outstanding packet. POCT also reports the durably retained interpretation outcome. |
| Storage failure | `Event::StoreFailed` | Never permits a positive acknowledgment. ASTM also reports `Action::StoreFailed` on failure or expiry. |
| Transport output | `Action::Write` | Issue exact bytes, or encode the typed POCT message through its protocol grammar. |
| Transport ended | `Event::Disconnected` | Preserve partial evidence; never grants permission to replay orders. |
| Close transport | `Action::Close` | Stop the transfer; host retains unresolved work. |
| Wake without input | `Event::Tick` | Evaluate dialog-owned deadlines with fresh monotonic time. |
| Next wake | `next_deadline()`, `Action::Deadline` where emitted | Absolute elapsed time; `None` waits for input or cancellation. |
| Refuse host input | `Action::Refused` | Preserve earlier ordered effects and report the specific error. |
| Reject wire input | `Action::Rejected` where emitted | Retain framing evidence and its specific reason. |
| Invalid settings | `Error::Configuration` | Construction fails before starting a session. |
| Reversed time | `Error::TimeReversed` | Reject before mutating the conversation. |
| Deadline or token overflow | `Error::Exhausted` | Stop rather than wrap a deadline or identity. |

POCT's `Parse`, `ParseLimit` and `ParseDeadline` actions ask the host to
interpret framed bytes; they are not storage requests or confirmations. Its
`StoreOutcome` describes successful durable custody, including parked semantic
errors; failed custody uses `StoreFailed`. VIDAS's `TimerArmed` and
`TimerCancelled` observer effects name its protocol timers and report relative
budgets, not a host wake schedule. Protocol-specific ACK, query, polling,
work and transfer outcomes retain their protocol names.

Storage identities remain distinct from clinical result and delivery identities.
Autolyser uses `StoreId` rather than a bare integer; its ordinal is meaningful
only with the host's unique connection incarnation. Hosts echo tokens unchanged.

HL7 and LAW share `ExchangeId` for host correlation and `Write` for bounded
MLLP output. Its epoch and ordinal identify a dialog exchange, never a stored
clinical receipt. HL7 receives the host's unique connection epoch; LAW uses
zero because its host owns one outstanding exchange and retains the connection
incarnation separately. Hosts echo the whole token unchanged. `Write::until`
is an absolute connection-clock issuance deadline when supplied; otherwise the
host's transport budget applies. LAW reports typed `Parked` failures, and HL7
uses `AckChoice` to distinguish rejection from a content error.

## Construction and completion correlation

A dialog that accepts a host incarnation uses `new(config, Epoch)`; ASTM and
HL7 share the same 128-bit `Epoch`, allowing a fresh UUID without truncation.
Constructors without a host incarnation take only the protocol inputs they
need: LAW link and timer budgets, POCT's wire session control ID, VIDAS framing
and policy, and Autolyser configuration. A wire session ID is not a host epoch.
Hosts call their per-connection monotonic time base `start`.

The completion pattern is always: retain the outstanding request identity,
commit the exact evidence, then echo that identity unchanged through `Stored`
or `StoreFailed`. Never derive a clinical result identity from these tokens.
The identity shapes deliberately follow the protocol's custody boundary:

| Dialog | Correlation | Why it differs |
| --- | --- | --- |
| ASTM | `CommitId` with `Epoch`, transfer ordinal and frame ordinal | Each frame needs its own durable ACK gate; transfer and frame counters cannot be collapsed to the wire's wrapping frame number. |
| HL7 / LAW | `ExchangeId` | One complete MLLP message is the custody unit. HL7 supplies a fresh host epoch; LAW owns one outstanding exchange and keeps its connection incarnation in the host. |
| Autolyser | `StoreId` | A connection-local packet ordinal is sufficient; the host owns the connection incarnation and never routes completions across hosts. |
| POCT | `CommitId` | Correlation includes protocol session identity, sequence and interpretation custody, rather than an MLLP exchange. |
| VIDAS | One implicit outstanding packet | The host serially awaits storage before driving the next dialog event; a separate token would add no correlation check. |

A universal token would either discard ASTM/POCT distinctions or require unused
fields in the other protocols. Keep these protocol-owned tokens and the same
echo-and-match completion pattern. Host maps use the whole typed token.
