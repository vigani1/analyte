# Machine definitions

Each definition lives in `machines/<vendor>/<model>/`, with lower-case,
hyphenated names. It contains `machine.toml`, `README.md` and `recordings/`.
Keep site settings and credentials outside this tree. Recordings follow
[ADR 0009](../docs/adr/0009-recording-format.md) and contain only synthetic
or reviewed de-identified data. Real machines must have their own evidence;
the example below is not a real machine.

`cargo test --workspace` discovers every `machine.toml`, validates it through
the real loader, compares the README evidence level, and replays every recording
through the running service. An empty inventory passes and prints that fact.
The README uses these sections in this order. In Evidence, write exactly one
`Level: documented`, `Level: simulated` or `Level: proven` line matching TOML.
Synthetic recordings never raise the evidence level.

## README template

```markdown
## Machine
Describe the confirmed facts, sources and unresolved details.

## Connection
Describe the confirmed facts, sources and unresolved details.

## Settings on the machine
Describe the confirmed facts, sources and unresolved details.

## Supported workflows
Describe the confirmed facts, sources and unresolved details.

## Departures
Describe the confirmed facts, sources and unresolved details.

## Evidence
Level: documented

## Open questions
Describe the confirmed facts, sources and unresolved details.

## Sources
Describe the confirmed facts, sources and unresolved details.
```
