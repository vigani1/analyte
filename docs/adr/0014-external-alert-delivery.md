# 0014. External alert delivery

Status: accepted, 2026-11-16

## Context

The operations plan requires alerts to reach named responders. The admin page
and status command show problems, but neither contacts a person. A lab needs
an outbound route that fits its own staff, and a failed notification must
never become a clinical delivery failure.

## Decision

- Analyte sends UTF-8 JSON alert events to explicitly configured HTTPS webhook
  endpoints. The lab owns the relay to email, chat or SMS and checks that its
  named responders receive them.
- Each event is `raised`, `reminder` or `resolved` for one condition (its kind,
  plus the machine ID for a silent machine), with a separate episode identity
  and a stored event identity and exact bytes that every retry reuses.
  Receivers deduplicate by event, never by condition.
- The payload is a closed list of operational fields. No patient data, sample
  IDs, raw messages, clinical values, credentials or remote answers ever enter
  an event or an ordinary log.
- The host owns endpoint validation, verified TLS, secrets, deadlines, bounded
  queues, capped backoff with a finite attempt budget, and stored progress that
  survives restarts. Exhausted or full queues stay visible and never delete
  events.
- Alert work never holds a store transaction or a clinical worker, and never
  gates intake, acknowledgment, order lookup or app delivery.

Fields, limits, relay rules and rehearsal steps are in
[Operate and respond](../operations/operate.md#alerts-and-named-responders).

## Why

A webhook to a lab-owned relay works with any messaging tool, keeps provider
credentials out of Analyte, and isolates notification trouble from patient
data flow.

## Consequences

- No built-in email, SMS, escalation chains or staff schedules; an HTTP success
  proves only that the relay accepted the event.
- A crash or restored backup can repeat an event with its original identity, so
  the relay must keep its deduplication records.
- Analyte cannot report its own total failure; the lab still monitors the
  service, store and disk independently.
