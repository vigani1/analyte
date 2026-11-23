# bioMérieux VIDAS

## Machine

Original bioMérieux VIDAS with external PC, not mini VIDAS or VIDAS 3.
Installed PC/BCI version is required; the documented sample-ID ceilings differ
between versions (12 versus 17 characters). Neither is selected here.

No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

Vendor Literal format over serial, not ASTM framing. Published 8N1 defaults
are not proof of the installed settings. Required site settings: endpoint,
baud, bits, parity, stops, flow control, Literal/Alternate mode, suffix policy
and deadlines. Flow control is unresolved in the source notes.

## Settings on the machine

Confirm enabled BCI option, PC version, COM Manager settings and authorized
service setup. Keep app destination, credentials, retention and overrides
outside the definition. Do not infer XML/FTP compatibility from a network port.

## Supported workflows

Documents describe uploads and work-list download. No workflows are enabled.
Sample ci and patient pi have different meanings; staff assign positions on
the PC. Assay codes pass through as sent; the lab app maps them (ADR 0029).
Transport ACK does not prove assay acceptance or clinical approval.

## Departures

No tolerance added. Literal acknowledges before ETX; Alternate after ETX.
The installed mode cannot be guessed. Optional CR/LF checksum treatment,
retry policy and ID limit must be established before a TOML can be shipped.

## Evidence

Level: documented

Research only; no machine recording or official simulator run. No recordings shipped.

## Open questions

- Blocking: serial flow control, PC version/ID ceiling and Literal mode.
- Required site settings: complete serial settings, checksum/suffix/case policy,
  timers/retries, the assay code system and unit mappings, patient namespace
  and identity.
- Confirm QC/invalid/call semantics, dilution state, and acceptance evidence.
  Never truncate identifiers to a conservative guessed ceiling.
- Still missing: the BCI Link LIS Interface Driver Development Guide 161150-486
  revision for the installed VIDAS PC and BCI version (revisions A and B used so
  far); the VIDAS PC version, which decides a 12 or 17 character ID; whether the
  BCI option is enabled; the assay short codes; recordings of uploads and of a
  work list download.

## Result identity evidence

BCI Link 161150-486 rev C, pp. 4-5 to 4-6 supports a candidate key of
ci sample ID, rt assay, td completion date and tt completion time. Confirm
timestamp precision and stability across resends, and confirm the sample ID
format for controls before enabling this recipe.

## Sources

[Research](../../../docs/research/machines/vidas.md), §§2–6 and 8: BCI Link guides
161150-486 A/B §§3-1–3-7, 4-5, 5-1; VIDAS PC User Manual F §§4-14,
10-24–10-26, 12-18. [VIDAS protocol notes](../../../docs/protocols/vidas/README.md)
and [version limits](../../../docs/protocols/vidas/versions.md).

