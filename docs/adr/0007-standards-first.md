# 0007. Standards first, machines after

Status: accepted, 2026-11-15

## Context

Analyte is a general product, not a connector for one lab. If a protocol is
built only as far as one machine needs, every later machine finds gaps, and
one machine's quirks end up presented as the standard.

## Decision

- Each standard (ASTM LIS01/LIS02, HL7 v2 with MLLP, IHE LAW, POCT1-A) is built
  completely from its official documents and tested against official examples
  or simulators, independent of any machine.
- Each standard's work starts by writing its scope: versions, actor roles,
  message types, options, acknowledgment modes and limits, with explicit
  exclusions. It is done when it conforms within that scope at the documented
  or simulated level, not when one machine passes.
- Machines are added afterwards as configs, by us or the community. A machine's
  departures live in its config or in a small piece in the right layer.
- Real recordings can be collected at any time; they qualify machine configs
  and never change the standard code.
- The first lab's six machines are the first to reach proven, after their
  standards are complete.

## Why

A written scope gives each standard a finish line, and keeping machine quirks
out of the standard code keeps it correct for every machine.

## Consequences

- The phases in [plan.md §13](../plan.md) put the standards before any machine
  config, so real-machine results come later.
- Standards work needs the official documents and simulators; tests cite their
  sections.
