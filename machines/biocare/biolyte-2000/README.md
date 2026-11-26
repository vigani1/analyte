# BioCARE Biolyte 2000

## Machine

Electrolyte analyzer. Appendix VI documents Na+, K+, Cl- and Li+ on the
wire. Confirm which electrodes are installed on the lab's unit.

The [machine definition](machine.toml) is written from the Service Manual
V4.2A and ships at evidence level **documented** (ADR 0034 amendment,
2026-11-26): no real analyzer has confirmed it. Results are read and kept,
but every result is held for staff because the manual documents no result
identity (see [Result identity](#supported-workflows)).

## Connection

The rear DB25 female RS-232 port uses 9600 baud, 8 data bits, no parity and
one stop bit (Service Manual V4.2A, pp. 13–14). The analyzer has no screen
for these values; Setup suggests them. The lab chooses flow control with no
suggestion (ADR 0031): the pinout lists CTS, RTS, DTR and DSR, but the manual
does not say which handshake the analyzer uses.

The analyzer initiates with ENQ, the host replies with ACK or NAK, and the
analyzer ends with EOT. Each documented frame is STX, a frame number
(1–7 then 0 in the ASTM cycle), one CR-terminated record, ETX, two
hexadecimal checksum digits and CR/LF. The checksum sums the frame
number through ETX modulo 256. The host has 15 seconds for establishment
or frame acknowledgment. The receiver waits up to 30 seconds for the
first or next frame or EOT. The analyzer retries ENQ once after silence
and aborts after six failed sends of a frame (Appendix VI §§2–3 and p. 86).
These deadlines equal the LIS01-A2 defaults the definition uses. Analyte
stores each frame before acknowledging it.

## Settings on the machine

Read the software version from **Menu → 4 (Software version)**. The menu
also has Data → Transfer and settings for test selection, units and
resolution (Appendix I, pp. 73–74). These settings change the analytes,
unit strings or value text on the wire; Analyte passes each through as sent,
so none of them is a Setup question. Appendix VI gives one layout and
establishes neither a transfer mode choice nor firmware-dependent field
layouts. The only ADR 0032 choice is the software version.

### Setup questions

| Setup asks | Where staff find the answer |
| --- | --- |
| Baud, data bits, parity, stop bits | Fixed on the analyzer (Service Manual pp. 13–14, no settings screen). Keep the suggested 9600, 8, none, 1. |
| Software version (no suggestion): 4.2A, or Other version (not checked) | **Menu → 4 (Software version)** on the analyzer. Choose 4.2A only if the screen shows exactly that; otherwise choose Other, and Overview keeps a "not checked" warning. No version changes how results are read. |
| Flow control (no suggestion) | Not on any analyzer screen or in the manual. Ask whoever wired the cable (pins 4, 5, 6 and 20 are the handshake lines) or the vendor's service engineer. |

One other answer belongs to the lab but is not a Setup question:

- **Where staff type the specimen ID.** Only the keypad **ID** field reaches
  the wire (P.4). The accession number and name are printed but never
  transmitted (Appendix VI p. 82), so this is not a choice: staff must enter
  or scan the lab's specimen ID in **ID**. A result with a blank ID is held.

## Supported workflows

Result upload only. Appendix VI §2 makes the analyzer the sender. The host
returns link acknowledgments. No queries or orders are configured. Menu
transfer of stored data is the documented workflow.

| Wire field | Documented meaning and how the definition uses it |
| --- | --- |
| H.5.3 / H.5.4 | Software version and analyzer serial number. With 4.2A chosen, H.5.3 is checked against `4.2A` and the two tokens the manual's own example shows, `2.1.1.1` and `1.2.1.1` (ADR 0036); any other token warns. H.14 is transmission time. H.3 and H.12 are unused. |
| P.4 | The laboratory-assigned ID, blank when none was entered. It is the sample ID, kept exactly as sent, including padding and leading zeros. |
| O.3 / O.4 | O.3 is unused. O.4 is the analyzer's wrapping sample sequence; it is not used as identity. Appendix VI gives no accession field. P.6 is unused even though the keypad accepts a name. |
| O.16 / O.26 | Specimen descriptor (Serum/Plasma, Whole Blood or Urine) and preliminary report code P, kept as result details. One P and one O record per message, each with record sequence 1. |
| R.3 | Component 4 carries the BioCARE analyte code, passed through under `test_system = "biocare/biolyte-2000"` (ADR 0029). Component 5 is the measured (M) or calculated (C) marker. The lab app owns the names and test list; none ships. |
| R.4 / R.5 | ASCII value text, possibly padded, and the original unit (`mmol/L` or `mEq/L`). Value and unit spelling are kept as sent; there is no unit translation or conversion. |
| R.7 / R.9 / R.12 | Abnormal flags and observation status are unused. The analysis start time appears on the first R record only. |
| L.3 | N for normal termination, T for sender abort; T holds every result. |

**Result identity.** Appendix VI pp. 83–85 documents no stable run ID and
no correction marker: R.12 appears only in the first result, R.13 and R.9
are unused, and O.4 wraps. The definition therefore declares no identity
recipe (ADR 0035). Every result is held for staff as unresolved; a repeat
transfer or a rerun is never merged or delivered automatically.

**Controls.** The layout has no patient/control marker, so every result
carries an unknown material class. No rule is invented.

## Departures

Both are declared in the definition for model `biocare/biolyte-2000`,
firmware `4.2A`, mode `data_transfer`, analyzer uploads, at level
documented:

- `astm_absent_h12_means_production`: Appendix VI p. 82 (ASTM ref 7.1.12)
  leaves H.12 unused in the upload, so an empty H.12 is read as production.
  Any other H.12 keeps its standard meaning.
- `astm_repeat_enq_after_lost_ack`: Appendix VI §2.1 documents one ENQ retry
  after 15 seconds without a reply. A second ENQ before any frame is
  acknowledged and keeps the original receive deadline.

The p. 86 frame description contains one record ending in CR per ETX frame;
the research does not establish packed-frame support. Analyte refuses
multiple records per frame under LIS01-A2 §§3.3.2/6.3.1.1; no packed-frame
tolerance is declared.

### Gap report

| Manual source | Why it matters | Status |
| --- | --- | --- |
| Appendix VI §§2–4 | The documented editions are E1381-91 and E1394-91. Matching envelopes and timers establish only part of compatibility. | **Open.** Framing, dialog and syntax use LIS01-A2 and LIS02-A2; config cannot select the older editions. |
| Appendix VI §§2.1–2.2 | The analyzer repeats ENQ once. | **Closed:** the repeat-ENQ departure is declared. |
| Appendix VI §3 and p. 86 | The prose allows only 0–9 and A–F; the example prints lowercase. If the analyzer emits lowercase, Analyte NAKs the frame (raw bytes are kept). | **Open.** The conflicting evidence does not justify the lowercase-checksum departure; a capture decides. |
| Appendix VI p. 82, H.5 and H.12 | H.12 is unused; H.5.3 carries the software version. | **Closed:** the empty-H.12 departure, the software version choice and the ADR 0036 check are declared. |
| Appendix VI pp. 82–84, P.4 and O.3/O.4 | Missing ID, use of the accession field on the keypad or reliance on a wrapping counter can misassign a result. | **Partly closed:** P.4 is the sample ID, kept as sent, and a blank P.4 is held. Whether the lab's real specimen ID arrives intact (padding, case) needs a capture. |
| Appendix I p. 73 and Appendix VI pp. 82–85 | A QC menu exists, but the transfer layout has no patient/control discriminator and does not say whether controls are exported. | **Open.** Material class stays unknown. No site choice can supply a marker missing from the bytes. |
| Appendix VI pp. 84–85, O.16, O.26 and R.9 | Specimen descriptor and preliminary report status. | **Closed:** kept as result details. Observation status stays unknown. |
| Appendix VI p. 84 and p. 86, R.3/R.4 | Measured/calculated marker and padded values. | **Closed:** R.3.5 is read; padding and over-width text are kept. The exact over-width spelling remains unverified on hardware. |
| Service Manual p. 61, Appendix I p. 74 and Appendix VI pp. 84–85 | The analyzer can dilute urine, but the wire carries neither a factor nor whether the value already includes it. | **Open.** No dilution is inferred. |
| Appendix VI pp. 82–85, H.3, H.14, O.4, R.9, R.12 | No stable run ID or correction marker. | **Open.** No identity recipe; results are held for staff (ADR 0035). |
| Appendix VI section 11.1.3 | L.3 T marks a sender abort. | **Closed:** the measurements are kept and held. |

## Evidence

Level: documented

The [recordings](recordings/) are synthetic ADR 0009 conversations with
made-up identifiers, values and times. Patient includes padding, leading
zeros, the four documented analyte code spellings and frame-number wrap.
QC is a hypothetical control run on the same documented layout. It
establishes no QC marker and makes no claim that the analyzer exports QC.
Empty optional fields leaves P.4 blank and places the start time only on
the first result. Broken checksum tests a correct lowercase checksum, an
incorrect uppercase checksum and an accepted uppercase retry.

The machine replay suite replays them through the definition, with the
site answers in [replay.toml](replay.toml) (both software version values). The host
replies are unchanged from the record-only fixtures: the ready path stores
each frame and acknowledges it the same way, and refuses lowercase checksum
digits under strict hex. A shared de-identified recording from a real
analyzer is what would raise the level to proven.

## Open questions

- Which flow control and cable wiring does the installed analyzer use?
- What does Menu → 4 report, and does H.5.3 carry the same token? The
  manual's own example is inconsistent (its data list and its wire example
  show different versions), and `4.2A` is the manual's revision. Note the
  device serial and the installed electrodes too.
- Do staff put the lab specimen identifier in ID? Make a synthetic capture
  using distinct values in ID and Acc., including leading zeros and mixed
  case in ID. Compare P.4 and the printed slip byte for byte where possible.
  Include a blank ID run.
- Capture patient uploads, a QC run, an out-of-range/failed result, a
  repeat transfer and a new run for the same specimen. Check checksum
  case, padding, QC visibility, result codes, values and ID stability. If
  urine is relevant, record the configured dilution and compare the wire
  value with the analyzer report. Confirm which actions trigger uploads.

## Sources

- BioCARE Biolyte 2000 Service Manual V4.2A, RS232 Port (pp. 13–14), urine
  handling (p. 61), Appendix I (pp. 73–74), Appendix V (p. 79) and Appendix VI
  (pp. 80–86).
- [ASTM protocol notes](../../../docs/protocols/astm/README.md) and
  [machine research notes](../../../docs/research/machines/biolyte-2000.md).
