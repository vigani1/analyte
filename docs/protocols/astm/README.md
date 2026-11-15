# ASTM: LIS01-A2 and LIS02-A2

Evidence level: documented. Phase 2.

Current conformance checklist, written in our own words. Checked rows name
executable synthetic tests; unchecked rows state remaining work. These tests
support documented implementation, not hardware or simulator qualification.

## Supported scope

- **Edition:** LIS01-A2 (2008) and LIS02-A2 (2004, printed as LIS2-A2).
  Both are archived by CLSI but still the current editions.
- **Frames:** strict mode follows LIS01-A2: up to 64,000 characters per
  frame including 7 bytes of overhead (63,993 of text). Smaller vendor
  limits (for example 247) are machine config.
- **Units of data:** a link session (ENQ to EOT) carries one or more H…L
  messages; a message is made of records; a long record is split across
  frames. Strict mode never packs two records in one frame; tolerating that
  is a vendor compatibility option.
- **TCP:** strict LIS01 over TCP keeps the full handshake, checksums and
  timers, with the computer as server by default. Unframed LIS02 records are supported only through `watched_folder`;
  direct unframed LIS02 over TCP is rejected by config. Reversed client/server
  roles may be selected explicitly. TCP packet boundaries never frame ASTM.
- **Queries:** either side may query; Analyte supports the search criteria
  it lists here and declares the rest unsupported.
- **Content:** the record hierarchy, sequence numbers, all result status
  codes and the processing ID (production, training, debug, QC) are kept;
  training and debug messages never reach production delivery.


## Notes

- [Design decisions](decisions.md)
- [Conformance checklist and retained evidence](checklist.md)
- [Versions](versions.md)
