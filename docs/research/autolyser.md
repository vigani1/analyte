# DIALAB Autolyser: host-interface research

1. The label and IFU identify the external-PC Autolyser family; REF 20010D likely originally lacked barcode readers, but installed options remain unverified.
2. The proprietary interface is host-driven: STX/ACK, worklist or R/L/A request, EOT; results require manual or automatic validation.
3. Serial is documented, and a manual screenshot shows a TCP/IP transport selector; TCP endpoint details and the complete serial settings list remain UNVERIFIED.
4. Barcode-free operation uses a patient code assigned to a physical cup position; returned results carry the code, not the cup position.
5. OpenELIS Analyzer Bridge needs an active polling driver and proprietary parser, not merely a profile; no implementation or device testing was performed.

Research date: 2026-11-14. Scope: research only, one instrument. This is not an implementation plan or authorization to connect a production analyzer.

## Evidence and reference conventions

**M** means the _AUTOLYSER/AUTOLYSER ISE Operator's Manual_, March 2025, Revision 18, software 21.18 / 21.18M (pp. 1–2). Printed page numbers equal PDF page numbers. Pages 119, 176, 178, 179, 183 and 184 were checked visually, because their tables and figures carry details the running text does not.

**VERIFIED** means checked directly against the cited page. **INFERRED** means reasoning from documented facts. **PROPOSED** means a future integration choice. **UNVERIFIED** means the sources do not establish the detail or the installed unit has not been checked. All example messages in this note are synthetic, written for it with made-up values; none is a vendor example or real patient data. Statements about the manual's own examples describe them without reproducing them.

## 1. Identity and test-menu style

Rating label as reported by the requester: Model `AUTOLYSER`, REF `20010D`, manufactured `2020/01`, `100–240 V`, `530 W`, made in EU; separate Windows PC. The label has not been independently inspected. M p. 159 agrees on 530 W and an external Windows PC. The IFU is newer than this machine; the installed software version must be checked before treating Rev18 as its exact interface specification.

| Name           | What the evidence establishes                                                                                                     | Applicability                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Autolyser      | External-PC clinical chemistry platform, up to 250 chemistry tests/hour; optional sample/reagent barcode readers and optional ISE | Best match to the reported label and IFU                                           |
| Autolyser 100  | Separate product with built-in touchscreen computer and up to 100 tests/hour                                                      | Do not substitute its protocol manual                                              |
| Autolyser 450  | Separate higher-throughput platform with different hardware                                                                       | Do not infer compatibility from the name                                           |
| Autolyser ISE  | Autolyser variant/configuration with ion-selective-electrode functionality; covered by the combined IFU                           | The label alone does not prove an ISE module is installed                          |
| Autolyser PLUS | No authoritative PLUS model identity or interface equivalence was established in the sources searched                             | **UNVERIFIED**; `BTPLUS.EXE` is a test program name, not evidence for a PLUS model |

Sources: official Autolyser product page; Autolyser 100, 450 and ISE product pages.

The October 2025 product flyer, p. 3, lists 20010D as the variant without a barcode reader and without a PC. Its open-system model without readers is U2011D; barcode-equipped models have other references. **INFERRED:** this 20010D was probably originally supplied without sample or reagent readers. That is a later catalogue's association, not proof of its 2020 configuration or later upgrades. Do not infer either installed barcode capability or open software from this label. The flyer gives 300 W, unlike the label/IFU's 530 W: use the actual label for electrical identification, not the current flyer.

M p. 38 distinguishes closed and open software. On closed software the user cannot add new analyses. The manual marks the open version with an "(M)" suffix, which matches the 21.18M software name.

Open parameters are editable by a Lev2 user; closed parameters cannot be changed and can only be exported (standards/controls remain editable). Therefore "open system" is configuration-dependent, not a blanket fact about every Autolyser. M p. 38 describes All Tests, Current Tray, New Code, Rename and analytical parameters. These are configured analysis/reagent codes, not a universal immutable LIS test menu. Only codes in the Current Tray can be ordered/run. Barcode reagent identifiers and manufacturer reagent REF numbers are not automatically the four-character host analysis code.

## 2. Physical connection, serial settings, Ethernet and cable

### Where the connection belongs

M p. 159 lists RS232 and Ethernet for host communication, and USB 2.0 for the link between the PC and the analyzer.

**INFERRED topology:** analyzer mechanics ↔ USB ↔ analyzer-control Windows PC ↔ RS232 or Ethernet ↔ LIS/on-site connector. The LIS connection is exposed by the analyzer application on the control PC. Do not confuse its host COM connection with the hardware-control USB link, or assume the analyzer chassis itself exposes an Ethernet LIS socket. Verify actual ports, cabling and COM ownership on site.

### Serial configuration: confirmed versus missing

M p. 119: Utility → Setup Analyzer → Serial; enable Present, then configure COM Number, Baud-Rate, Hand-Shake and internal/variable protocol. COM Number picks the serial port; the manual gives COM 1 and COM 2 as examples.

| Parameter                | Confirmed source                          | What remains UNVERIFIED                                                                                  |
| ------------------------ | ----------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| COM number               | Selectable; COM1/COM2 examples, p. 119    | Installed host port and availability; do not impose the old test utilities' COM1–4 limit on the main app |
| Baud list                | Selectable dropdown, p. 119               | Complete list is not printed and the dropdown is not expanded in the figure; no invented list            |
| Baud 9600                | p. 188, test utilities                    | Whether it is the installed application's selected/default baud                                          |
| Handshake options        | Selectable dropdown, p. 119               | Complete list and mapping of "Hardware" to specific modem lines                                          |
| Hardware handshake       | p. 188 test utilities                     | Whether the installed setup uses it                                                                      |
| Parity None; stop bits 1 | p. 188 test utilities                     | Other supported values and whether configurable in this app version                                      |
| Data bits                | Not stated in the inspected documentation | **UNVERIFIED**, including the commonly assumed 8 data bits; do not call 8N1 vendor-confirmed             |

The p. 188 settings are specifically COMUNICA.EXE/BTPLUS.EXE settings, not a complete port-option specification. Those DOS-era utilities are not evidence of compatibility with a modern Windows installation. Do not execute them on the lab PC during research.

### Ethernet/TCP: a real selector, incomplete wire specification

**VERIFIED:** M p. 119, Figure 15-7, contains a **Transport** box with **Serial** and **TCP/IP** radio buttons. These labels appear only in the screenshot, not in the running text. Thus the evidence supports a TCP/IP mode in the documented analyzer software; it is not merely a speculative serial-to-Ethernet adapter.

The older Service Manual for software 14 also lists RS232 or LAN (TCP/IP) as the host connection, in its external-PC requirements.

**UNVERIFIED:** server/client role, bind address, remote address, port, connection lifetime, whether a license is required, and whether the TCP stream carries exactly the chapter-23 byte protocol. No TCP setup dialogue or TCP packet sequence was found in the IFU. "Host initiates" refers to application dialogue; it does not establish which side calls TCP connect. Obtain the installed TCP settings and a Dialab TCP host-interface supplement. No basis exists to claim HL7/MLLP or ASTM over TCP. Ethernet might avoid COM access constraints, but it does not remove the need to establish the protocol and active polling behavior.

### Cable and pinout

M p. 179, §23.4, shows two wiring diagrams. The table below is our reading of them, left "Analyser" to right "IBM"; no crossing is treated as an electrical junction:

| Left 9-pin | Right 9-pin, lower diagram | Right 25-pin, upper diagram |
| ---------- | -------------------------- | --------------------------- |
| 2          | 3                          | 2                           |
| 3          | 2                          | 3                           |
| 4          | 6                          | 6                           |
| 5          | 5                          | 7                           |
| 6          | 4                          | 20                          |
| 7          | 8                          | 5                           |
| 8          | 7                          | 4                           |

Pins 1 and 9 are not wired in these drawings. The upper diagram marks both ends `F`; the lower drawing does not clearly label connector sex. The 9↔9 drawing crosses transmit/receive and handshake pairs: a fully wired null-modem arrangement, not a straight-through cable. **INFERRED:** hardware handshake explains why a three-wire-only cable may be unsuitable. Connector sex, installed port pin functions and grounding/shield requirements must be checked against the actual PC and Dialab-approved cable. These drawings describe host communication, not a replacement for the PC↔mechanics USB cable.

## 3. Internal protocol, byte layouts and errors

### Session framing and commands

M p. 175: the host always starts the exchange. It sends STX (0x02) and waits for ACK (0x06) from the analyzer. It ends the exchange by sending EOT (0x04).

| Byte | Hex  | Meaning/direction                                                |
| ---- | ---- | ---------------------------------------------------------------- |
| STX  | `02` | Host starts an operation                                         |
| ACK  | `06` | Analyzer accepts initialization                                  |
| EOT  | `04` | Host terminates request/data; analyzer report also ends with EOT |
| `R`  | `52` | Host requests next available report                              |
| `L`  | `4C` | Host requests last report sent, for recovery                     |
| `A`  | `41` | Host requests first available report, to restart retrieval       |
| NAK  | `15` | Analyzer has no report for the requested R/L/A                   |
| `Y`  | `59` | Successful patient insertion; followed by one position byte      |
| `N`  | `4E` | Failed patient insertion; followed by one error byte             |

R/L/A definitions: M p. 177. They are ASCII letters, not binary numeric command IDs. Worklist insertion has **no extra command letter**: patient code begins the payload immediately after ACK. No ENQ, ASTM frame numbers, ETX, CR/LF, or result-receipt ACK is specified by this internal protocol. Do not borrow these from ASTM.

The general wording plus §23.2 says R/L/A use STX/ACK and EOT. The request sequence `STX → ACK → R EOT → report EOT` is the direct reading used below; **the precise R-versus-EOT response timing needs a capture**, since no time-resolved diagram is provided. No documented final EOT after `Y + position`, `N + error`, or NAK is specified. Do not silently append bytes to those responses.

### Patient list download: host → analyzer

Fixed-width ASCII payload, one patient per operation; offsets zero-based, framing excluded. M pp. 175–176; the field alignment was checked against the manual's example and script, pp. 186–187.

| Offset  | Length | Field             | Encoding                                                              |
| ------- | ------ | ----------------- | --------------------------------------------------------------------- |
| 0       | 15     | Patient code      | Space-pad shorter values to protocol width; preserve identity exactly |
| 15      | 1      | List type         | `T` Routine, `R` STAT                                                 |
| 16      | 1      | Sample type       | `S` Serum, `U` Urine                                                  |
| 17      | 1      | Clone             | `Y` yes, `N` no                                                       |
| 18      | 2      | Cup position      | ASCII `00` unknown/autoassign, `01`–`99` specified position           |
| 20      | 2      | Number of tests n | Two ASCII digits, e.g. `02`                                           |
| 22      | 4 × n  | Analysis codes    | Four characters each, e.g. `ALB `                                     |
| 22 + 4n | 3      | Checksum          | Zero-padded decimal ASCII, `000`–`255`                                |
| 25 + 4n | 1      | End               | EOT `04`                                                              |

Field widths imply payload-plus-EOT length `26 + 4n`. Range `01`–`99` is the protocol's position encoding, not proof that 99 sample slots physically exist; current product literature describes 62 sample positions and Setup divides the tray among patient, STAT, standards and controls.

M p. 175: on success the analyzer answers `Y` and then one byte giving the position where it placed the patient. On failure it answers `N` and then one byte giving the error type.

Interpret the position as the documented single byte, **not** two ASCII digits. **UNVERIFIED:** special position values for extra-list insertion and whether all successful cases use ordinary numeric byte values. Preserve the reply byte for site verification.

### Result upload: analyzer → polling host, controls disabled

M p. 177. The page places its per-report label so that it looks like one checksum per test. The manual's example shows otherwise: one final checksum follows all the repeated analysis/result pairs.

| Offset   | Length | Field                                   |
| -------- | ------ | --------------------------------------- |
| 0        | 15     | Patient code                            |
| 15       | 1      | List type: `T` Routine / `R` STAT       |
| 16       | 1      | Sample type: `S` / `U`                  |
| 17       | 3      | Number of reports/results n, e.g. `002` |
| 20       | 11 × n | Repeated analysis code (4), result (7)  |
| 20 + 11n | 3      | Whole-report decimal checksum           |
| 23 + 11n | 1      | EOT                                     |

Whole response length `24 + 11n`. Result is a seven-character field, not necessarily a valid number: the Setup option can send `<NC>` for a non-calculable result. Its exact padding and all other non-numeric forms are **UNVERIFIED**. Standard results have no patient name, cup position, units, flags, ranges, timestamp or run identifier.

### Result upload with "Send also the controls" enabled

M p. 178, VERIFIED visually. This switch changes the header for **all** results, including ordinary samples:

| Offset   | Length | Field                                  |
| -------- | ------ | -------------------------------------- |
| 0        | 15     | Patient code                           |
| 15       | 1      | List type `T` / `R`                    |
| 16       | 1      | Sample type `S` / `U`                  |
| 17       | 1      | Control sample `Y` / normal sample `N` |
| 18       | 1      | Known `K` / unknown `U` control        |
| 19       | 3      | Control level                          |
| 22       | 20     | Control lot number                     |
| 42       | 3      | Number of results n                    |
| 45       | 11 × n | Code (4) + result (7)                  |
| 45 + 11n | 3      | Whole-report checksum                  |
| 48 + 11n | 1      | EOT                                    |

Whole response length `49 + 11n`. For an ordinary sample, the manual specifies `N`, `K`, `001`, and twenty spaces as the four control fields. The discriminator is the control-sample flag, **not** the known-control flag or level. Never route QC values into patient results based on a coincident patient code.

**Vendor-example defect:** the p. 178 example prints a checksum of 063. It also shows six unexplained spaces between the sample-type field and the control fields. Rebuilding that example from the documented layout, with the same patient, codes and results as the p. 177 example and the ordinary-sample control fields, gives a checksum of **167**, not 063. Counting the six extra spaces as well gives 103, still not 063. The field diagram has no such six-byte field. This is unresolved; do not implement the spurious spaces or accept 063 as a golden test. Request corrected vendor examples and an actual control/ordinary-sample capture in this mode.

### Exact checksum rule and span

M p. 179: add up the ASCII values of all sent characters, then take the remainder after dividing by 256; that remainder is the checksum. The terminator characters of variable-length strings are left out of the sum.

Operational rule reconstructed and verified against both standard examples:

`checksum = sum(unsigned ASCII bytes of payload before checksum) mod 256`

Represent the result as **three decimal ASCII digits**, left-zero-padded. Include every payload space and all header/test/result bytes. Exclude STX, ACK, the checksum's own three digits and EOT. A report request `R` is a separate host command, not part of the returned report checksum. This span is **INFERRED and example-verified**, because the prose does not enumerate the exclusions.

- The manual's worklist example (p. 176): its bytes sum to 1934, so the checksum is 142, which matches the printed value.
- The manual's controls-disabled result example (p. 177): its bytes sum to 2813, so the checksum is 253, which matches the printed value.
- The same data in the controls-enabled layout (p. 178) sums to 3751, so the checksum should be 167; the printed 063 is inconsistent.

Variable protocol: exclude terminator bytes emitted by `String`; do not assume the checksum covers every byte of the physical stream. Constants are data unless excluded by a documented rule. The manual says checksum omission disables checking (p. 180); omission is not proposed here. Exact constant/terminator combinations in a custom script require vendor/compiler verification.

### Insertion error byte table, VERIFIED against the page

M p. 176. These are the **second byte after ASCII `N`**. For example `4E 15` is insertion failure 0x15; it is different from standalone NAK `15` during report polling. Meanings below are in our own words.

| Error byte | Meaning                                                                                       |
| ---------- | --------------------------------------------------------------------------------------------- |
| `0x01`     | Checksum does not match                                                                       |
| `0x02`     | Command not recognized                                                                        |
| `0x03`     | Invalid list-type (routine/STAT) field                                                        |
| `0x04`     | Invalid sample-type (serum/urine) field                                                       |
| `0x05`     | Invalid clone field                                                                           |
| `0x06`     | Invalid cup position                                                                          |
| `0x07`     | Invalid test-count field                                                                      |
| `0x08`     | Test count does not match the tests sent                                                      |
| `0x09`     | The position is already being processed                                                       |
| `0x0A`     | The sample cannot be cloned                                                                   |
| `0x0B`     | Duplicate patient code                                                                        |
| `0x0C`     | At least one test is not defined on the analyzer                                              |
| `0x0D`     | At least one test is not on the current tray                                                  |
| `0x0E`     | More tests than allowed for one patient                                                       |
| `0x0F`     | Invalid constant data                                                                         |
| `0x10`     | Invalid variable data                                                                         |
| `0x11`     | Invalid variable data inside the analysis set                                                 |
| `0x12`     | Repeat requested, but there is no patient to repeat                                           |
| `0x13`     | Repeat requested, but the sample type differs from the stored patient                         |
| `0x14`     | Repeat requested, but the list is full                                                        |
| `0x15`     | Repeat requested, but the list differs                                                        |
| `0x16`     | The requested position is already taken                                                       |
| `0x17`     | Patient already exists or was run, is not a clone, and sits on the extra (supplementary) list |
| `0x18`     | Patient already run, and neither repeats nor clones are enabled                               |

M p. 180 further explains 0x0F as invalid constant data for Char/String/Stringn, 0x10 as invalid variable data, and 0x11 as an analysis variable outside its analysis SET. Do not substitute the separate script **compiler** errors on p. 182 for these wire errors.

### NAK, validation, timing and retry gaps

NAK `15` is documented for R/L/A when no report is available (p. 177), including the practical case where no run's results have yet been validated. It is not documented as the insertion-error mechanism or a busy indication. ACK failure responses, busy responses and malformed-session behavior are **UNVERIFIED**.

M p. 177: results are not ready to send right after a run, because they must be validated first. The operator does this with the Utility → RS232 option **Accept result to be sent**, after every run or group of runs.

Alternatively, Utility → Setup Analyzer → Serial has an option to send all results automatically without validation. §23.2 and p. 119 spell the last word of that option label slightly differently. **INFERRED reconciliation:** this makes reports automatically eligible for host retrieval; it does not overturn §23.1's explicit host-driven initiation. Do not interpret "automatically" as unsolicited push without vendor confirmation.

M p. 69 also documents **Delete Result to be sent**. Deleting that pending queue is not a recovery procedure. The transmission option for `<NC>` controls whether non-calculable results are sent or removed from the outbound queue (p. 119).

**UNVERIFIED:** ACK deadline, inter-byte timeout, response timeout, maximum frame size/test count accepted, retry count, polling interval, reset after dropped connections, when the report cursor advances, persistence across restart and how L/A behave at an empty queue. `L` is documented for recovery, `A` for first/all retrieval, but no documented host ACK commits a report. Do not promise exactly-once acquisition. Polling, timeout and replay settings must be established with Dialab/site captures; any chosen intervals are connector policy, not manufacturer requirements.

## 4. Variable Serial Protocol

M pp. 180–187 allows custom transmit and receive scripts defining fields, constants, order and widths. Framing, report request commands and success/failure responses remain the same (p. 180). It is not ASTM simply because it can emit separator characters.

### Script mechanics and cautions

- `Char` emits/receives a byte. `String <value>|<terminator>` is terminated; `Stringn <value>|<length>` is fixed-width. Decimal literals use `$`, hex uses `0x`, printable strings use single apostrophes. `;` introduces a comment (pp. 180–181).
- Stringn right-pads text with spaces and truncates overlong text; numerical fields are left-zero-padded and also truncated if overlong (p. 181). Widths must therefore be selected with actual limits, not guesses.
- One complete command per line. DOS/Windows line endings are required; the manual warns against UNIX text files (p. 180).
- Use one analysis-repeat SET; `PATIENTNUMBERTEST` must occur before it (p. 182). The vendor example uses `Set #BeginAnalysesData` / `Set #EndAnalysesData` as the SET keywords (pp. 186–187). The explanatory prose uses inconsistent group spellings; use the keywords from the example and validate in the vendor compiler.
- Save **and recompile both transmitting and receiving scripts**, even when only one changes (p. 180).
- Internal protocol list characters are `T` Routine / `R` STAT; variable protocol is `R` Routine / `S` STAT (p. 180). Transmissions from archive always identify Routine (p. 183).

M p. 175 warns that only qualified personnel may change the Variable Serial Protocol, and that the customer is responsible for any malfunction caused by changed scripts.

### Full reporting field list: analyzer → host

Variable names from p. 183 Table 1, VERIFIED visually. All accept String; the "Also Char" column marks variables that also accept Character, as printed. Meanings are in our own words.

| Variable                 | Meaning                                                           | Also Char |
| ------------------------ | ----------------------------------------------------------------- | --------- |
| `PATIENTCODE`            | Patient code; may serve as sample accession if set up that way    | No        |
| `PATIENTNAME`            | Given name                                                        | No        |
| `PATIENTSURNAME`         | Family name                                                       | No        |
| `PATIENTGROUP`           | Group: M male, F female, C child                                  | Yes       |
| `PATIENTLISTTYPE`        | List: R routine, S STAT                                           | Yes       |
| `PATIENTTYPE`            | Sample: S serum, U urine                                          | Yes       |
| `PATIENTNOTE`            | Free-text note                                                    | No        |
| `PATIENTNUMBERTEST`      | How many results follow                                           | Yes       |
| `CHECKSUM`               | Checksum                                                          | Yes       |
| `ANALYSESCODE`           | Test code                                                         | No        |
| `ANALYSENAME`            | Test name; the variable is spelled this way in the manual         | No        |
| `ANALYSESTYPE`           | Test kind: C clinical chemistry, R relation (calculated)          | Yes       |
| `ANALYSESCONCENTRATION1` | First concentration                                               | No        |
| `ANALYSESCONCENTRATION2` | Second (final) concentration                                      | No        |
| `ANALYSESFLAGS1`         | Flag on the first result                                          | No        |
| `ANALYSESFLAGS2`         | Flag on the second result                                         | No        |
| `ANALYSESMINVALUE`       | Lower range value                                                 | No        |
| `ANALYSESMAXVALUE`       | Upper range value                                                 | No        |
| `ANALYSESUM1`            | Unit of the first result                                          | No        |
| `ANALYSESUM2`            | Unit of the second result                                         | No        |
| `ANALYSESUMFACTOR`       | Factor between the two units                                      | No        |
| `ANALYSES2RESULT`        | Whether a second result exists: Y or N                            | Yes       |
| `ANALYSESSERUMTYPE`      | Sample type the method is set for: S or U                         | Yes       |
| `ANALYSESURINE24H`       | 24-hour urine value                                               | No        |
| `ISCONTROL`              | Y for QC, N for an ordinary sample                                | Yes       |
| `CONTROLTYPE`            | K known control, U unknown control                                | Yes       |
| `CONTROLLEVEL`           | Control level                                                     | Yes       |
| `CONTROLLOTNAME`         | Control lot number                                                | No        |

M p. 183, after the table: to send only the final result, use the second-result variables. When there is no second result, those variables carry the same values as the first result.

Use concentration2 with UM2/flags2, rather than mixing primary and converted units. Units, flags and ranges can be included in the analysis SET; names can be added before the SET. There is no separately listed reporting `SAMPLEID` or `PATIENTCUPPOSITION`: sample identity is `PATIENTCODE`. Unit/flag maxima, flag vocabulary, encoding of names and the exact relationship between emitted min/max ranges and converted units are **UNVERIFIED**.

### Full reception field list: host → analyzer

Variable names from p. 184 Table 2, VERIFIED visually; accepted values continue on pp. 184–185. Meanings are in our own words.

| Variable              | Meaning                                                 | Allowed data type   |
| --------------------- | ------------------------------------------------------- | ------------------- |
| `PATIENTCODE`         | Patient code                                            | String              |
| `PATIENTNAME`         | Given name                                              | String              |
| `PATIENTSURNAME`      | Family name                                             | String              |
| `PATIENTLISTTYPE`     | Routine or STAT list                                    | String or Character |
| `PATIENTGROUP`        | Male, female or child                                   | String or Character |
| `PATIENTTYPE`         | Serum or urine                                          | String or Character |
| `PATIENTURINE24H`     | 24-hour urine value                                     | String              |
| `PATIENTNOTE`         | Free-text note                                          | String              |
| `PATIENTISCONTROL`    | Ordinary sample or control                              | String or Character |
| `PATIENTCONTROLKNOK`  | Known or unknown control; the manual spells it this way | String or Character |
| `PATIENTCONTROLLEVEL` | Control level                                           | String or Character |
| `PATIENTCLONE`        | Whether to clone                                        | String or Character |
| `PATIENTCUPPOSITION`  | Cup position; zero lets the analyzer choose             | String or Character |
| `PATIENTNUMBERTEST`   | How many tests follow                                   | String or Character |
| `CHECKSUM`            | Checksum                                                | String or Character |
| `ANALYSESCODE`        | Test code, inside the repeated SET                      | String              |

Accepted reception values (M pp. 184–185; `$n` is a numeric byte, quoted digits are ASCII strings):

- List: `$0`, `0`, `R`, `ROUTINE` → Routine; `$1`, `1`, `S`, `STAT` → STAT.
- Group: `$0`, `0`, `M`, `MAN` → male; `$1`, `1`, `F`, `FEMALE` → female; `$2`, `2`, `C`, `CHILD` → child.
- Sample type: `$0`, `0`, `S`, `SERUM`; or `$1`, `1`, `U`, `URINE`.
- Control: `$0`, `0`, `N`, `S`, `NO`, `SAMPLE` → sample; `$1`, `1`, `Y`, `C`, `YES`, `CONTROL` → control.
- Known control: `$0`, `0`, `N`, `U`, `NO`, `UNKNOW` → unknown; `$1`, `1`, `Y`, `K`, `YES`, `KNOW` → known. Unusual spellings are the manual's.
- Level: `$1`, `1`, `L`, `LOW`; `$2`, `2`, `N`, `NORMAL`; `$3`, `3`, `A`, `ABNORMAL`.
- Clone: `$0`, `0`, `N`, `NOCLONE` → normal; `$1`, `1`, `Y`, `CLONE` → clone.

### Proposed reporting script: code, test, final result, unit, flags

**PROPOSED, UNCOMPILED, NOT device-tested.** Uses the vendor example's SET keywords and tab-terminated variable strings to avoid guessing numeric/unit/flag widths. All terminators are literal `$9`; do not include them in the checksum. It also transmits list/sample type and QC identity so interpretation is explicit. The three-digit count determines the repetitions. EOT remains protocol framing, not an additional script command.

```text
Stringn #PatientCode|$15
Char #PatientListType
Char #PatientType
Char #IsControl
Stringn #PatientNumberTest|$3
Set #BeginAnalysesData
Stringn #AnalysesCode|$4
String #AnalysesConcentration2|$9
String #AnalysesUM2|$9
String #AnalysesFlags2|$9
Set #EndAnalysesData
Stringn #CheckSum|$3
```

To add demographics, insert `String #PatientName|$9` and `String #PatientSurname|$9` before the count/SET. To add range values, insert `String #AnalysesMinValue|$9` and `String #AnalysesMaxValue|$9` within the SET. For QC details use the reporting control fields above. No patient name is required for accession matching; omit it unless needed. A paired receiving script can follow the structure of the manual's p. 186 receiving example, with the variable protocol's **R/S** list convention. Both scripts must be saved and compiled together.

**Risks requiring qualification:** compiler acceptance of the proposed combinations; tabs or control characters inside values; non-ASCII names/units (e.g. µ); checksum treatment of delimiters; absent flags represented as empty strings; result conversion/rounding; field truncation in fixed-width alternatives; switched list conventions; changed script without matching parser; lack of timestamp/run/position in this proposed message. Treat `<NC>` as a non-result, never zero. No literal flag meaning such as H/L or analytical failure is assigned here without a verified vocabulary. This script is a design candidate for Dialab to validate, not an installable instruction.

## 5. Sample identification, with and without barcodes

### Barcode option and symbologies

M p. 67 says the barcode reader is optional; it adds Scan Tray and Scan Tray and Run, each with All/Single Position. M pp. 138–139 describe host/manual/profile acquisition and reader behavior.

Default reader formats (M p. 139): EAN-13, Code 39, Codabar, and Interleaved 2 of 5 with 6, 8 or 10 digits.

The module stores one to six format configurations. Additional blocks can be supplied on request and uploaded by USB pen. The documented request list includes Code 128, EAN 128, Code 39 Full ASCII, Code 93, Plessey, Pharmacode and EAN/UPC variants; these are **not** proof Code 128 is already enabled. EAN13-specific constraints: twelve data digits plus generated check digit; the manual says its first digit must not be zero (p. 139).

### ID length and exact 8+7 rule

Host patient code is 15 characters. Barcode setup permits up to 15 sample-ID digits depending on symbology (p. 138); the preceding general text mentions a numeric maximum-13 format, so do not assume every symbology carries arbitrary 15-character text. Optional serum/urine and profile digits can occupy additional barcode positions. Patient ID has priority over suffix fields when lengths overlap (p. 138).

M p. 175 explains padding with an example: with a 15-character code field, an 8-character code is sent as those 8 characters followed by 7 spaces. The code read from the barcode label must follow the same 8 + 7 pattern to be matched.

The exact documented host identity for an eight-character ID is that ID followed by seven spaces, not seven leading zeroes. The manual does not explain whether the scanner itself normalizes a short printed barcode to that padded identity. Confirm the combination of printed label, scanner configuration and host padding; do not assume literal trailing spaces must be physically encoded or that generic trim() is safe.

M p. 175 additionally removes **characters below space** from the beginning/end of the patient code; space itself is explicitly excluded from this removal. If nothing remains the code becomes `'{0}'`. This is not permission to strip ordinary spaces or assume variable-length framing in the fixed-width protocol.

### Without barcodes: supported and explicit

M p. 138: a sample without a barcode must go exactly into the position assigned when it was programmed. A sample with a barcode can go into any free position; its assigned number does not matter.

Barcode-free workflow: host sends patient code + tests + explicit two-digit cup position; analyzer accepts with `Y + position byte`; staff place the physical sample in that accepted position. With `00`, the analyzer assigns a position and staff must use the returned assignment. The lab can also enter code and position manually in Patients → Insert Routine/STAT. No barcode reader is required for those associations.

**Result matching is by patient code**, because the result includes that programmed code. **Position-only matching is not supported by the documented result layout**: neither internal result header nor the reporting variable list contains cup position. A connector-side position↔accession map may assist handling, but cannot recover an omitted or ambiguous result ID from a wire position that was never sent. Making patient code equal a position is a local convention, not a vendor position identifier; reused positions and reruns make it insufficient as a stable sample accession. Use a unique sample/visit accession in patient code, not merely a lifelong demographic patient ID, when clinically approved in a future design.

Batch acquisition differs: M p. 64 says a running `Batch # xx` report identifier is assigned without patient data entry. Do not assume host accession matching for this mode without a capture and an agreed workflow.

### Additional/extra list and duplicates

M pp. 66–68: Extra Patients contains acquired patients not inserted into the run list. Send to Current Tray assigns free positions; drag/drop can choose a free position. Setup's Add to extra patient list governs manual New Entry behavior (p. 116); it does not document a host command for an extra-list target.

M p. 139: if the same barcode is read in two different positions, the analyzer discards both codes, because it cannot tell which one is wrong. The samples must then be programmed again.

When a barcode-bearing tube occupies a position programmed for a sample without barcode, the barcode sample stays and the unbarcoded patient moves to the additional list. Staff must inspect the accepted worklist after scanning. Duplicate host codes are separately subject to 0x0B and clone/repetition/extra-list errors; do not equate every duplicate barcode with a permitted clone.

## 6. Test codes, configuration and QC

Host analysis codes have **four bytes**, for example a three-letter code plus one space such as `ALB `; padding is part of the checksum. They must match the analyzer's configured codes and current reagent tray, not LOINC, app catalogue labels or vendor reagent references. M p. 38 documents Tests → All Tests → New Code, Save, Rename, Clear Code, then Parameters; Modify Current Tray brings tests online. **Lab-defined where open software/permissions allow; not universally editable on closed software.** Obtain the installed code/units export, not a guessed standard menu. In the manual's worklist example, the description names a glucose code GLY while the payload sends GLU; use the actual configured code.

QC is transmitted when Setup → Serial → **Send also the controls** is enabled (p. 119). The added header distinguishes normal/QC, known/unknown, level and lot. These fields are present even for normal samples. Variable reporting offers `ISCONTROL`, `CONTROLTYPE`, `CONTROLLEVEL`, `CONTROLLOTNAME`; variable reception also has fields to create control worklist entries. The standard patient-download layout has no control-creation field: do not invent one or use the list-type letter as QC type.

The internal controls-disabled format has no QC marker. It is intended to exclude controls; enabling controls without updating the parser shifts every field. QC routing/handling must be explicit in the eventual connector profile. This research does not add a QC product feature or invent QC catalogue mappings.

## 7. Annotated example exchanges

Notation: `H>` host, `D>` analyzer; `<STX>`, `<ACK>`, `<EOT>`, `<NAK>` represent single bytes, not the printed angle-bracket strings. All spaces in payload examples are significant; there is no implicit CR/LF. Request/response timing caveat from §3 applies. All payloads below are **synthetic**: the patient code, tests, positions and results are made up for this note.

### Send a worklist patient: synthetic payload

Built from the layout on M pp. 175–176.

```text
H> <STX>                                      [02]
D> <ACK>                                      [06]
H> TEST0042       TSN0502ALB CRE 139<EOT>
D> Y<position-byte>                           [59 pp]
```

Payload is patient code `TEST0042` plus seven spaces (the 8 + 7 rule), routine `T`, serum `S`, clone `N`, requested position `05`, two tests `02`, codes `ALB ` / `CRE `, checksum `139` (bytes `31 33 39`), EOT `04`. The 30 bytes before the checksum sum to 1675, and 1675 mod 256 = 139. `pp` is a placeholder, not a claimed analyzer reply value. Failure alternative: `N<error-byte>`; for example `4E 0C` means at least one requested test is not defined on the analyzer.

The manual's own worklist example (p. 176) is internally consistent: its printed checksum matches its byte sum. Its narrative names the code GLY while its payload sends GLU; using GLY would change that checksum (from 142 to 146).

### Poll next result, receive a report, end operation

Built from the request and layout on M p. 177.

```text
H> <STX>                                      [02]
D> <ACK>                                      [06]
H> R<EOT>                                     [52 04]
D> TEST0042       TS002ALB 0004.21CRE 00087.5191<EOT>
```

The report has patient code `TEST0042` plus seven spaces, routine `T`, serum `S`, count `002`, then two results: `ALB ` with `0004.21` and `CRE ` with `00087.5`. It ends in checksum `191` (bytes `31 39 31`) and EOT `04`. The 42 bytes before the checksum sum to 2239, and 2239 mod 256 = 191. The result widths follow the layout; the zero-padded number style is illustrative, not a confirmed format. The host ends its request with EOT; the analyzer ends the report with EOT. No extra result ACK is documented. This is an operation exchange, not proof that a physical COM port or TCP session must close each time.

### Empty queue or replay

```text
H> <STX>
D> <ACK>
H> R<EOT>
D> <NAK>                                      [15]
```

Use ASCII `L` (`4C`) instead of `R` to request the last sent report after a receive problem. Use `A` (`41`) to restart at the first report; subsequent `R` requests are the **inferred** traversal for retrieving all reports, not one command that streams an unspecified number of frames. Never invent a `Q`/logout command; EOT is the documented operation terminator. Replay behavior after partial sessions remains to be verified.

## 8. Analyzer-side setup and observation checklist

These are future site steps, not actions performed during this research:

1. Record actual software version (21.18 vs 21.18M or another release), installed sample/reagent readers, ISE presence and whether parameters are open. Preserve the existing analyzer↔PC hardware link.
2. Utility → Setup Analyzer → Serial → Present (M p. 119). Record the selected Transport and inspect its options before choosing Serial or TCP/IP.
3. For Serial, record COM Number, complete Baud-Rate/Hand-Shake dropdowns and all advanced parameters; configure the connector peer consistently. Use the p. 179 approved wiring, verified for the installed connectors. For TCP/IP, obtain address/port/role and exact host protocol from Dialab.
4. Select Internal Protocol initially if its result fields suffice. If variable protocol is necessary, use the dedicated protocol-options icon, qualified personnel and a saved/compiled transmitting **and** receiving script (pp. 119, 180). Keep their exact versions paired with the connector parser.
5. Decide with the lab whether to validate each run using Utility → RS232 → Accept result to be sent, or enable automatic validation in Setup → Serial. Record Send also the controls and the setting that sends `<NC>` instead of a value. Do not use Delete Result to be sent to clear an integration problem.
6. Tests → All Tests / Current Tray: export the actual four-character codes, unit configuration, conversion factors and active tray. Resolve GLU/GLY differences against the machine, not the example narrative.
7. For no barcode reader, program patient code and accepted cup position, physically place the labelled sample at that position, then inspect Patients → Insert Routine/STAT. If barcode is installed, Setup → Bar-Code configures ID length, serum/urine digit and profile digits (M pp. 118, 138–139); confirm symbology and padding by capture.
8. On nonclinical qualification material, observe an ordinary result, empty queue, rerun, non-calculable result and QC if enabled. Record exact bytes and timing in the protected store; compare accepted worklist identity to the returned code before any app integration. Do not send clinical captures to external services.

## 9. Fit with OpenELIS Analyzer Bridge

### Source snapshot and actual limitations

Read [README](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md), [profile authoring](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md), and **all three files** in [serial/](https://github.com/DIGI-UW/openelis-analyzer-bridge/tree/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial). Snapshot: `develop`, commit `5a3b8a361f873af566bfaae8a549d3b80a4d958e`, resolved 2026-11-14. Develop is the inspected source, not a promise about a deployed Docker tag.

| Source                                  | Observed behavior                                                                                                                                                                                              | Implication                                                                 |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `SerialPortListener.java`               | Opens jSerialComm port, sets parameters, installs DATA_AVAILABLE/DISCONNECTED listener, reads available bytes; writes only response arrays returned by frame buffer; scheduled tasks handle timeouts/reconnect | No periodic host request and no STX/ACK/R poll initiator                    |
| `SerialFrameBuffer.java`                | Only ASTM/HL7 accepted; ASTM receiver waits for analyzer ENQ and sends ACK, parses numbered frames/checksum; HL7 receives MLLP and returns ACK                                                                 | Neither framing state machine matches Autolyser                             |
| `SerialMessageHandler.java`             | Builds SERIAL MessageEnvelope and invokes MessageNormalizer; direct handler explicitly rejects protocols other than ASTM/HL7                                                                                   | Cannot configure a raw proprietary fixed-width message into working support |
| `model/Protocol.java`                   | ASTM, HL7, CSV, UNKNOWN                                                                                                                                                                                        | No Autolyser protocol enum/strategy                                         |
| `routing/NormalizedBundleRenderer.java` | Profile-pinned parsing into normalized FHIR bundles; parser dispatch includes ASTM/HL7                                                                                                                         | A driver alone does not supply the proprietary result parser                |

**Confirmed with qualification:** the current **serial acquisition path** listens/responds; it cannot initiate this host-driven poll. Saying "the whole Bridge only listens" would be false: README also documents outbound HL7 order sessions over TCP/MLLP. Serial ACK responses and reconnect activity do not constitute analyzer-result polling.

Profile authoring covers immutable/reviewable profile revisions, vendor-sourced test definitions, code overrides, units, result parts, number format and control recognition. It does not add a transport state machine by JSON configuration. The Autolyser protocol is neither ASTM LIS2 nor HL7 and must not be labelled ASTM simply to pass current serial validation.

### Proposed minimum addition: active polling serial driver and parser

**PROPOSED architecture only; no code written.** Keep acquisition in an explicit connection-owned strategy, alongside the existing reactive framing strategies, with one owner of each COM port. A future implementation would need protocol/profile validation and parser dispatch changes as well as the driver.

```text
DISCONNECTED -> OPEN -> IDLE
IDLE --interval--> SEND_STX(02) -> WAIT_ACK(06)
WAIT_ACK --ACK--> SEND_R(52), END_REQUEST(04) -> READ_RESPONSE
READ_RESPONSE --NAK(15)--> IDLE until next poll
READ_RESPONSE --payload through EOT(04)--> VERIFY_LENGTH_AND_CHECKSUM
VERIFY -> durably retain raw payload + normalized context -> PARSE_AND_DELIVER -> IDLE
timeout / malformed data / checksum failure -> explicit failed operation -> recovery policy
```

`SEND_R/END_REQUEST` ordering follows the manual reading, with timing to be qualified before implementation. A proposed initial idle interval could be 5 seconds, **not vendor-specified**; ACK/inter-byte/report timeouts and backoff remain unset until confirmed. Recovery should consider `L` for the last report; `A` is an explicit replay/reconciliation operation, not normal per-poll behavior. Do not advance to another `R` after a corrupt result without resolving the last-report recovery policy. Serialize optional patient downloads with polling on the same connection.

Parse exact byte counts and configured mode; verify checksum before interpreting a patient identity or clinical value. Retain raw bytes and original result text. Map patient code to specimen/accession identity, analysis code to the pinned assay definition and site mapping, numerical value and comparator only when present, unit when actually transmitted/configured, flags without invented meanings, and QC markers to control handling. Preserve S/U and list semantics as context. Do not invent dates, patient demographics, unit values, normal ranges or "final/released" status absent from the source.

The current normalized delivery is **FHIR**, not an arbitrary JSON `{patient,test,value}` endpoint: use the existing `MessageNormalizer`/outbox and `NormalizedBundleRenderer`/FHIR-building boundary for an added parser, with one Observation per actual test result and preserved raw data. Follow the Bridge's existing durable receipt practice before requesting the next result. There is no Autolyser receipt ACK to delay; local persistence and `L` recovery must cover the risk of a poll moving the analyzer's send cursor. The wire lacks a clear immutable run ID, so correction/rerun identity and duplicate handling need explicit qualification; hashing only code+value can collapse genuine repeats.

Optional worklist support: create the exact §3 patient frame from a typed work item and installed test-code mapping, await `Y + position` or preserve the specific `N + error`, and record the accepted position. A lost reply is an uncertain outcome, not a successful save or permission to blindly resubmit/clone. Standard download does not send names or QC metadata; those need variable reception if required. This is an optional later capability, not needed to prove result polling.

### Windows and destination implications

- jSerialComm is the source's native serial library. A native Windows JVM process is a plausible way to reach a Windows `COMn` port; exact JVM/native-library/OS compatibility remains to be verified for that PC.
- A physical COM endpoint normally has one process owner. An analyzer application and Bridge cannot both use the same endpoint as if each were the peer. On separate machines use opposite ends of the cable; on the same PC, a deliberate paired-port arrangement or the documented TCP mode would be required. Do not capture the analyzer-control USB port.
- README's Docker serial instructions map `/dev/ttyUSB0`; that Linux device mapping is not an instruction for passing a Windows COM port into Docker Desktop. WSL/VM/container device access must be separately qualified. Native Windows serial support is not proof the project's deployment scripts support a Windows service out of the box.
- The Bridge forwards normalized bundles to OpenELIS `/analyzer/fhir`, with pairing/security and durable delivery. EchoMed does not thereby gain a compatible receiver. A future app adapter/API, specimen/test mapping and audit policy require an authorized feature plan. Research does not establish a plug-and-play connection to this web app.

## 10. Open questions and source confidence

### Ask the lab first

1. What exact analyzer software/OS versions are installed? Is there an M suffix? Are barcode readers or ISE physically fitted, and were they added after purchase?
2. What does Setup → Serial show, including TCP/IP settings, every baud/handshake choice, data bits/parity/stop bits and selected protocol? Supply configuration observations without patient data.
3. Which physical/virtual COM ports belong to the analyzer-control USB link and the host link? Is there an existing LIS peer? Which cable/reference and connector sexes are present?
4. What are the actual Tests/Current Tray codes and first/second units? Is the software open or closed? Who is allowed to edit parameters?
5. Without barcodes, who loads the accepted cup position and verifies identity? Are batches, extra lists, clones, reruns and mixed barcoded/unbarcoded runs used?
6. Is result validation manual or automatic? Are controls and `<NC>` transmitted? Can nonclinical qualification records and byte captures be obtained?

### Ask Dialab / authorized service

1. Confirm REF 20010D's 2020 configuration and possible upgrades; identify whether "PLUS" is a legitimate alternative name and whether any such device shares this interface.
2. Provide the complete Rev18/installed-version host specification: serial supported values, default data bits, exact hardware handshake lines, approved cable and connector/ground details.
3. Provide TCP/IP dialogue/settings: socket client/server role, address/port, licensing, persistence and whether bytes/checksums/request timing are identical to serial. Can the control-PC app communicate to a connector on the same Windows host?
4. Confirm `R` request EOT timing, all response terminators, ACK/busy/error behavior, timeouts/poll limits and send-cursor advance/persistence. Define exact L/A recovery semantics after interrupted polls and restarts.
5. Correct the controls-enabled p. 178 example, unexplained spaces and checksum 063. Confirm the p. 178 field table applies to both known and unknown controls and normal samples on this software.
6. Confirm limits/encoding for patient code, result, units, names and flags; exact `<NC>` bytes; complete flag semantics; conversion-unit/range behavior and results for ISE/relation tests.
7. Validate the proposed paired scripts, spellings (especially `ANALYSENAME`, `PATIENTCONTROLKNOK`), tab-delimited fields, constants/terminators in checksum and compiler limits. Explain why the p. 187 example omits list type although the standard header includes it.
8. Confirm scanner short-code normalization, eight-plus-seven padding, EAN restrictions and available symbology programming. Confirm successful position-byte encoding and any extra-list special values.
9. How are corrected/rerun results identified without a timestamp/run ID? Can a script expose a stable execution identity beyond the listed fields? Do not assume an undocumented variable exists.

### Sources and confidence ledger

| Source / key claim | Evidence class | Confidence and limit |
| --- | --- | --- |
| Rating label as reported by the requester | Requester evidence | High for stated label, not independently photographed/verified |
| M pp. 1–2, 38, 159 | Manual | High for Rev18 identity, open/closed software, USB versus host connectivity; installed version unknown |
| M p. 119 Figure 15-7 | VERIFIED visually | High: Serial/TCP/IP selector exists; socket behavior UNVERIFIED |
| M pp. 175–177 | Manual; checksums of its standard examples recomputed | High: host initiation, layouts, R/L/A, NAK and validation; detailed timings UNVERIFIED |
| M p. 176 | Error table VERIFIED visually | High for 0x01–0x18; meanings restated in our words |
| M p. 178 | Layout VERIFIED visually; contradictory example | High for printed field definitions, low for example's spaces/checksum pending vendor correction |
| M p. 179 | Diagrams and checksum rule VERIFIED visually | High for printed wiring; checksum span inferred from consistent standard examples; site cable still unverified |
| M pp. 180–187 | Manual tables and examples; pp. 183–184 VERIFIED visually | High for listed variables and list-code differences; proposed script uncompiled/unverified |
| M pp. 64, 66–69, 116, 138–139, 175 | Manual | High for barcode-free placement, duplicate handling, default symbologies, extra list and padding rule; scanner normalization unknown |
| Official Autolyser product page | Primary public product source | High for current product family/options; not a 2020 configuration record |
| October 2025 product flyer, p. 3 | Primary order table | High for current 20010D/no-reader association; applying it to the installed 2020 unit is INFERRED |
| Official Autolyser 100, 450, ISE product pages | Primary public model pages | High for separate model names; protocol equivalence not established |
| Service Manual, software 14 | Vendor-authored older document | Corroborative only: LAN/TCP/IP and optional modules; not authority to override Rev18 |
| [Bridge README at pinned SHA](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md) | Primary source code/docs | High for inspected snapshot: FHIR forwarding, Linux serial deployment and outbound HL7 orders; deployed tag unknown |
| [Bridge profile authoring at pinned SHA](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md) | Primary docs | High for profile responsibilities and source requirements; not a custom-driver facility |
| [SerialFrameBuffer](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialFrameBuffer.java), [SerialMessageHandler](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialMessageHandler.java), [SerialPortListener](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialPortListener.java) | Primary code, entire serial directory read | High: reactive ASTM/HL7 acquisition, no Autolyser poll initiator |
| [Protocol enum](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/model/Protocol.java), [MessageNormalizer](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/normalizer/MessageNormalizer.java), [NormalizedBundleRenderer](https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/routing/NormalizedBundleRenderer.java) | Primary code | High for protocol restriction and normalized FHIR boundary; future extension is PROPOSED |

Verification performed: source comparison, visual inspection of the manual's tables and diagrams, ASCII checksum calculations for the manual's standard examples and for the synthetic examples above, and read-only Bridge inspection. No analyzer contacted, no script compiled, no serial/TCP capture, no app code changed. Application builds/tests were not run: this one-file research task neither changes nor claims runtime behavior.
