# YHLO iFlash 1200

## Machine

YHLO iFlash 1200. The 2018 family LIS manual V1.1 names iFlash3000 models;
its applicability to the 1200 remains unverified. Operation Manual V3.0
identifies a 1200-A, which does not establish the installed software.

No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

The requested profile is HL7 2.3.1 over MLLP, with analyzer PC connecting to
the host. This is family-document evidence, pending 1200 confirmation.
Endpoint/port, session lifetime and response deadlines are required site settings.
Do not use another model's port or connection timers as 1200 defaults.

## Settings on the machine

Obtain the installed protocol selection, software version, editable LIS assay
channels and authorized setup instructions. Keep endpoint, app destination,
credential names, retention and overrides in separate site configuration.

## Supported workflows

Family manual documents ORU uploads and QRY/QCK/DSR/ACK query exchange.
No workflow is enabled here. OBR-2 is the family sample barcode; PID-3 is not.
OBX-9 carries qualitative calls, including cases with empty OBX-5.
QC has parallel component arrays in OBR rather than ordinary OBX observations.

## Departures

No vendor special case added. Current config cannot select the vendor result
status semantics or map OBR-only QC arrays into observations (docs/context.md).
Generic HL7 configuration must not silently reinterpret these fields.
These are definition blockers, even though the legacy query dialog exists.

## Evidence

Level: documented

Research only; no machine recording or official simulator run. No recordings shipped.

## Open questions

- Blocking: confirm the family protocol for the 1200 and resolve status/QC
  mapping expressiveness with evidence before writing a configuration.
- Required site settings: port, software, encoding, deadlines, sample selector,
  assay/app and unit mappings, identity/rerun evidence and query layouts.
- Confirm BOTH datatype, OBX-9 qualitative calls, dilution application and
  calibration handling. Unknown lookup must remain an error, never no tests.

## Sources

[Research](../../../docs/research/iflash-1200.md), §§1–5: YHLO LIS Protocol
Manual V1.1 §§1.1, 2.2–2.4 (notably 2.3.4.2 and 2.4.1.2); 1200 Operation
Manual V3.0. [HL7 notes](../../../docs/protocols/hl7/README.md) and
[current gaps](../../../docs/context.md#before-machine-configs-phase-7).

