# 0024. One Needs review item per cause

Status: accepted, 2026-11-21. Refines
[ADR 0003](0003-clinical-safety-rules.md) rule 2 (never drop data). Its
example cause changed by [ADR 0029](0029-lab-app-owns-test-names.md): a code missing from a
test table no longer parks; a missing order ID or test code has the same shape.

## Context

One synthetic HL7 message with one test code missing from the test mapping
opened three Needs review items: the mapping finding ("test not in this
machine's test mapping"), the result's missing identity stated again as a
finding ("so its result has no identity"), and the store's own item for that
unresolved result ("result identity missing"). The second and third follow
from the first. VIDAS, Autolyser and POCT results without a trusted identity
opened two items each, and an IHE LAW message also opened "results held back
for review" next to its finding. Processing a message again while the cause
remained parked the same items again. An operator read one problem as several,
and the follow-on items stayed open after the cause was fixed.

## Decision

A problem is parked once, under the item that names its cause:

1. **The cause, not its consequences.** A result's missing identity is kept as
   a finding only when no finding of the same message already names its cause
   (an unknown test code, or the missing identity value at the same place);
   the mapping crate owns which reason restates which finding. A result that
   arrives without an identity gets no item of its own from the store when the
   reading carries findings. A reading that fails only because its findings
   held all results back (IHE LAW records all or none) parks only those
   findings.
2. **Never none.** Without any finding, the unresolved result or the held-back
   reading still parks its own item. A problem the store finds itself, such as
   conflicting content under one identity or an unchanged edit, keeps its own
   item, one per result.
3. **No copies.** Within one reading, and against the items the message
   already has open, an item with the same kind, reason, linked records and
   result is not parked again. Processing again while the cause remains keeps
   the new reading and its audit, and leaves the one open item as it is.

## Why

The operator's list must count problems, not the steps they passed through.
Every reason stays in the protected store: each result keeps its own
unresolved reason in the stored reading, and the original message and every
reading version are kept as before.

## Consequences

- The Needs review count, the sidebar badge, the Overview line and the alert
  count all count parked items, so they now count causes.
- Items parked before this change are not merged or resolved; they stay as
  they are until someone resolves or processes them again.
- A message with two different causes still has two items, one per cause;
  Process again on one resolves that one. (The original example, two unknown
  test codes, no longer parks since ADR 0029.)
