# 0004. Outbound connections only, orders polled

Status: accepted, 2026-11-15

## Context

Lab PCs sit behind routers and firewalls, and opening inbound ports from the
internet to a PC wired to analyzers is a security risk. Lab apps may be
hosted anywhere. Some machines ask for orders and expect an answer within
seconds (the cobas e 411 within about 15), faster than a round trip to a
remote app can be guaranteed.

## Decision

- Analyte only makes outbound connections to the lab app, over HTTPS with
  verified certificates.
- Results are pushed to the app with a delivery ID; retries reuse it.
- Orders are pulled: the app gives a full initial snapshot, then a versioned
  feed of changes since an opaque cursor, including deletions, expiry and a
  reset. Analyte polls it every few seconds and commits each cache update
  together with its cursor.
- Machine queries are answered from the cache within the machine's deadline,
  but only when the cache is complete and fresh for that sample under the
  deployment's offline policy. Otherwise Analyte never answers "no orders";
  it answers in the machine's protocol that the lookup failed, or not at all.
- Before sending an order to a machine, Analyte rechecks its current version
  and cancellation. A timeout after sending is an uncertain outcome to
  reconcile, never a reason to resend.

## Consequences

- No inbound internet ports at the lab.
- Orders reach machines after roughly one poll interval in normal operation;
  network problems and retries add to that.
- The cache needs freshness rules, and cancellations must be applied before
  an order is sent to a machine.
- Analyte keeps working through internet outages: results wait in the outbox,
  and query answers stop being given once the cache goes stale.
