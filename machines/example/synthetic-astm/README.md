## Machine

Synthetic ASTM teaching fixture, not a real machine. Not qualified for clinical use.
Model: synthetic; firmware: unknown; driver: workspace version.

## Connection

TCP server, ASTM LIS01 framing and LIS02 records. The test supplies a loopback
endpoint; no installation addresses or credentials are shipped.

## Settings on the machine

There is no physical machine. Use the exact limits and deadlines in machine.toml.

## Supported workflows

One synthetic result upload; exact frame ACK bytes are compared during replay.
Sample 00012A and value 12.30 are original synthetic data. Missing run evidence
remains unresolved. No order download, cancellation or rerun qualification.

## Departures

None. `deviations = []` selects strict standard behavior. No installed departure
scope or vendor omission guarantee is claimed; synthetic replay cannot supply one.

## Evidence

Level: documented

recordings/upload.rec.jsonl is synthetic, copied unchanged from the repository's
ASTM receive fixture. Synthetic replay does not qualify for simulated or proven.
Config digest and platform qualification remain untested, as declared in TOML.

## Open questions

All real-machine and clinical qualification is outside this example's scope.

## Sources

The original synthetic crates/core/config/examples/astm.toml definition and
crates/testing/sim/recordings/astm.rec.jsonl recording. Protocol requirements are traced
in docs/protocols/astm; no vendor manual or patient capture was used.

The separate site file must select `retention = "keep"` or an explicit period
from 1 to 3650 days (90 recommended). Finite retention requires the installation
`retention-digest` credential; no credential belongs in this definition.
