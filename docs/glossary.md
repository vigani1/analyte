# Domain glossary

Shared terms are defined here once. Protocol-specific wire terms remain in the protocol
notes. Each definition links to the architectural decision that sets its meaning or
ownership.

## Receipt

One retained arrival from a machine, with its raw bytes, receive context and
driver/config versions. It is distinct from a result and from a delivery. [ADR
0003](adr/0003-clinical-safety-rules.md).

## Delivery

One immutable document addressed to a lab app. Its ID and body stay the same on retries
and operator resends. [ADR 0008](adr/0008-app-contract.md).

## Result identity

The trusted evidence that distinguishes an observation and run in its order context.
Missing evidence stays unresolved; sample ID alone is insufficient. [ADR
0003](adr/0003-clinical-safety-rules.md).

## Revision

A store-assigned version of one result. A permitted status progression, correction or
withdrawal can create the next revision; a retransmission does not. [ADR
0011](adr/0011-result-status-progression.md).

## Custody

Responsibility for keeping received data durably before promising acceptance or
advancing a destructive read. The positive step differs by protocol. [ADR
0010](adr/0010-acknowledgment-boundaries.md).

## Parked item

Retained work that cannot safely proceed, together with a specific reason visible to an
operator. Parking never deletes its evidence. [ADR
0003](adr/0003-clinical-safety-rules.md).

## Evidence level

A statement of how a particular capability was tested: documented from documents,
simulated with an official simulator or vendor traces, or proven with real-machine
recordings. It does not qualify untested capabilities. [ADR
0005](adr/0005-evidence-levels.md).

## Stack

The configured combination of transport, framing, dialog and syntax for a machine
connection, followed by mapping into the common model. [ADR
0001](adr/0001-rust-and-layered-architecture.md).

## Departure

A named, evidence-backed difference from a standard selected in machine configuration.
The current config type calls it a deviation; it must not silently change shared
standard behavior. [ADR 0002](adr/0002-config-first.md).

## Recording

A bounded `.rec.jsonl` conversation: a header followed by timed, directed byte events
and optional link events. Only synthetic or de-identified material belongs in the
repository. [ADR 0009](adr/0009-recording-format.md).

## Replay

Running the machine side of a recording and comparing host output against its expected
bytes under configured deadlines and declared exceptions. [ADR
0009](adr/0009-recording-format.md).

## Transport

The layer that moves bytes over serial, TCP or files. Packet/read boundaries alone do
not define application messages. [ADR 0001](adr/0001-rust-and-layered-architecture.md).

## Framing

The layer that identifies byte boundaries and verifies the framing checksum where the
selected protocol has one. [ADR 0012](adr/0012-uniform-layout-and-naming.md).

## Dialog

A connection-owned state machine that decides who speaks next, which acknowledgment is
allowed and when deadlines expire. [ADR
0001](adr/0001-rust-and-layered-architecture.md).

## Syntax

The grammar layer that turns bytes into protocol structure and encodes that structure
back to bytes. [ADR 0012](adr/0012-uniform-layout-and-naming.md).

## Mapping

The conversion between protocol fields or machine codes and the common model, controlled
by explicit configuration. [ADR 0002](adr/0002-config-first.md).

## Common model

The shared representation of results, orders and sample identity used across protocols
and the app contract. [ADR 0001](adr/0001-rust-and-layered-architecture.md).

## Machine definition

The shipped configuration identifying one machine's protocol, settings, mappings,
departures and evidence. Site settings and credentials are separate. [ADR
0012](adr/0012-uniform-layout-and-naming.md).

## Effective configuration

The exact combination of shipped definition, site settings and overrides used for a
receipt, retained with its digest without secrets. [ADR 0002](adr/0002-config-first.md).

## Sample ID

The lab's specimen identifier as a string, preserving leading zeros and case. It is
never inferred from a physical position. [ADR 0003](adr/0003-clinical-safety-rules.md).

## Sample position

Physical loading location such as a rack, slot or cup. It accompanies identity where
available and never substitutes for it. [ADR 0003](adr/0003-clinical-safety-rules.md).

## Order

The lab app's authorized request for tests on a sample. Sent, machine-accepted, done and
cancelled are separate states. [ADR 0008](adr/0008-app-contract.md).

## Uncertain order

An order whose external outcome is unknown after an interrupted send or lost response.
It requires reconciliation and is never automatically resent to a machine. [ADR
0008](adr/0008-app-contract.md).

## Order cache

The local retained view of app-authorized orders fetched through the changes-since
contract. A failed lookup is not an empty order list. [ADR
0008](adr/0008-app-contract.md).

## Outbox

Durable pending app deliveries, each with a stable delivery identity and retry state.
[ADR 0008](adr/0008-app-contract.md).

## Interpretation

The retained outcome of parsing and mapping a receipt, including findings. Reprocessing
creates a new version without overwriting the original evidence; it is not a clinical
decision. [ADR 0003](adr/0003-clinical-safety-rules.md).

## Retransmission

The same trusted result identity with identical content. It creates no new revision or
result delivery. [ADR 0003](adr/0003-clinical-safety-rules.md).

## Correction

Changed content explicitly marked by the source's correction semantics for the same
trusted result identity. It becomes a new revision. [ADR
0003](adr/0003-clinical-safety-rules.md).

## Rerun

Another execution of a test, with a distinct result identity when the machine supplies
enough evidence to establish it. [ADR 0003](adr/0003-clinical-safety-rules.md).

## Conflict

Different content or incompatible status history under an identity that does not justify
a new revision. It is parked instead of resolved by guessing. [ADR
0011](adr/0011-result-status-progression.md).

## Status progression

A protocol-declared permitted move in a result's lifecycle, such as preliminary to
final. The store applies the syntax-owned rule to assign revisions. [ADR
0011](adr/0011-result-status-progression.md).

## Acknowledgment

A protocol response whose promise depends on the configured mode and custody boundary;
it never automatically means the app or a clinician accepted the result. [ADR
0010](adr/0010-acknowledgment-boundaries.md).

## Bridge

The product boundary: receive, retain and deliver faithfully, and answer machine queries
from app-authorized orders, without clinical decisions. [ADR
0001](adr/0001-rust-and-layered-architecture.md).

