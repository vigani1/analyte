# 0014. External alert delivery

Status: accepted, 2026-11-16.

## Context

The operations plan requires alerts to reach named responders. The local admin
page and status command retain useful evidence, but neither contacts a responder.
A lab needs an outbound route that fits its own staffing and communications.
Notification failures must not become clinical delivery failures.

## Decision

Analyte sends UTF-8 JSON alert events to one or more explicitly configured HTTPS
webhook endpoints. The lab owns the relay from those endpoints to email, chat or
SMS and verifies that its named responders receive them. Enabling local policies
without endpoints remains supported and does not imply external notification.

The existing kinds are oldest undelivered, machine silent, disk reserve, order
cache stale, uncertain orders, parked items and restore paused. A condition has
one stable identity within an installation: its kind and, for machine silence,
the configured machine ID. Threshold or responder changes do not change it.
An event distinguishes `raised`, `reminder` and `resolved`. Repeated episodes
share the condition identity, but have distinct episode identities. Each event
also has a persisted delivery identity and immutable bytes; retries reuse both.
Receivers deduplicate by event identity, not condition identity, so a reminder
or a later episode cannot be mistaken for a retransmission.

Each event names the responder from its validated policy. Payload construction
uses a closed list of operational fields: identities, event type, kind, configured
machine ID when applicable, responder, observation time, episode time and the
signal's count, age in seconds or free disk bytes. Unknown measurements remain
explicitly unknown. No patient data, sample IDs, raw messages, clinical values,
parking explanations, remote response bodies or credentials enter an event.
Responder and machine configuration must contain operational names only.

The host owns endpoint validation, verified TLS, credentials, deadlines, queueing
and retries. Reuse the sink's reqwest/rustls configuration without adding a
dependency. URLs cannot contain credentials, queries or fragments; redirects
are disabled. Credentials are installation secrets referenced by name, never
repository values. Ordinary logs contain only operational identities, attempt
counts and fixed failure categories, never secrets, URLs or response bodies.

Persist episode state, immutable pending events, per-endpoint delivery progress,
retry deadlines and reminder deadlines in versioned store migrations. A crash
after the peer accepts but before progress commits can repeat an event with its
original identity and bytes. A restart does not generate a fresh raise or reset
the reminder interval. A restored older backup can repeat events; the relay must
retain deduplication records and the lab must include this in restore rehearsal.

Configuration validates endpoint count, unique endpoint identities, secret
references, policy responders, reminder interval, queue capacity, request timeout
and retry bounds once at the installation boundary. Reminders are generated at
the configured interval while active; missed intervals do not create a burst.
Resolution follows the observed condition, never an assumption that an outage
ended. Configuration removal does not invent a resolution or erase pending work.

Use bounded independent workers and a bounded pending queue per endpoint.
Attempts use capped exponential backoff and a finite attempt budget per event;
exhausted events remain visibly failed for operator action. Queue exhaustion is
visible, preserves transition evidence and stops queue admission rather than
deleting pending events. Resume from retained transition progress when capacity
returns. At most one outstanding reminder per condition and endpoint avoids an
unbounded reminder backlog. State and queue work uses bounded store operations;
network waits never hold a store transaction or a clinical worker. Alert failures
must never gate clinical intake, acknowledgment, order lookup or app delivery.

The admin overview and status expose each endpoint's pending count, oldest
pending age, last successful delivery, retry state, exhausted failures and queue
exhaustion. An alert delivery failure does not recursively enqueue another alert.
Failure to read status stays an explicit failure, never a healthy empty queue.
Independent host monitoring remains necessary for a stopped process, unreadable
store or failed disk; Analyte cannot promise to report its own total failure.

## Scope and consequences

Direct SMTP and SMS integrations are out of scope: the lab's relay owns provider
credentials and provider-specific delivery rules. Escalation chains, acknowledgments
by people and staff scheduling are also out of scope because staffing policy
belongs to the lab. An HTTP success proves acceptance by the relay, not receipt
or action by a person.

This adds host notification delivery and storage migration tests, without changing
analyzer protocols, clinical identities or the lab app contract. Local synthetic
receiver tests must cover lifecycle events, restart, retry ambiguity, queue
capacity, slow peers, secret redaction and the closed payload field list. Pilot
instructions must require an end-to-end relay rehearsal and named responder cover.
