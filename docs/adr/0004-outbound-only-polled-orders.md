# 0004. Outbound connections only, orders polled

Status: accepted, 2026-11-15

## Context

Lab PCs sit behind routers and firewalls, and an inbound internet port on a PC
wired to analyzers is a security risk. Lab apps may be hosted anywhere. Some
machines expect an answer to an order query within seconds (the cobas e 411
within about 15), faster than a remote round trip can be guaranteed.

## Decision

- Analyte only makes outbound HTTPS connections to the lab app, with verified
  certificates.
- Results are pushed with a delivery ID that every retry reuses.
- Orders are pulled: a full snapshot, then a versioned feed of changes since an
  opaque cursor, including deletions, expiry and a reset. Analyte polls every
  few seconds and commits each cache update together with its cursor.
- Machine queries are answered from that cache within the machine's deadline,
  but only when the cache is complete and fresh for that sample. Otherwise
  Analyte says in the machine's protocol that the lookup failed, or stays
  silent; it never answers "no orders".
- Before sending an order, Analyte rechecks its version and cancellation. A
  timeout after sending is an uncertain outcome to reconcile, never a reason
  to resend.

## Why

No inbound ports at the lab, and fast machine answers that do not depend on
the internet.

## Consequences

- Orders reach machines about one poll interval later; network trouble adds to
  that.
- The cache needs freshness rules, and cancellations apply before sending.
- Through an internet outage, results wait in the outbox and query answers
  stop once the cache goes stale.
