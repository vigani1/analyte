# analyte-transport

Host-owned transport foundations. Bytes are opaque; TCP read boundaries are
not message boundaries. Queue acceptance does not prove peer receipt. An
interrupted write may already have sent a prefix and must not be retried blindly.

Each TCP pump is a caller-owned future. Dropping it closes the socket. Dropping
the dialog closes its queues and cancels the pump. Reads, writes and event queue
backpressure have configured deadlines. There are no detached runtime tasks.

Serial settings retain the exact port name. The replaceable blocking port seam
requires bounded reads and writes. `serial::run` owns a scoped dedicated thread;
call it outside the runtime's worker threads. It does not catch a thread panic
and resume a protocol session.

Folder operations are synchronous storage operations for a dedicated host
storage thread. A producer rename is preferred. Stable-file completion requires
a producer agreement to stop modifying the file after the stable interval.
Claimed receipts survive process failure. Recovery reopens `input` in each receipt
directory. Archive and quarantine use fresh directories and never delete input.
The maximum supported input size is 16 MiB. OS filesystem operations themselves
are not cancellable by this API.

## Recording format v1

Recordings contain protected raw bytes: keep them out of ordinary logs and use
synthetic or de-identified content for fixtures. The host must create a directory
with suitable access controls before enabling recording.

The file is append-only. Every record consists of `ATR1` (four ASCII bytes), a
one-byte kind (0=open/settings, 1=in, 2=out, 3=close/reason), a little-endian u128
monotonic elapsed nanosecond timestamp, a little-endian u32 payload length, then
that many exact bytes. Payloads are bounded to 4096 bytes. Elapsed time is relative
to worker creation; open starts a session. A reader must reject an incomplete
trailing record and must cap lengths before allocation. A session without a close
record is incomplete, never evidence of a clean disconnect.

The optional standalone tap uses a bounded nonblocking queue; saturation returns
an error. The host must surface that failure and the worker's file error. The
worker syncs each record. Drop the tap and join the worker to finish. An idle
worker reports a timeout after one second. Recording is not a durable receipt
store and cannot authorize a protocol acknowledgment.

## Incomplete integration

This is a foundation, not the completed transport task. Automatic recording tap
integration, explicit failed events, reconnect supervision with capped backoff and
jitter, server connection limits, serial vanished-port recovery, and a cancellable
folder watcher still need implementation. Do not use this crate for patient data.
