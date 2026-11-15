# 0002. Config first, code only where config cannot express it

Status: accepted, 2026-11-15

## Context

Most analyzers speak a standard protocol with their own test codes, field
choices and settings. If each machine needs code, every new machine is a
release and a review of code that touches clinical data. If config tries to
express everything, it turns into a hidden programming language.

## Decision

- A machine that speaks a supported protocol is a config file (TOML): its
  transport settings, protocol options, field selection and code mappings.
- Config is validated once when it is loaded, with specific errors.
- When config cannot express a behavior (a vendor framing, a polling dialog,
  an irregular record layout), it becomes a small code piece in the right
  layer, never a special case inside a shared one.
- Each receipt stores the exact config and driver version that read it.

## Consequences

- Labs and the community can add machines without writing Rust.
- The config format is a public interface and needs versioning and
  migration rules.
- Mappings stay bounded and typed; stateful or timed behavior lives in code,
  where it can be tested directly.
