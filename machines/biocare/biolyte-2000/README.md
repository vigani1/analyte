# BioCARE Biolyte 2000

## Machine

Electrolyte analyzer. Appendix VI documents Na+, K+, Cl- and Li+ on the
wire. Confirm which electrodes are installed on the lab's unit.

This is a blocked machine definition, available for record-only connection.
A normal connection would require an interpretation of blank H.12 and
enough evidence to identify specimens, separate controls and recognize
retransfers safely.

## Connection

The rear DB25 female RS-232 port uses 9600 baud, 8 data bits, no parity and
one stop bit (Service Manual 4.2A, pp. 13–14). Setup suggests those four
values. The lab chooses flow control with no suggestion (ADR 0031). The
pinout alone does not establish which handshake the instrument uses.

The analyzer initiates with ENQ, the host replies with ACK or NAK, and the
analyzer ends with EOT. Each documented frame is STX, a frame number
(1–7 then 0 in the ASTM cycle), one CR-terminated record, ETX, two
hexadecimal checksum digits and CR/LF. The checksum sums the frame
number through ETX modulo 256. The host has 15 seconds for establishment
or frame acknowledgment. The receiver waits up to 30 seconds for the
first or next frame or EOT. The analyzer retries ENQ once after silence
and aborts after six failed sends of a frame (Appendix VI §§2–3 and p. 86).
Analyte keeps incoming bytes before replying in record-only mode.

## Settings on the machine

Read the software version from **Menu → 4 (Software version)**. The menu
also has Data → Transfer and settings for test selection, units and
resolution (Appendix I, pp. 73–74). These settings can change the analytes,
unit strings or value text on the wire, which a receiver can read directly.
Appendix VI gives one layout and establishes neither a transfer mode choice
nor firmware-dependent field layouts. There are no ADR 0032 choices to
expose. Port and serial settings are selected in Setup.

## Supported workflows

Record-only capture (ADR 0034). Appendix VI §2 makes the analyzer the
sender. The host returns link acknowledgments. No queries or orders are
configured. Menu transfer of stored data is the documented workflow.

| Wire field | Documented meaning and safe use |
| --- | --- |
| H.5.3 / H.5.4 | Software version and analyzer serial number. H.14 is transmission time. H.3 and H.12 are unused. |
| P.4 | The optional ID field in the transfer table. Confirm that the lab enters its specimen identifier in the keypad ID field and that it arrives here exactly. Without that evidence, retain P.4 as received and leave specimen identity unresolved. |
| O.3 / O.4 | O.3 is unused. O.4 is the analyzer's wrapping sample sequence, which cannot identify a specimen or a run. Appendix VI gives no accession field. P.6 is unused even though the keypad accepts a name. |
| O.16 / O.26 | Specimen descriptor (Serum/Plasma, Whole Blood or Urine) and preliminary report code P. One P and one O record per message, each with record sequence 1. |
| R.3 | Component 4 carries the BioCARE analyte code. The p. 86 example locates the measured marker in component 5. The prose repeats component 4 when introducing the measured/calculated markers. Use `biocare/biolyte-2000` as the future `test_system` (ADR 0029). The lab app owns the names and test list. |
| R.4 / R.5 | ASCII value text, potentially padded, and the original unit (`mmol/L` or `mEq/L`). Preserve value and unit spelling, including any over-width representation. No unit conversion is required. |
| R.7 / R.9 / R.12 | Abnormal flags and observation status are unused. The analysis start time appears on the first R record only. Subsequent R records carry their own sequence, incremented within the order. |
| L.3 | N for normal termination, T for sender abort. L.2 is 1. |

## Departures

Appendix VI section 7.1.12 identifies H.12 as unused in the upload. A normal
definition can declare the scoped empty-H.12 production interpretation.
Appendix VI section 2.1 documents one ENQ retry after 15 seconds without a reply.
The scoped retry departure acknowledges that second ENQ before any frame
arrives and keeps the original receive deadline. Both rules require a
definition and installed scope before activation.

The p.86 frame description contains one record ending in
CR per ETX frame; the research does not establish packed-frame support.
Confirm actual output with a capture. Analyte refuses multiple records per
frame under LIS01-A2 §§3.3.2/6.3.1.1; do not enable packed-frame tolerance.

### Gap report

| Manual source | Why it matters | Layer and current limit |
| --- | --- | --- |
| Appendix VI §§2–4 | The documented editions are E1381-91 and E1394-91. Matching envelopes and timers establish only part of compatibility. | Framing, dialog and syntax use LIS01-A2 and LIS02-A2. Config cannot select the older editions. |
| Appendix VI §§ 2.1–2.2 | **Closed:** a definition can declare the scoped repeated-ENQ departure (one retry before any frame, original receive deadline). | Dialog (ADR 0015 departure). |
| Appendix VI §3 and p. 86 | If the machine emits the lowercase checksum shown in the example, record-only will NAK the frame. Raw bytes are still journaled. | Framing. Normal ASTM supports a scoped lowercase-checksum departure with qualifying evidence. The record-only profile has strict hex and only accepts the packed-record departure. The manual's conflicting case evidence does not justify a departure. |
| Appendix VI p. 82, H.5 and H.12 | **Closed for H.12:** a definition can declare the scoped empty-H.12-means-production departure. H.5 software and serial are read by the software-version check (ADR 0036) where declared. | Mapping (ADR 0015 departure). |
| Appendix VI pp. 82–84, P.4 and O.3/O.4 | Missing ID, use of the accession field on the keypad or reliance on a wrapping counter can misassign a result. The documented padding on the example ID also needs a capture to establish what the keypad entered and what arrived. | Mapping can select P.4 and keep its text, including leading zeros and padding. Neither config nor the document establishes that it represents the lab's specimen, and O.4 gives no safe fallback. Accession and name have no documented wire positions. |
| Appendix I p. 73 and Appendix VI pp. 82–85 | A QC menu exists, but the transfer layout defines no reliable patient/control discriminator and does not say whether controls are exported. | Machine evidence and mapping. Equality rules need a transmitted marker. No site choice can supply a marker missing from the bytes. |
| Appendix VI pp. 84–85, O.16, O.26 and R.9 | The specimen descriptor and the preliminary report status have their own meanings. An absent observation status cannot establish release or correction. | Mapping retains O.26 in source hierarchy status. It does not map it to a separate normalized report status or project O.16 into a normalized ASTM specimen descriptor. R.9 remains unknown. |
| Appendix VI p. 84 and p. 86, R.3/R.4 | The measured or calculated marker is distinct from the test code. The over-width notation is described without an unambiguous example of the emitted token. | Mapping can read R.3.4 and preserve raw measurement text. It has no dedicated measured/calculated attribute. Keep any unrecognized value as source text until its meaning is established. |
| Service Manual p. 61, Appendix I p. 74 and Appendix VI pp. 84–85 | The analyzer can dilute urine, but Appendix VI supplies neither a factor nor whether the transmitted value already incorporates it. | Mapping can represent explicit dilution evidence. This wire layout provides none. No amount or applied status can be inferred. |
| Appendix VI pp. 82–85, H.3, H.14, O.4, R.9, R.12 | No stable run ID or correction marker is documented. Transmission time changes on transfer, sequence wraps and one analysis time cannot prove revision or run identity. | Mapping and identity. Existing selectors cannot make this evidence sufficient. Keep identity unresolved. H.14 and the first R.12 represent different times. Later R records supply no analysis time of their own. |
| Appendix VI p. 85, L.3 | A sender-aborted message can have a complete record envelope. Its measurements should remain distinguishable from a normally completed transfer. | Syntax can read T. The inbound ASTM dialog and mapping do not use L.3 to classify a complete-looking message as sender-aborted. |


## Evidence

Level: documented

The [recordings](recordings/) are synthetic ADR 0009 conversations with
made-up identifiers, values and times. Patient includes padding, leading
zeros, the four documented analyte code spellings and frame-number wrap.
QC is a hypothetical control run on the same documented layout. It
establishes no QC marker and makes no claim that the analyzer exports QC.
Empty optional fields leaves P.4 blank and places the start time only on
the first result. Broken checksum tests a correct lowercase checksum, an
incorrect uppercase checksum and an accepted uppercase retry. Those host
replies describe Analyte's strict record-only profile.

The shipped machine replay test discovers normal definitions only. These
fixtures are for record-only link replay and do not qualify this model to
deliver results.

## Open questions

- Which flow control and cable wiring does the installed analyzer use?
- What does Menu → 4 report for software version? Confirm it and the device
  serial against H.5 in a capture and note the installed electrodes.
- Do staff put the lab specimen identifier in ID or Acc.? Make a synthetic
  capture using distinct values in both fields, including leading zeros
  and mixed case in ID. Compare P.4 and the printed slip byte for byte
  where possible. Include a blank ID run.
- Capture patient uploads, a QC run, an out-of-range/failed result, a
  repeat transfer and a new run for the same specimen. Check checksum
  case, padding, QC visibility, result codes, values and ID stability. If
  urine is relevant, record the configured dilution and compare the wire
  value with the analyzer report. Confirm which actions trigger uploads.

## Result identity evidence

Service Manual 4.2A Appendix VI, pp. 83-85 lists R.12 analysis start time
only in the first result and leaves R.13 completion time and R.9 status
unused. The candidate sample ID and analysis start time need confirmation
and a reliable way to associate that start time with the remaining assays.

## Sources

- BioCARE Biolyte 2000 Service Manual 4.2A, RS232 Port (pp. 13–14), urine
  handling (p. 61), Appendix I (pp. 73–74), Appendix V (p. 79) and Appendix VI
  (pp. 80–86).
- [ASTM protocol notes](../../../docs/protocols/astm/README.md) and
  [machine research notes](../../../docs/research/machines/biolyte-2000.md).
