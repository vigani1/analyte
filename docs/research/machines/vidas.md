1. UDI-DI 03573026351663 identifies the original full-size VIDAS, REF 410417; installed software is unknown.
2. The LIS connects to VIDAS PC/BCI, normally by RS-232; original VIDAS supports upload and request download.
3. Literal/Alternate are proprietary protocols with checksum before ETX, incompatible with unchanged ASTM E1381 framing.
4. Uploaded sample identity is the entered/downloaded `ci` sample ID; the documented result has no A1–E6 position field.
5. The Bridge needs new framing and content parsing; confirm versions, ID limits, cable, timing and assay codes before building.

# bioMérieux VIDAS connectivity research

Research date: 2026-11-14. Scope: original beige five-section VIDAS controlled by VIDAS PC. Research only; no application code or instrument settings changed. Biotek Solutions (North Macedonia) is the distributor supplied by the task, not independently verified here.

**VERIFIED** means directly supported by vendor-authored documentation or inspected source, not bench-tested. **INFERRED** means an engineering conclusion from those sources. **UNVERIFIED** means unresolved, unavailable or dependent on installed software. Instrument captures are still needed.

## 1. Identity, generations and typical test menu

**VERIFIED:** the manufacturer IVDR reference list gives REF 410417 as the VIDAS analyser, with UDI-DI 03573026351663. This matches the supplied label after removing GS1 application identifier `(01)`. The list separately identifies mini VIDAS REF 410416, UDI-DI 03573026351656, and VIDAS 3 REF 412590, UDI-DI 03573026380175. A manufacturer notice, table 1, corroborates REF 410417 and this UDI. Its affected software versions do not establish this unit's version.

| System/generation             | Connectivity distinction                                                                                                                         | Evidence/confidence                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| Original VIDAS + VIDAS PC     | Five sections A–E, six positions each; external PC manages sample identity/loading. Analyzer-to-PC wiring is separate from PC-to-LIS wiring.     | VERIFIED: PC manual, introduction/glossary; BCI B §2-1.                 |
| Legacy PC BCI RS232 / BCI NET | 2008 manual describes separate COM Manager and network-link configuration; uni- and bidirectional workflows.                                     | VERIFIED: PC chapter 10; installed generation UNVERIFIED.               |
| Later VIDAS PC + BCI Link     | BCI Link on PC provides serial Literal or ASTM-XML over FTP. Guide A §4 references PC `VU_06`; guide B is January 2017.                          | VERIFIED: A §4-1; B §2. Not automatically applicable to every older PC. |
| mini VIDAS                    | Standalone; rear COMPUTER DB9; upload only, no request download; no stored patient demographics. Different field subset/RS placement.            | VERIFIED: A §§2-3, 5, 6-3; B §2-3.                                      |
| VIDAS 3                       | Computer-to-instrument network connection; guide supports LIS serial through its computer and BCI Link FTP. Extra XML traceability/flags differ. | VERIFIED: A §§2-2, 8, appendix B; B §2-2.                               |

The manufacturer assay menu lists D-Dimer Exclusion II, B·R·A·H·M·S PCT, TSH/TSH3, HCG, TOXO IgM/IgG/avidity, RUB IgM/IgG, CMV IgM/IgG/avidity, HIV DUO Ultra/Quick, HBs Ag Ultra/anti-HBs, ferritin and high-sensitive troponin I, alongside other thyroid/fertility/serology assays. This is a range menu, not the installed assay list. Country clearance, assay revision and instrument compatibility require local confirmation. Do not include menu entries marked VIDAS-3-only on the original instrument.

## 2. Physical connection, cable and network options

**VERIFIED:** B §2-1 places BCI Link on VIDAS PC and the LIS connection at that PC's COM port. One or two VIDAS analyzers can connect to one PC. Do not mistake an analyzer-control connection for the LIS port.

**Cable:** A §5-1 says the mini cable is the same as the VIDAS BCI cable, and that wires 2 and 3 must be crossed. Crossed transmit/receive is therefore VERIFIED. For DB9-to-DB9, `2↔3, 3↔2` follows directly. Ground `5↔5` is a conventional three-wire null-modem inference, **UNVERIFIED as part of the complete vendor pinout**. PC-end connector genders, shield and RTS/CTS or DTR/DSR bridges remain UNVERIFIED. The mini's female instrument-end plug and P/N 186036-x must not be assumed to describe every PC cable.

| Parameter                  | BCI Link documentation                                                          | Confidence                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Port                       | Default COM3; any of COM1 to COM4 can be chosen                                 | VERIFIED A §3-8; the port in use here is unknown.                                             |
| Baud                       | Default 9600; settings from 110 to 256000                                       | VERIFIED, same section.                                                                       |
| Data/parity/stop           | Default 8 data bits, no parity, 1 stop bit                                      | VERIFIED as defaults; both ends must match the real settings.                                 |
| Flow control / modem lines | The guides do not settle which mode is required                                 | UNVERIFIED. ENQ/ACK is an application handshake, separate from flow control.                  |
| Payload characters         | The later guide limits RS232 content to plain 7-bit ASCII; this fits in 8 bits | VERIFIED A §4-1. The old PC screen offers ISO-8859-1; settle the version before non-ASCII data. |

**VERIFIED network alternative:** B §§2-4 onward and A chapters 7–9 describe vendor-schema XML over FTP: `lisMessage` requests, `aiMessage` results, based on ASTM 1394-97 attributes. This is neither E1381 framing nor ordinary pipe-delimited ASTM H/P/O/R content. Guide A makes the LIS the FTP client: STOR into `/download`, list/RETR from `/upload`, binary transfer. Names use `<prefix>_yymmdd_hhmmss_<sequence>.xml` (FTP account prefix for requests, system prefix for results), with recommended polling every 10–30 seconds. Confirm paths/retention/schema for installed BCI. Old BCI NET demonstrates a network option, not compatibility with every later XML schema. A TCP socket carrying Literal was not established.

## 3. Literal and Literal Alternate low-level protocol

Control names/roles: VERIFIED A/B §§3-1–3-7. Numeric bytes: independently verified standard ASCII in RFC 20 §2.

| Token | Hex | Decimal | Function                                                     |
| ----- | --- | ------- | ------------------------------------------------------------ |
| ENQ   | 05  | 5       | Asks to open a session                                       |
| ACK   | 06  | 6       | Ready, or packet accepted                                    |
| NAK   | 15  | 21      | Not ready, or packet refused (for example checksum or parity) |
| STX   | 02  | 2       | Starts a packet                                              |
| RS    | 1E  | 30      | Starts a payload chunk                                       |
| GS    | 1D  | 29      | Ends the checksummed bytes; the checksum comes next          |
| ETX   | 03  | 3       | Ends a packet                                                |
| EOT   | 04  | 4       | Ends a session                                               |
| CR    | 0D  | 13      | Optional suffix after a control byte, if configured          |
| LF    | 0A  | 10      | Optional suffix after a control byte, if configured          |
| Pipe  | 7C  | 124     | Ends each tagged payload field                               |

Names below stand for actual single bytes; `hh` is two ASCII hex digits. Schematic, not a live capture:

```text
Literal:
sender -> receiver: ENQ
receiver -> sender: ACK (NAK if not ready)
sender -> receiver: STX RS chunk [RS chunk ...] GS hh
receiver -> sender: ACK (NAK for bad packet)
sender -> receiver: ETX EOT

Literal Alternate:
sender -> receiver: ENQ
receiver -> sender: ACK
sender -> receiver: STX RS chunk [RS chunk ...] GS hh ETX
receiver -> sender: ACK (NAK for bad packet)
sender -> receiver: EOT
```

**VERIFIED:** Alternate moves packet ACK after ETX; normal Literal expects ACK before ETX. One STX, one ETX, at least one RS and one GS per packet. Each RS text chunk allows up to 80 printable ASCII characters; download description gives 1920 bytes maximum across RS blocks. RS is a transport chunk boundary, not a test/field boundary; it can split a field. Reassemble before parsing. Multiple requests may use separate sessions or several STX…ETX packets in one ENQ…EOT session. Upload uses one complete session per result. A/B §§3-1–3-4.

**Checksum — VERIFIED:** sum all transmitted bytes **from first RS through GS inclusive**, retain low eight bits, encode as two ASCII hexadecimal digits. Exclude STX, hex digits, ETX and EOT. Include later RS bytes, all payload spaces and pipes. There is no frame number. Guide A §3-7 specifies lowercase vendor-sent hex letters and acceptance of either case. Guide A §3-6 permits selected controls to receive CR/LF suffixes, disabled by default. The guide does not make clear whether the checksum covers these optional suffixes: **UNVERIFIED**; confirm before enabling them.

**Timeouts/retries — UNVERIFIED:** upload retries are mentioned, but the guides reviewed give no reliable numbers for ACK timeout, interbyte/session timeout, retry count/backoff, collision handling or abort sequence. Do not import ASTM timing constants. Obtain BCI Link user manual ref. 4501-2122 and read the actual link settings. Transport acceptance and subsequent application rejection are distinct.

## 4. Message format, complete field table and assay codes

**VERIFIED:** A §§4-2–4-8: two-character tag + value + terminal pipe; no separator between tag and value. No space before a tag, no pipe or CR inside a value. Empty fields may remain present; field order matters.

Below is the **complete 20-tag PC/VIDAS 3 RS232 field inventory in guide A**, not a claim about every historical BCI command or XML attribute. D = LIS→PC; U = PC→LIS; M = mandatory; O = optional. Optionality comes from the data table; demography mode/configuration still applies. Cells are our own summary.

| Tag | Meaning                    | Direction / presence | Format / rule                                                                                                                                       |
| --- | -------------------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| mt  | Kind of message            | D/U M                | Three-letter value: `mpr` for a request, `rsl` for a result, so the field reads `mtmpr` or `mtrsl`.                                                 |
| pi  | Patient ID                 | D/U M                | Up to 17 characters. A download with it absent, empty or too long is refused. Each ID maps to one set of demographics. See simplified-mode note below. |
| pn  | Patient name               | D/U O                | Up to 30 characters; longer names are cut.                                                                                                          |
| pb  | Date of birth              | D/U O                | Shown as yyyy/mm/dd; the link setting decides the real layout. A badly formatted date is ignored.                                                   |
| ps  | Sex                        | D/U O                | One character. In English M, F or `-` (the default). Letters depend on the UI language.                                                            |
| so  | Where the sample came from | D/U O                | Up to 5 characters. Origin metadata; not a section or position.                                                                                     |
| si  | Start of specimen part     | D/U M                | Always empty; marks structure only. Not a sample ID and not a specimen type.                                                                        |
| ci  | Sample ID                  | D/U M                | Up to 17 characters in the later guide. A download with it absent, empty or too long is refused. Controls are special; see below.                   |
| rt  | Assay code (short)         | D/U M                | Up to 4 characters. An unknown code is refused, possibly with no alarm. Absent or empty is an error.                                               |
| rn  | Assay name (long)          | U O                  | Up to 40 characters.                                                                                                                                |
| tt  | Time the test finished     | U M                  | HH:MM.                                                                                                                                              |
| td  | Date the test finished     | U M                  | Shown as yyyy/mm/dd; the configured date layout applies.                                                                                            |
| ql  | Interpretation             | U M                  | Up to 29 characters. Can be empty for a quantitative assay. See the contradiction note below.                                                       |
| qn  | Measured value             | U M                  | Up to 29 characters. Can carry a comparator, unit, spaces or failure text. A global call over several sub-results can leave it empty.              |
| y3  | Unit of qn                 | U O                  | Up to 30 characters. The table says PC leaves it out for qualitative assays, yet the TXG example has it.                                            |
| qd  | Dilution                   | D/U O, conditional   | Whole number: 1–999 on PC, 1–9999 on VIDAS 3. Read on download only when the dilution option is on; then a quantitative request without a valid dilution is refused. |
| nc  | Validity flag from VIDAS   | U O                  | Up to 15 characters. Reports calibration, control or lot state; not a positive or negative call.                                                   |
| id  | Instrument ID              | U O                  | Up to 15 characters, set in configuration.                                                                                                          |
| sn  | Serial number              | U O                  | Up to 20 characters, set in configuration; may contain spaces.                                                                                      |
| m4  | Operator                   | U O                  | Up to 20 characters; the person who ran the test.                                                                                                   |

Expected tag order:

```text
request: mt pi pn pb ps so si ci rt qd
result:  mt pi pn pb ps so si ci rt rn tt td ql qn y3 qd nc id sn m4
```

**Qualitative versus quantitative:** in guide A's TSH result example, ql is empty and qn holds the number with its unit. In its HIV examples, ql holds NEGATIVE and qn holds the index. Preserve comparator and exact units; never turn ND/invalid into zero. **Source contradiction:** A §4-7 has a note reversing TV/interpretation between ql/qn, while column descriptions and examples show ql=call, qn=value. Verify against applicable revision and capture. Guide A specifies decimal dot; the older PC trace has comma decimal, no unit space and ratio dilution `1:1`. These cannot be collapsed into a universal format. Date order is configured, not inferred from OS locale.

**Calls:** A appendix B lists POSITIVE/EQUIVOCAL/NEGATIVE, REACTIVE, NONREACTIVE, LOWPOSITIVE, BORDERR, INTERMEDIATE, LOW, HIGH, INVALID, ND and other assay-dependent strings. Examples include localized `Equivoque`. Numeric appendix indexes are not evidence that ql transmits numeric codes. Get the enabled-assay vocabulary and language before mapping.

**nc flags verified:** `lotexp` lot past its expiry; `calexp` calibration past its expiry; `contneff` control not run; `calinc` calibration incomplete, invalid or missing; `conthwst` control breaks a Westgard rule; `conthnor` control out of range; `valid` calibration is valid. A appendix B-7; PC §10-8. `valid` does not mean clinically normal. Later guide marks lotexp/calexp PC-only. VIDAS 3 XML flags are not presumed available here.

**Grouping verified:** one assay request per logical message; one uploaded result per session. Main HIV5 can generate separate global/AB/AG messages sharing ci; do not overwrite them or treat AB/AG as unrelated assays. Download the main assay, not sub-assays. Dual assays upload test results, not reference results. Cx controls can put sample ID plus a space and strip lot in ci; QC results use QC ID and empty patient demographics. These are not patient accessions. A chapter 6.

| Assay                                                                          | Verified code evidence          | Gap                                                                                                         |
| ------------------------------------------------------------------------------ | ------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| D-dimer                                                                        | DD2 in A §3-5 frame example     | Confirm exact installed D-dimer revision.                                                                   |
| TSH                                                                            | TSH in A §§3-9/6-1              | Does not prove TSH3 code.                                                                                   |
| hCG                                                                            | HCG in PC §10-26 trace          | Original-PC example verified.                                                                               |
| Toxo IgG II / avidity                                                          | TXG / TXGA in A chapter 6       | Confirm installed revision.                                                                                 |
| Rubella IgM                                                                    | RBM in A §9-10 XML example      | Vendor testIdentifier verified; local serial rt needs confirmation.                                         |
| HIV DUO Ultra                                                                  | HIV5, AB, AG in A chapter 6     | Some of these examples carry a VIDAS 3 instrument ID; verify original-PC output.                            |
| HIV P24 II                                                                     | P24 in A §6-3 QC example        | Example verified.                                                                                           |
| CMV avidity                                                                    | CMVA parameter in B §4-8 update | Not a general CMV code.                                                                                     |
| PCT, toxo IgM, rubella IgG, CMV IgG/IgM, HBs Ag/anti-HBs, ferritin, troponin I | **UNVERIFIED rt values**        | Obtain installed assay code export and vendor interface mapping; never derive from kit REF or English name. |

## 5. Directions and request download

**VERIFIED:** original VIDAS PC supports upload-only or bidirectional BCI, unlike mini VIDAS. PC chapter 10 defines download as the assays that the LIS sends to VIDAS PC.

LIS initiates ENQ, waits for ACK, sends ordered mtmpr data, follows selected Literal ACK timing, then closes the session. BCI imports into the work/loading list. Staff still load sample aliquots/reagents and start runs. This is order download, not evidence of a barcode-triggered host query protocol.

On upload the LIS answers ENQ with ACK when ready (NAK otherwise), validates the checksum and acknowledges at the configured point. No HL7 ACK or separate ASTM result message is documented. Packet ACK confirms transport, not clinical acceptance. Unknown test requests can be rejected after transmission; inspect the work list/error log. A §§3–4, 10; PC §§10-24–10-26.

## 6. Sample identification — the critical distinction

**VERIFIED:** original VIDAS internal barcode reading identifies reagent strips; it is not an automatic primary-tube sample barcode reader. The sample identifier is entered in VIDAS PC, optionally using an external handheld barcode reader. With simple demographics, the manual says the operator enters only the sample identifier (ci). PC §§10-5/10-22, chapter 4. No automatic tube reader does not mean IDs cannot be scanned at the PC.

**Without a barcode, uploaded ci is the typed sample ID**, or the ID from the downloaded request. Match against that accession/sample identity. Section/slot is the physical loading location. The complete 20-tag schema has no section/position tag. **INFERRED from that inventory:** A1–E6 cannot be recovered from a normal Literal result unless someone deliberately typed a position-like string as ci; that is an entered ID, not location metadata. `so` means origin; `si` is an empty marker. A missing ci is not documented as automatically falling back to section+position.

**Version-dependent ID limits:** 2008 PC manual §4-14 requires sample ID up to 12 characters; §12-18 restricts it to alphanumeric. Later A §4-5 gives ci up to 17, and later examples include spaces in general IDs. Installed length/punctuation rules are **UNVERIFIED**. Twelve ASCII letters/digits is a conservative candidate for confirmation, not a newly established specification. Never truncate a real ID to fit.

Later simplified-demography output can copy ci into pi/pn (A §6-3), so pi is not reliably an independent patient ID in that mode. Controls have special ci formats. Multiple tests/repeats share ci; it is not a unique result identifier. Confirm actual entry/scanning and ID reuse practices locally.

## 7. Annotated example messages

Synthetic examples written for this note, with made-up values. They follow the field rules in §4. They are not copied from any vendor document and are not live captures. Each shows the message body only, without STX, RS, GS, checksum or ETX, so no checksum is claimed.

**Result**, synthetic, structure per A §§3-9/6-1:

```text
mtrsl|piSYN-0001|pn|pb|ps-|so|si|ci0004711|rtHCG|rnHCG|tt09:05|td2026/01/02|ql|qn12.5 mUI/ml|y3mUI/ml|qd1|ncvalid|idVIDASPC01|sn|m4tech01|
```

ci is the sample ID; keep its leading zeros. rt is the assay code. qn holds the number and the unit; y3 gives the unit on its own. ql is empty because the assay is quantitative. Name and birth date are left empty on purpose. `valid` in nc is a calibration state, not a clinical judgement; a warning such as `calinc` would have to travel with the result. id names the instrument, not a slot.

**Request**, synthetic, structure per A §§3-9/6-1:

```text
mtmpr|piSYN-0002|pn|pb|ps-|soWARD2|si|ci0004712|rtTXG|qd1|
```

Orders TXG on sample 0004712 at dilution 1. WARD2 is the sample's origin, not a section or position. One request example in guide A leaves out the mandatory rt tag; do not treat it as a valid order.

**Original-PC trace evidence**, PC §10-26: the manual shows an HCG download and the matching upload. In the download, one field is split across two RS chunks, and ACK comes only after ETX, which is Alternate timing. The upload writes qn with a decimal comma and no space before the unit, and writes qd as a ratio (`1:1`). This shows the older PC differs from the later guide's dot decimal and whole-number dilution. This note makes no claim about the trace's checksum values. Never transplant a checksum to a differently chunked message.

## 8. Instrument-side setup and service involvement

Paths VERIFIED for PC version F (05/2008), chapter 10; actual UI may differ:

1. Navigation tree → **Configuration → Routine**: choose demographics and result-validation-before-transmission behavior. Simple demographics is recommended for upload only, complete for bidirectional.
2. **Configuration → Link BCI** (also worded BCI Link): select RS232 link, enable it, review selected upload/download fields, dates, case and instrument identity. Some installation-defined fields are shaded. Save on leaving the screen; retain printed configuration.
3. Windows **BCI RS232** icon → authorized BCI_ADMIN/BCI_ROUTINE login → **Link Operations** → select link → Start (S green), enable Upload (U green); enable Download (D green) for bidirectional use. Upload-only setup leaves Download disabled.
4. **Transaction Log** shows Upload/Download/Both. Automatic result transmission depends on configured validation/printing, not just assay completion.
5. Later BCI Link: **Software administration → RS232 configuration → select link → Update** exposes link settings and optional control CR/LF (A §3-6; defaults §3-8). Read/export actual settings before changing anything.

PC chapter 10 requires qualified personnel for connection work and directs users to bioMérieux for tests. Installation-defined fields may be locked. Contact Biotek Solutions/bioMérieux for cable, BCI availability and authorized setup. Mandatory paid activation or an on-site visit for this unit is **UNVERIFIED**. A COM socket alone does not establish that BCI is installed/enabled. No service credentials are reproduced here.

## 9. Fit with OpenELIS Analyzer Bridge

Inspected public `develop` commit **5a3b8a361f873af566bfaae8a549d3b80a4d958e**. Read [README][readme] and [profile-authoring.md][profiles], plus the serial and ASTM/HL7 Java sources below. Static findings are VERIFIED for this snapshot, not runtime tests.

| Layer           | Source / observed behavior                                                                                                                                                                            | Literal fit                                                                                                                          |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Serial I/O      | [SerialPortListener][port], [SerialConnectionSettings][settings]: configurable port, baud, parity, stops, flow control, RTS/DTR, reconnect, timeout.                                                  | Reusable foundation; settings do not change packet syntax.                                                                           |
| Framing         | [SerialFrameBuffer][frames] accepts ASTM/HL7 only. ASTM expects STX + frame number + text + ETX/ETB + checksum + CR/LF; frame numbers cycle modulo 8, checksum spans frame number through terminator. | **Incompatible unchanged:** Literal uses RS chunks and GS+checksum before ETX, no frame number; normal Literal needs ACK before ETX. |
| Receipt/routing | [SerialMessageHandler][handler] guards ASTM/HL7; durable receipt callbacks precede positive acceptance.                                                                                               | Must admit Literal and adapt persistence to its ACK boundary.                                                                        |
| Protocol model  | [Protocol][protocol]: ASTM, HL7, CSV, UNKNOWN.                                                                                                                                                        | No existing Literal selection or named VIDAS implementation found.                                                                   |
| ASTM content    | [ASTMResultParser][astm] switches on O/P/C/R records; accession comes from O. Profile overrides still address ASTM fields.                                                                            | Cannot interpret mtrsl body via profile alone.                                                                                       |
| HL7 content     | [HL7ResultParser][hl7] expects HL7 segments/OBX; serial HL7 uses MLLP.                                                                                                                                | Neither framing nor content fits.                                                                                                    |
| FTP/XML         | README/profile docs cover file/tabular input; no named BCI FTP client or aiMessage parser found.                                                                                                      | Vendor ASTM-XML needs separate support; choosing ASTM/FILE is insufficient.                                                          |

**INFERRED precise failure:** the Bridge can ACK ENQ, but normal Literal waits for packet ACK after GS+checksum while the Bridge waits for ETX and then trailing checksum/CRLF. Alternate sends ETX, but checksum is already before it; validation still cannot succeed. Stripping the frame leaves unrecognized mtrsl content. A JSON profile alone cannot connect it.

**Design sketch only:**

1. Add an explicit Literal protocol and Literal/Alternate framing choice through profile/saved-connection schema, validation and configuration UI. Keep ASTM behavior intact.
2. Reuse port ownership/reconnect; add a bounded Literal state machine for RS/GS checksum, ACK timing, confirmed timers and configured control suffixes. Preserve exact bytes for checksum; separately reassemble content. Persist accepted data before ACK. Literal's pre-ETX ACK requires adapting the current ASTM receipt assumption; track subsequent ETX/EOT completion explicitly.
3. Add a tagged-field result parser/normalizer: ci accession, rt assay, ql call, qn quantity/text under verified generation rules. Preserve units/comparators, nc, timestamps, operator/instrument and raw evidence. Recognize QC separately; retain multiple assay components without inventing physical location.
4. Author vendor-sourced profiles for verified installed assays/locale and feed the existing normalized FHIR/outbox path. Profile policy excludes unverified codes and requires assay sources; no guessed rt entries.
5. If bidirectional use is later requested, add mtmpr serialization/outbound session scheduling with the same serial-port owner. Current ASTM outbound orders cannot simply emit this protocol. Verify rejection/duplication behavior; no host-query feature is established.

Future verification should cover pre-ETX deadlock, Alternate ACK, fragmented reads, RS inside fields, checksum/NAK/timeouts, multiple requests/results, missing ci, localized calls, invalids and QC, using synthetic messages built from the documented rules and a peer simulator. None was executed during research.

**Application boundary:** README forwards FHIR to OpenELIS `/analyzer/fhir` with connection/profile identity and pairing. Connecting this Bridge to another web app also needs a compatible receiving adapter/contract; the current app's compatibility was not established here.

**Open-source search:** recursive tree inspection of [OpenELIS plugins][plugins] (`develop`, 8faf00561ffb4d7474ef1d62cf89b74cb5e11511) and [SENAITE instruments][senaite] (`master`, fd30695868b23f03f99a9d034b8cc3c5defa9e9b) found no VIDAS/bioMérieux path. Web searches for GitHub VIDAS/Literal/mtrsl, Bika/SENAITE, hmislk and Mirth produced no verified implementing source file. **No reusable open-source parser verified, not proof none exists.** This was not an exhaustive content search of all repositories. The guide examples remain the best reference, but they stay out of the repository; fixtures must be synthetic or de-identified captures.

## 10. Open questions, confidence and sources

Ask lab / Biotek Solutions / bioMérieux:

1. Exact VIDAS PC, analyzer and BCI versions; applicable interface guide; is BCI installed/licensed?
2. Actual LIS COM port, cable P/N/full pinout/genders, serial/flow-control/modem-line settings?
3. Literal or Alternate; CR/LF options/checksum span; ACK/interbyte/session timers, retry/backoff, abort and collision behavior?
4. Actual ci/pi length and characters; typed-ID workflow, handheld reader availability, simplified demographics; any installed location-export extension?
5. Obtain deidentified raw hexadecimal uploads paired with printout/loading entries: numeric, qualitative/equivocal and flagged/invalid examples. Preserve chunks/spaces/checksums; do not send patient data to outside research services.
6. Installed assay-code list and vendor mappings for PCT, D-dimer, thyroid, hCG, TORCH, HIV, hepatitis, ferritin, troponin. Resolve ql/qn contradiction, decimal comma/dot, unit omission and dilution integer/ratio by version.
7. Upload only initially or bidirectional? How are rejected orders shown, repeats/components distinguished, IDs reused and results released for upload?
8. Is BCI NET/FTP available, with which schema/path/retention? Which revision of 161150-486 applies, and is the 4501-2122 manual available? Supervise a bench test if needed.

| Key claim                                  | Confidence                                                                             |
| ------------------------------------------ | -------------------------------------------------------------------------------------- |
| UDI = original VIDAS REF 410417            | VERIFIED vendor reference list, corroborated by a vendor notice.                       |
| PC LIS port and bidirectional capability   | VERIFIED BCI guide B §2-1 and PC manual chapter 10.                                    |
| mini upload-only versus PC                 | VERIFIED BCI guides A and B §2-3.                                                      |
| Complete cable / installed serial settings | Crossed 2/3 and published defaults VERIFIED; remaining wiring/site settings UNVERIFIED. |
| Checksum and ACK timing                    | VERIFIED vendor descriptions; optional suffix checksum treatment/timers UNVERIFIED.    |
| ci sample identity, no location field      | ci meaning VERIFIED; location absence INFERRED from complete schema; no live capture.  |
| 12 versus 17 IDs                           | Both document limits VERIFIED; this installation's rule UNVERIFIED.                    |
| Calls, units and rt                        | Table/examples VERIFIED with explicit contradictions; unlisted codes UNVERIFIED.       |
| Bridge changes needed                      | Source shapes/guards VERIFIED at pinned commit; compatibility judgment INFERRED.       |

Source notes:

- **BCI guide B**, 161150-486 rev B, 2017-01, P/N 161150-827: system layouts and connections (§2), Literal protocol (§3) and the XML/FTP interface. Field-level detail in this note comes from guide A.
- **BCI guide A**, 161150-486 rev A: main source for fields, checksum, call vocabulary and examples. Earlier revision; confirm applicability. Guide C, 2020-11, is newer; no C-specific changes assumed.
- **PC manual**, REF 93 073, rev F, 05/2008, 4501-1714-en: original VIDAS PC workflow, BCI configuration (chapter 10) and example traces.
- **Manufacturer reference list** and **manufacturer notice**: identity, not software inventory. **Assay menu**, 08/2022, 9322911 008/GB/Y: menu, not serial-code authority.
- **RFC 20**: byte encodings. **[Bridge README][readme]/[profile guide][profiles]** and linked source: static snapshot, not executed.

No live instrument access, app code, commits, pushes or branches created. Tests/builds not run for documentation-only research.

[readme]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md
[profiles]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md
[port]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialPortListener.java
[settings]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/connection/SerialConnectionSettings.java
[frames]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialFrameBuffer.java
[handler]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialMessageHandler.java
[protocol]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/model/Protocol.java
[astm]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/fhir/ASTMResultParser.java
[hl7]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/fhir/HL7ResultParser.java
[plugins]: https://github.com/DIGI-UW/openelisglobal-plugins/tree/8faf00561ffb4d7474ef1d62cf89b74cb5e11511
[senaite]: https://github.com/senaite/senaite.instruments/tree/fd30695868b23f03f99a9d034b8cc3c5defa9e9b
