# Boule Swelab Lumi

## Machine

Boule Swelab Lumi is the proposed model; confirm the installed model and
software. Similarities to Dymind/Mindray do not establish protocol equivalence.

No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

HL7 over TCP is described by Lumi User Manual 1504529. Analyzer connects to
the host; 5600 is the documented default, not a shipped site endpoint.
Framing (including whether MLLP), encoding and exact HL7 edition remain
unverified. A loadable stack requires these choices and cannot guess them.

## Settings on the machine

Confirm Communication settings with authorized staff, ACK behavior and lookup
options. Required site settings include endpoint, software, app destination,
credential names, retention and local overrides. Obtain Boule's LIS supplement.

## Supported workflows

Results and sample lookup are described, but the exact sample-ID field and
lookup messages are missing. No workflows are enabled. Do not substitute
Dymind ORM/ORR layouts or assume PID patient identity is specimen identity.

## Departures

None added. In particular no guessed MLLP envelope, HL7 version, sample field,
QC prefix or OEM profile is shipped.

## Evidence

Level: documented

Research only; no machine recording or official simulator run. No recordings shipped.

## Open questions

- Blocking: framing, encoding, edition, sample field and lookup message layout.
- Required site settings: software, endpoint, assay/app and unit mappings,
  identity/run evidence, QC discriminator, flags, ACK and lookup deadlines.
- Compare typed, scanned and auto-assigned IDs in captures. Establish graphics
  format and bounds separately; no image is to be interpreted as a number.
- Still missing: Boule's LIS protocol description for the Lumi; the nameplate
  (to confirm it is a Lumi) and the software version.

## Sources

[Research](../../../docs/research/machines/swelab.md), §§2–6 and 8: Lumi User Manual
1504529 Communication and identification sections; Boule LIS protocol
supplement still needed. [HL7 scope](../../../docs/protocols/hl7/README.md).

