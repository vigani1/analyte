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

## Consequences

- Labs can choose what to trust, and must still validate before go-live.
- Raising a config to proven needs a de-identified recording from a real
  machine, which becomes a test fixture.
- The level is part of the config and is checked in review.
