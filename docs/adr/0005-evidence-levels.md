# 0005. Evidence levels for drivers and machine configs

Status: accepted, 2026-11-15

## Context

Configs will come from us and from the community, often written from manuals
without the machine at hand. A lab needs to know how far each one was tested
before trusting it with patient results.

## Decision

Every protocol driver and machine config declares exactly one level, and the
docs show it:

- **proven:** tested against the real machine with recorded conversations;
- **simulated:** tested against an official simulator (for example IHE
  Gazelle) or vendor sample traces;
- **documented:** built from documents only.

A level is never raised without the evidence for it. It summarizes a
capability list (model, firmware, driver and config digest, transport, tested
dialogs, failure cases, platform, evidence files) and covers only what was
exercised: a recorded result upload proves results, not orders or
cancellation, and a simulator run covers only the actor and options it tested.

## Why

One honest word tells a lab what it can trust, and the capability list keeps
that word from claiming more than was tested.

## Consequences

- Labs still validate before go-live.
- Raising a config to proven needs a de-identified real recording, which
  becomes a test fixture.
- The level is part of the config and is checked in review.
