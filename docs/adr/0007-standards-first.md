# 0007. Standards first, machines after

Status: accepted, 2026-11-15

## Context

Analyte is a general product, not a connector for one lab. If protocols are
built only as far as one machine needs, each later machine finds gaps, and
the code fills up with one machine's quirks presented as the standard.

## Decision

- Each standard (ASTM LIS01/LIS02, HL7 v2 with MLLP, IHE LAW, POCT1-A) is
  implemented completely from its official documents and tested against
  official examples or simulators, independent of any particular machine.
- Each standard's phase starts by writing its scope: the versions, actor
  roles, message types, options, acknowledgment modes and limits supported,
  with explicit exclusions. "Complete" means complete within that scope, so
  the work has a finish line.
- A standard is done when it conforms to its documents within that scope, at
  the documented or simulated evidence level, not when one machine passes.
- Recordings from real machines can be collected at any time and kept as test
  data; they qualify machine configs later and never change the standard
  code.
- Machines are added afterwards as configs on top, by us or the community.
  A machine's deviations from the standard live in its config or in a small
  piece in the right layer.
- The first lab's six machines are the first real hardware to reach the
  proven level, after the standards they use are complete.

## Consequences

- The phases in [plan.md §13](../plan.md) put the standards before any
  machine config.
- Real-machine results come later than in a machine-first plan.
- Standards work needs the official documents and simulators; tests cite
  their sections.
