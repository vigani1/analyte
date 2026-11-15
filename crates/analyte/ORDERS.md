# Orders in the program

Evidence level: **documented**. Tests use synthetic TCP peers and the repository's
reference app receiver. No real analyzer has qualified this implementation.

## Enable a machine explicitly

Append an optional `[orders]` section to the machine definition. The synthetic
[orders section](../config/examples/orders.toml) shows every required table.
It is separate from incoming result mappings. Without this section the host
answers no order queries and sends no work lists. `send_ahead` is explicit;
the presence of `query` enables queries and requires both a deadline and an
untrusted-cache policy.

Each outbound test maps an exact app `{namespace, value}` pair to explicit
ASTM universal-test components. Priorities also require explicit translations.
Specimen IDs retain their spelling. A required position missing from the app
order causes rejection; neither the sample ID nor a made-up slot substitutes
for it. Contract v1 currently supplies no order position.

The first outbound repertoire uses standard ASTM delimiters and ASCII without
reserved delimiter characters in data. Unrepresentable input is rejected,
never trimmed or changed. ASTM field destinations are restricted to their
standard O-record roles; optional instrument ID/position uses O field 4.

## Query scope

The supported LIS02 §11 subset requests orders with information status `O`,
an explicit specimen ID in the second starting-ID component, optional repeated
specimen IDs, and an empty or `ALL` test selector. Ranges, patient/vendor IDs,
historical/date filters and other request types follow the error/no-answer
policy. Status `A` cancels the pending lookup. One query is outstanding per
connection; a new transfer supersedes its pending answer.

The query deadline starts at the transfer's ENQ, before storage admission and
frame journaling. Answers use only a complete cache younger than 30 seconds,
the same freshness policy used by the app poller. Trusted empty answers use
no-information termination. Untrusted or late lookups produce query error or
silence according to the explicit policy. Mapping failures never produce a
partial order answer; the rejected order also gets a protected lifecycle event.

## Send-ahead and recovery

The host maps an order, then atomically reserves the current version in the
store immediately before starting its ASTM sender. The transaction rechecks
cache trust, version, withdrawal, expiry, requested state and analyzer targets.
It writes the internal `Sending` state and append-only history, with no app event.
Mapping rejections write `Rejected`, history and an event without claiming a
send occurred. App events use the existing immutable delivery outbox.

A reservation prevents another automatic send for that app order and machine,
including after a version change or restart. A link timeout, interrupted transfer
or disconnect before transfer confirmation becomes `Uncertain`. On restart,
only remaining `Sending` entries become `Uncertain` with an app event; confirmed `Sent` entries remain `Sent`.

LIS01 frame acknowledgments establish transfer completion, not application
acceptance of a work order. Successful transfers advance from `Sending` to `Sent` with one app event; they do not
invent an `Accepted` event. The lifecycle transition API supports explicit
machine acceptance/rejection evidence and operator reconciliation.

## Operator commands

```text
analyte --config analyte.toml --actor operator-name orders list
analyte --config analyte.toml --actor operator-name orders list --after 100 --machine analyzer-id
analyte --config analyte.toml --actor operator-name orders reconcile 101 --as accepted --reason "Verified in the machine work list"
```

Reconciliation accepts `accepted`, `rejected` or `cancelled` for an uncertain
entry. `cancelled` means cancellation was confirmed, not merely requested.
The local ID comes from `orders list`. Listing is bounded and audited;
reconciliation commits its state, history, operator audit and app event in one
transaction. Neither command sends or retries machine work.

## Deferred

HL7 QRY/QCK/DSR query handling and ORM/OML send-ahead are a separate round.
Live-machine validation, vendor-specific acceptance evidence and wider ASTM
query selectors are not claimed by these synthetic tests.

## Allowed lifecycle transitions

| From | Allowed next states |
| --- | --- |
| Requested | Sending, CancelRequested, Rejected |
| Sending (internal only) | Sent, Uncertain |
| Sent | Accepted, Rejected, CancelRequested, Uncertain |
| Accepted | Done, Rejected, CancelRequested, Uncertain |
| CancelRequested | CancelConfirmed, Done, Rejected, Uncertain |
| Uncertain | Accepted, Done, Rejected, CancelRequested, CancelConfirmed |
| Done, Rejected, CancelConfirmed | None |

All other transitions, including every attempt to return to Sending, are refused.
Contract conversion explicitly rejects Sending; only real transfer outcomes become
app events. The same reservation and completion rules apply to query answers.


## Durable command items

Every new machine-order reservation retains items atomically in schema v5.
ASTM and vendor records cover all tests in one item; HL7 send-ahead will reserve
one item per test. Store-owned placer identities are `<machine-order-id>-<index>`;
the HL7 configuration must supply their namespace. App order IDs belong in
ORC-4, independently of these identities. Neither identity may be truncated.

Application evidence is recorded per item. All Accepted, all Rejected, all Sent
or all CancelConfirmed produce that aggregate state; mixed or missing answers
produce Uncertain with per-item reasons. Cancellation covers all items. Explicit
refusal restores the preceding order state with a CancelRefused event/history;
missing evidence remains Uncertain. Orders list shows item identities, test
coverage and reasons. Reconciliation stays per order, audited, and never resends.

Migrated v4 orders explicitly mark original test coverage unavailable, because
v4 did not retain it. Their existing reservations continue to prevent replay.
The store API is implemented; HL7 program/config wiring remains deferred
until its own implementation steps are complete.
