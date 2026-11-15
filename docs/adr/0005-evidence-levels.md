# 0005. Evidence levels for drivers and machine configs

Status: accepted, 2026-11-15

## Context

Configs will come from us and from the community, often written from manuals
without access to the machine. A lab needs to know how far each one has been
tested before trusting it with patient results.

## Decision

Every protocol driver and machine config declares exactly one level, and the
docs show it:

- **proven:** tested against the real machine with recorded conversations;
- **simulated:** tested against an official simulator (for example IHE
  Gazelle) or vendor sample traces;
- **documented:** built from documents only.

A level is never raised without the evidence that justifies it.

The level is a summary of a capability list that names the model, firmware,
driver and config digest, transport, tested dialogs, failure cases, platform
and the evidence files. A level applies only to the capabilities that were
exercised: a recorded result upload proves results, not orders or
cancellation. A simulator result applies only to the actor and options it
tested.

## Consequences

- Labs can choose what to trust, and must still validate before go-live.
- Raising a config to proven needs a de-identified recording from a real
  machine, which becomes a test fixture.
- The level is part of the config and is checked in review.
