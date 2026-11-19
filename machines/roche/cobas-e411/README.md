# Roche cobas e 411

## Machine

Roche cobas e 411, disk or rack model. Installed software and the selected
Elecsys/cobas host dialect are required site confirmations.
+
No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

RS-232C HOST port, ASTM E1381-91/E1394-91 (HIM B-2/B-7, D-2–D-10).
9600 8N1 is a documented recommendation, not the installed setting.
Required site settings: endpoint, baud, bits, parity, stops and flow control.
The cable diagram loops analyzer pins 7–8; host flow control NONE is only an
inference in the research and is not shipped as a confirmed setting.

## Settings on the machine

Confirm Host Communication, dialect, upload options and installed transmission
code table with authorized staff (HIM B-5/B-6). Keep destination, credentials,
retention and all installation overrides outside this repository.

## Supported workflows

The manual describes patient/QC uploads, host queries and batch orders
(HIM C-2, E, F). None is enabled by this blocked definition. Host assay codes,
app mappings, units, specimen identity and order context require site evidence.
Do not equate method-sheet numbers with transmission codes or generated
@sequence identities with lab specimen IDs. Cancellation is not established.

## Departures

No tolerance added. HIM D permits several records in one cobas-dialect frame;
Elecsys sends at most one record per frame. Analyte refuses packed frames under
LIS01-A2 §§3.3.2/6.3.1.1 (PR #140). A cobas-dialect definition is blocked until
the installed behavior is established; do not silently select Elecsys instead.

## Evidence

Level: documented

Research only; no machine recording or simulator run. No recordings shipped.

## Open questions

- Blocking: selected dialect, packed-frame behavior and serial flow control.
- Required site settings: software version, host codes, app/unit mappings,
  identity/run evidence, QC rules, deadlines and order reply fields.
- Confirm barcode-error handling, query coordinates and rerun semantics from
  de-identified conversations before activation; never infer a specimen ID.

## Sources

- [Research](../../../docs/research/cobas-e411.md), §§2–5 and 8:
  Host Interface Manual v2.2 B, D, E, F; v2.3 corroborates settings only.
- [ASTM policy](../../../docs/protocols/astm/decisions.md) and
  [scope](../../../docs/protocols/astm/README.md).
