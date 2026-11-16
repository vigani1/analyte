## Machine

BioCare Biolyte 2000 electrolyte analyzer. Installed firmware and electrode
configuration require site confirmation; calcium support is unverified.

No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

RS-232 DB25 female, 9600 baud, 8 data bits, no parity, one stop bit.
Service Manual 4.2A pp.13–14 and 80–86 describes E1381-91/E1394-91.
Flow control is explicitly UNVERIFIED. Serial settings require a concrete
flow_control value, so a loadable serial definition cannot yet be written.

## Settings on the machine

Confirm cable/handshake and firmware. The Data Transfer menu sends stored
results; automatic live upload is unverified. Required site settings include
endpoint, app destination, credential names, retention and local overrides.

## Supported workflows

Results only; the host must send link ACK/NAK but must not download orders.
P.4 carries ID, O.4 a wrapping sequence; O.3 is unused. Which entered ID is the
lab specimen identifier remains unverified. Do not use the sequence as identity.
No workflows are enabled until specimen and QC classification are established.

## Departures

No tolerance added. The p.86 frame description contains one record ending in
CR per ETX frame; the research does not establish packed-frame support.
Confirm actual output with a capture. Analyte refuses multiple records per
frame under LIS01-A2 §§3.3.2/6.3.1.1; do not enable packed-frame tolerance.

## Evidence

Level: documented

Research only; no machine recording or official simulator run. No recordings shipped.

## Open questions

- Blocking: serial flow control, specimen ID interpretation and reliable QC marker.
- Required site settings: firmware, test/app mappings, exact units, identity/run
  evidence, cable and confirmed transfer settings. Na+/K+/Cl- descriptions do
  not establish the site's app equivalents; no mapping is invented.
- Capture patient, QC, flagged and retransferred synthetic results, including
  frame boundaries and leading zeros; preserve original padded values.

## Sources

[Research](../../../docs/research/biolyte-2000.md), RS232 and sample identification
sections: Service Manual 4.2A Appendix VI, pp.13–14, 80–86; reference manual
V2 Table 3-2, pp.55–58, 63, 140. See [ASTM notes](../../../docs/protocols/astm/README.md).

