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
cannot extend the session or commit budget. Its `on` and arm/cancel reports
remain available for existing untimed event consumers and observers; live hosts
use `step` and `next_deadline`, and do not schedule those reports themselves.
