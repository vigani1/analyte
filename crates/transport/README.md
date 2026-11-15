# analyte-transport

Host-owned byte transports. They do not frame, parse, acknowledge, or interpret
clinical messages. A successful queued write means queue acceptance, never peer
receipt. A cancelled or failed write may have sent a prefix; do not retry it
blindly. This crate is not a durable receipt store.

## Connections and ownership

`connection(Limits)` creates bounded event and write queues without starting a
task. Reads and queued writes contain at most 4096 bytes. Every connection has
one I/O owner: a caller-owned TCP future, a server-owned task, or a dedicated
blocking serial/storage thread. There are no detached tasks.

The event stream contains `Opened { session }`, exact `Received { at, bytes }`,
`Failed(reason)`, `Closed(reason)`, and independent `RecordingFailed` events.
Reasons distinguish peer disconnect, host cancellation, operation timeout, I/O
error kind and endpoint, missing serial device, and server connection limit.
Socket read boundaries are not protocol message boundaries. Received timestamps
use the monotonic clock immediately after the read completes.

Consume `Opened` before writing. Writes are tagged with that observed session;
unobserved reconnects reject writes, and stale queued writes are not replayed.
The session number is local transport state, not a clinical result identity.
Dropping a dialog requests cancellation; `cancellation()` requests shutdown while
keeping its event receiver open. Keep the owner and check its returned result:
when a consumer refuses events longer than the deadline, the owner returns an
explicit backpressure error. A saturated event queue cannot also guarantee room
for a terminal event. No success is implied by that termination.

## TCP

`bind` requires an explicit address. `client` supervises reconnects to exactly one
address with capped exponential delay and jitter. Supply a different scheduling
seed per connection. `server`/`server_recorded` take an explicitly bound listener,
a maximum connection count and a bounded notification sender. Excess sockets are
closed and reported as `Refused { event: Failed(LimitReached), .. }`. Accepted
connections each have their own ordered byte stream and owner task.

Cancel the dialog or server, then await its owner. TCP cancellation interrupts
connect, idle reads, active writes and retry delays. Event delivery is bounded by
the configured deadline. Server shutdown cancels and joins its owned task set
within twice that deadline; dropping the server future aborts its task set.
Cancellation never proves that an in-progress write had no effect.

## Serial

`Settings` validates the exact port name, baud, framing, flow control and read/write
timeout. `run_system` opens only that name. `run` provides the same supervision
with a fake factory for tests. A missing device is reported as `PortVanished`;
reopen attempts use capped backoff. An unrelated operating system I/O failure
retains its portable error kind. No device discovery or fallback port is used.

Call `run` or `run_system` from the host's blocking orchestration thread. It owns
and joins a scoped dedicated I/O thread. Port implementations and their factories
must bound their blocking operations; fake ports obey that same contract. During
normal serial I/O cancellation is observed within the configured port timeout,
plus any bounded event delivery already in progress. OS device open and filesystem
calls cannot be forcibly interrupted through the safe standard/library APIs;
shutdown guarantees assume responsive local drivers and filesystems. Do not put
these calls on a Tokio runtime worker.

## Folder handoff

`Watcher` is driven on a dedicated storage thread at the host's cancellable poll
interval. `scan` bounds each directory to 1024 entries. Inputs are bounded to
16 MiB. Completion uses either a producer agreement to rename complete files into
the incoming directory, or unchanged size and modification time for a configured
stable interval. Stable observation cannot prove completeness without that
producer agreement; rename handoff is preferred.

Each claim atomically renames input into a fresh receipt directory on the same
filesystem. Existing receipts are never replaced. `recover` finds files left in
the claimed area after a crash. `Claimed::deliver` emits a read-only common
connection, including its optional recorder; it does not delete or archive input.
After durable host acceptance, call `finish` with a fresh archive directory, or a
fresh quarantine directory after rejection. A cancelled/failed delivery leaves
the claimed file available for recovery. Filesystem errors preserve input and
include its path. This is process-crash recovery; the API does not promise
power-loss durability of directory renames.

## Optional recording: append-only format v1

Create `recording(path, capacity)`, attach its tap with `IoSide::with_recording`,
and run its `Worker` on an owned dedicated storage thread. For a TCP server,
`server_recorded` accepts a per-peer tap factory. Use a separate file/worker per
connection, including all reconnect sessions. TCP, serial and folder delivery
record exact incoming chunks, successful outgoing write prefixes, settings,
session boundaries and disconnect/failure reasons. Folder delivery is read-only.

The tap never waits for disk: its queue is bounded and nonblocking. Saturation,
worker closure and file failure produce a visible `RecordingFailed` event and
disable that tap; transport bytes continue unchanged. The worker's returned error
is authoritative even if the connection has already closed. The host must check
it. Drop all taps, then join the worker to drain and finish. Idle receives poll
with a one-second deadline and do not treat quiet connections as failed. Each
written record is synchronized. As with other blocking filesystem operations,
a stuck filesystem cannot be forcibly cancelled safely.

Protect the recording directory before starting: it contains raw clinical bytes.
Recordings never enter ordinary logs. Only synthetic/de-identified recordings
belong in repository fixtures.

Each record contains:

- `ATR1`: four ASCII bytes identifying format v1.
- Kind: u8, 0=settings/open, 1=in, 2=out, 3=close/failure reason.
- Timestamp: little-endian u128 nanoseconds from worker creation (monotonic).
- Payload length: little-endian u32, at most 4096.
- Payload: exactly that many bytes.

The file is opened append-only, never truncated. A reader must cap lengths before
allocating and reject incomplete trailing records. Each `open` starts a session;
a session without a close is incomplete. A new worker appended to an existing
file has a new time origin, identified by its next open. A failure event followed
by a closed event may produce two reason records. Recordings are evidence, not
permission to acknowledge a machine.

## Validation

Synthetic tests cover ordered TCP bytes, split/combined writes, disconnect,
client reconnect and cancellation during backoff, server refusal and shutdown,
serial vanish/reopen to the same name, jitter bounds, session write isolation,
recording round trips for TCP/serial/folder, recording queue/file failures,
half-written folder input and claimed-file crash recovery. No test contacts
hardware, live credentials or remote endpoints.
