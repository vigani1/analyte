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
- Orders are pulled: Analyte polls the app for "changes since" a cursor every
  few seconds and keeps a local order cache.
- Machine queries are answered from the cache within the machine's deadline.
  If the cache cannot be trusted (too old, never loaded), Analyte never
  answers "no orders"; it answers in the machine's protocol that the lookup
  failed, or not at all.

## Consequences

- No inbound internet ports at the lab.
- Orders reach machines with a delay of up to one poll interval.
- The cache needs freshness rules, and cancellations must be applied before
  an order is sent to a machine.
- Analyte keeps working through internet outages: results wait in the outbox,
  and query answers stop being given once the cache goes stale.
