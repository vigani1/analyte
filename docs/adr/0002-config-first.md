# 0002. Config first, code only where config cannot express it

Status: accepted, 2026-11-15. Changed by [ADR 0029](0029-lab-app-owns-test-names.md): a machine's code
mappings name its own code system, never the lab app's codes; and by
[ADR 0030](0030-lab-app-owns-specimen-codes.md), which applies this to specimen types.

## Context

Most analyzers speak a standard protocol with their own test codes, field
choices and settings. If every machine needs code, every new machine is a
release and a code review on clinical data. If config tries to express
everything, it becomes a hidden programming language.

## Decision

- A machine that speaks a supported protocol is a TOML config file: transport
  settings, protocol options, field selection and code mappings. (Changed by
  ADR 0029: the code mappings are the machine's own code system and the code
  facts needed to read its messages; the lab app keeps the test list.)
- Config is validated once, when loaded, with specific errors.
- What config cannot express (a vendor framing, a polling dialog, an irregular
  record layout) becomes a small code piece in the right layer, never a special
  case inside a shared one.
- Each receipt stores the exact effective config (shipped definition plus site
  settings and overrides, without secrets), its digest and the driver version.
- A config goes draft → validated → active and switches only between
  conversations. Activation never changes queued deliveries or resends an order.

## Why

Labs and the community can add machines without writing Rust, while stateful
or timed behavior stays in code where it can be tested directly.

## Consequences

- The config format is a public interface and needs versions and migration
  rules.
- Mappings stay bounded and typed.
