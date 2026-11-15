# ASTM versions

Analyte implements LIS01-A2 (2008) framing/dialog and LIS02-A2 (2004,
printed LIS2-A2) records. Evidence remains documented.

| Choice | Difference that matters | Source |
| --- | --- | --- |
| LIS01-A2 | Link frames, handshake and deadlines are separate from record syntax | Retained references in [checklist](checklist.md) |
| LIS02-A2 | H through L records and logical queries, independent of transport | Retained references in [checklist](checklist.md) |

No other edition is qualified. A vendor edition substitution is not implicit
compatibility; keep the raw input and refuse unsupported semantics. Current
config does not have a general ASTM edition selector. A LIS2 header version
is evidence to retain, not permission to guess another grammar.
