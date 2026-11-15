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
for it. Contract v1 can carry position; ASTM forwarding is a separate step.

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


## VIDAS Literal work lists

Evidence remains **documented**, from the existing VIDAS research notes (BCI A
sections 3-5 and PC sections 10-24 through 10-26), not machine recordings.
Enable both `[vidas] download = true` and `[orders] send_ahead = true`, with
`patient_namespace` naming the accepted patient assigning system. Use the same
explicit outbound test table, with exactly one 1-4 byte assay code in each
`machine_components` entry. Omit `[orders.astm]`. The priority table admits only
explicitly listed app priorities; Literal has no priority tag to transmit.

The shared candidate/mapping/reservation path maps every assay before reserving
one app order as Sending. VIDAS transmits one request per assay through its
existing download dialog. Only completion of every request produces one Sent
event (`VIDAS-transfer-confirmed`), never Accepted. A failed assay stops the
entire reserved order as Uncertain, including any already transferred assays;
it is never automatically retried. Restart recovery uses the shared Sending
recovery path; Sent stays Sent.

Missing patient, mismatched patient namespace, and patient/sample IDs exceeding
17 bytes or the installed sample limit respectively are rejected before any
transmission. IDs are copied exactly. Literal has no specimen-type tag: every
supplied specimen type is rejected as `unmapped-specimen-type`, rather than
being silently lost or placed in `so` (origin) or `si` (empty structural marker).
The request includes only `mt`, `pi`, `si`, `ci` and `rt`; no demographics,
position, dilution or other context is invented.

Uncertain transfer codes distinguish acknowledgment/interbyte/session deadlines,
disconnection, shutdown cancellation, write/input failures, framing/suffix
failures and refusal/unexpected input. Raw incoming bytes are still journaled
before the dialog receives them. Patient identifiers occur only in protected
order evidence and wire data, not ordinary logs.

`tests/orders/vidas.rs` drives the program with a temporary store and the
reference receiver: multi-assay completion, timeout, mid-send disconnect,
restart during Sending, Sent surviving restart, no replay, unmapped test and
specimen type, wrong/missing patient context, over-long IDs and cancellation
before sending. Autolyser sending remains deferred.
