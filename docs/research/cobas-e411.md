Summary: Roche cobas e 411 has disk and rack variants and an RS-232C ASTM host interface with selectable Elecsys and cobas dialects.
Summary: Roche recommends 9600 baud, 8-N-1; the cable crosses RX/TX and loops analyzer pins 7-8 locally.
Summary: Results, real-time order queries and batch orders are documented; non-barcode identification uses sequence plus carrier/position.
Summary: Host codes are configurable; reagent test numbers and newer assay generations must not be mistaken for verified site transmission codes.
Summary: Bridge serial reception looks compatible, but e411 parsing, non-barcode matching and serial orders/query replies need additional work and bench verification.

# Roche cobas e 411 - analyzer integration research

Research date: 2026-11-14. Research only; no implementation or device access. Actual lab variant, software, serial settings, barcode configuration and host-code table remain UNVERIFIED. All examples in this note are synthetic. They are our own construction and contain no patient data.

## Evidence and scope

**Verified-document** means read in a Roche-authored document, not tested on the lab's instrument. **Verified-code** means read in the upstream Bridge source, not executed. **Inferred** identifies a conclusion from these facts. **UNVERIFIED** means evidence is missing, contradictory, or version/site dependent. Field numbers below are **one-based and include the record type**: `O.3` is the third pipe-delimited field; components are also one-based. They are not zero-based array indexes.

The main protocol reference is the Roche Host Interface Manual (HIM) v2.2, November 2013. Printed sections/pages are given throughout. This is more direct evidence than borrowing an Elecsys 2010 driver. The US edition of HIM v2.3 agrees on serial settings, setup and host codes; its full tables were not compared line by line. Use the revision that matches the installed software before implementation.

## 1. Identity, variants, menu and current status

The e 411 is a fully automated random-access ECL immunoassay analyzer, succeeding Elecsys 2010. Roche lists rack material number **04775201001** and disk **04775279001**. The disk has 30 sample/control/calibrator positions. The rack uses five-position racks; the software 03-01 operator manual describes a 75-position tray plus 25-position input buffer. There are 18 reagent positions. The current Roche product page and operator manual state up to 86 tests/hour; the older brochure states 88, so do not use the brochure as the current performance specification. Roche product page; operator manual v3.2, pp.60-61.

Typical menu includes thyroid tests (TSH, FT4, FT3, T4, T3, anti-TPO), fertility/hormones (hCG, prolactin, LH, FSH, estradiol, progesterone, testosterone, cortisol, insulin), anemia/nutrition (ferritin, B12, folate, vitamin D), tumor markers (PSA, CEA, AFP, CA125, CA15-3, CA19-9), cardiac markers (troponin T, NT-proBNP), and infectious serology (HBsAg, anti-HCV, HIV combi). Exact reagent generation, sample material, approvals and availability are assay/country dependent. Menu presence does not establish which assays the lab runs. Roche menu.

**Discontinuation/service: UNVERIFIED for the Balkans installation.** Roche still publishes an e 411 product page and lists both variants at research time. That is evidence of an active public listing, not proof of continued manufacture, regional sales, spare parts or a service commitment. No authoritative global end-of-sale/end-of-service notice was found. Ask the local Roche distributor for the serial-number-specific support horizon.

## 2. Physical connection and serial settings

### Port and cable

Verified-document, HIM B-2/B-7: one half-duplex RS-232C port, a **male D-SUB9 connector labelled HOST on the left side**, maximum cable length 15 m. This is based on the connector figure and the wiring diagram. Use the HOST connector, not SERVICE or PRINTER.

| Analyzer pin / signal | Host side                                    | Evidence     |
| --------------------- | -------------------------------------------- | ------------ |
| 2 / RXD               | Goes to the host's transmit line             | HIM Fig. B-2 |
| 3 / TXD               | Goes to the host's receive line              | HIM Fig. B-2 |
| 5 / signal ground     | Goes to the host's signal ground             | HIM Fig. B-2 |
| 7 and 8               | Jumpered together on the analyzer side only  | HIM Fig. B-2 |
| 1, 4, 6, 9            | The diagram shows them unused                | HIM Fig. B-2 |

**Inferred:** for a conventional PC DTE DB9 host this means **2-3, 3-2, 5-5**, i.e. a null-modem-style cable with the analyzer-end 7-8 loop. The manual labels host signals but does not number host pins. Verify the adapter's DTE/DCE role, connector gender and continuity; an arbitrary commercial "null modem" cable may wire control pins differently.

**Flow control:** hardware RTS/CTS is not carried end to end in this diagram; analyzer pins 7-8 are looped. No XON/XOFF setting is documented. **Inferred starting configuration: NONE** at the connector PC, with ASTM ENQ/ACK providing protocol control. This is not a verified GUI factory default.

### Serial parameters

HIM B-2, Table B-1 recommends **9600 baud, 8 data bits, no parity, 1 stop bit**, and Elecsys dialect. Roche presents these as recommended values. They are not a verified factory default or the current lab setting.

Allowed baud rates: **4800, 9600, 19200**. The allowed character formats are exactly these eight:

- 7 data bits with even or odd parity, and 1 or 2 stop bits (four formats).
- 8 data bits with no parity, and 1 or 2 stop bits (two formats).
- 8 data bits with even or odd parity, and 1 stop bit only (two formats).

Both speed and character format are selectable in the GUI. HIM B-2/B-6.

### Network/converters

The operator manual separates the two-way host interface (CCITT V.24 / RS-232-C) from the ordinary PC USB, Ethernet and serial ports. Those ports serve remote access and cobas link (OP p.60). **Native ASTM-over-Ethernet LIS support is UNVERIFIED**; an Ethernet socket is not evidence of it. A USB-to-RS232 adapter on the connector computer or a transparent serial device server is a plausible external conversion (**inferred**, not a Roche-approved model recommendation). It must preserve bytes, parity and handshake timing. It does not add order-query handling or alter Roche message semantics. No converter model was bench-tested.

## 3. Low-level ASTM protocol

Verified-document: HIM A-2 and D-2-D-10 explicitly name **ASTM E1381-91 and E1394-91**. LIS1-A/LIS2-A are related later CLSI terminology; do not claim that Roche declares these newer revisions in this manual.

| Element                         | e 411 behavior                                                                                                                                                                                                                                           |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Establish                       | Sender sends ENQ `05`; ready receiver sends ACK `06`. Either side can initiate.                                                                                                                                                                          |
| Frame                           | `STX FN text ETX-or-ETB CS1 CS2 CR LF`, STX `02`, ETX `03`, ETB `17`, CR `0D`, LF `0A`.                                                                                                                                                                  |
| Number                          | ASCII `1` first, then `2-7,0,1...`; sequence restarts at a new establishment.                                                                                                                                                                            |
| Size                            | At most **240 text bytes**, **247 total frame bytes** including framing/checksum/trailer. Record CR occupies text bytes.                                                                                                                                 |
| Segmentation                    | ETB for intermediate portions, ETX for the last portion. Elecsys: no more than one record in a frame; a long record spans frames. cobas: a frame may contain multiple records.                                                                           |
| Checksum                        | Sum bytes from ASCII FN through ETX/ETB inclusive, modulo 256; encode as two uppercase hexadecimal ASCII characters. Exclude STX and trailing checksum/CR/LF.                                                                                            |
| Accept/retry                    | ACK accepts a frame; NAK `15` rejects it. Analyzer retransmits rejected frames until the error retry count reaches six, then EOT and error. Exact interpretation as initial attempt plus six retries should be confirmed against the installed revision. |
| End                             | Sender sends EOT `04`; receiver sends no EOT acknowledgment; both return idle.                                                                                                                                                                           |
| Establish busy                  | Non-ACK, normally NAK: analyzer waits 10 s and retries ENQ, up to the documented error retry limit six.                                                                                                                                                  |
| Establish silence               | After 15 s without a response, analyzer sends EOT and reports error.                                                                                                                                                                                     |
| Frame ACK timeout               | 15 s, then EOT and error.                                                                                                                                                                                                                                |
| Receiving incomplete host frame | Analyzer times out 15 s after its last ACK/NAK, discards the incomplete message and returns idle.                                                                                                                                                        |
| Simultaneous ENQ                | **Analyzer has priority.** Host stops asserting ENQ and responds ACK or NAK; analyzer waits 1 s and reissues ENQ.                                                                                                                                        |

HIM D-4 works one checksum through by hand. Our own synthetic check of the same rule: the frame `STX 1 ABC ETX FA CR LF` has checksum `FA`, because `31+41+42+43+03` hex is `FA`. This only shows the checksum rule; it is not an application message. The analyzer's 15 s timing takes precedence over generic ASTM receivers that assume 30 or 60 s.

## 4. Message records and field positions

### Record set and syntax

Verified-document, HIM E-2-E-18:

- Query: **H, Q, L**.
- Order reply or batch download: **H, P, O, L**.
- Results: **H, P, O, R, optional following C, ..., L**.
- **M records are not supported** by e 411, even though predecessor Elecsys 2010 has manufacturer-specific records. No demographic C after O is used.
- CR terminates records; `|` separates fields; `\` repeats fields/tests; `^` separates components; `&` escapes. Recognized escapes: `&F&`, `&S&`, `&R&`, `&E&`. Alarm messages are not specified as UTF-8.

### Header, patient and terminator

| Field       | Meaning / distinction                                                                                                                                                                                              |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| H.2         | Delimiter definition; conventional header begins `H\|\^&`.                                                                                                                                                         |
| H.5         | Sender. Elecsys analyzer upload normally omits it. cobas uses `cobas-e411^1` (device name/program version); this is a model/program identifier, **not an instrument serial number**.                               |
| H.10        | Receiver; cobas analyzer upload uses `host`; host downloads name `cobas-e411` as receiver.                                                                                                                         |
| H.11        | cobas message purpose: `TSREQ^REAL`, `TSDWN^REPLY`, `TSDWN^BATCH`, `RSUPL^REAL`, `RSUPL^BATCH`. Not an effective Elecsys field.                                                                                    |
| H.12 / H.13 | Processing ID `P`; cobas version `1`.                                                                                                                                                                              |
| H.14        | Standard date/time slot exists but is not effective in this manual; do not assume a timestamp is supplied there.                                                                                                   |
| P.2         | Record sequence, normally `1`. All patient demographic/ID fields are unused; do not look for patient identity in P.3-P.9.                                                                                          |
| L.2         | Record sequence, normally `1`.                                                                                                                                                                                     |
| L.3         | cobas `N` normal / `E` error. Elecsys queries use `I`; the field table also lists `F` with response data, `I` without, `Q` inapplicable, `E` error; normal non-query uploads/downloads may omit the termination code. |

### Order O and sample identity

| Field | Elecsys dialect                                                                                 | cobas dialect                                                                                |
| ----- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| O.2   | Record sequence, normally 1; **not sample sequence**                                            | Same                                                                                         |
| O.3   | Sample ID, max 22; QC control name                                                              | Sample ID, max 22; generated `@<sequence>` on barcode error; QC name or name`^`lot per setup |
| O.4.1 | Analyzer internal sample sequence, max 4 digits                                                 | Same                                                                                         |
| O.4.2 | Carrier ID (disk/rack), max 5                                                                   | Same; generated rack carrier can start `@`                                                   |
| O.4.3 | Position in carrier, max 2                                                                      | Same                                                                                         |
| O.4.4 | Empty/reserved                                                                                  | Empty/reserved                                                                               |
| O.4.5 | `SAMPLE` patient / `CONTROL` QC                                                                 | `S1` serum / `S2` urine / `S5` other / `QC` control                                          |
| O.4.6 | `NORMAL` tube/cup / `REDUCED` reduced volume                                                    | `SC` tube/cup / `MC` reduced volume, **not permission to use micro cups**                    |
| O.5   | Upload `ALL`; download repeated `^^^application^dilution`                                       | Repeated `^^^host-code^ratio`; retain trailing `^` even if ratio blank                       |
| O.6   | `R` routine/control, `S` STAT                                                                   | `R` routine, `S` STAT; not used for QC                                                       |
| O.8   | Request reception timestamp `YYYYMMDDHHMMSS`, optional                                          | Same                                                                                         |
| O.12  | Upload `X` measured, QC `X\Q`; download `N` new order / `Q` QC; table also lists `A` additional | Upload `N` patient / `Q` QC; download `A`                                                    |
| O.16  | Not effective                                                                                   | Specimen descriptor: `1` serum, `2` urine, `5` other; required in table                      |
| O.23  | Not effective                                                                                   | Date/time when all test results collected; analyzer-originated, host cannot set              |
| O.26  | Upload `O`; download reply `Q`, no-order reply `Z`                                              | Upload `F`; download `O`                                                                     |

Sources: HIM E-11-E-14. **Manual inconsistency:** some Elecsys example strings in the manual put the request timestamp in O.7, while the field table marks O.8 effective and O.7 unused. Preserve captures and confirm this with Roche; do not silently choose one. For cobas, the table says O.16 takes precedence when its sample type disagrees with the instrument-specimen sample type (except QC). Query sample-type behavior also changes by software, described below.

### Result R and alarm C

| Field             | Elecsys dialect                                                                                                   | cobas dialect                                                                               |
| ----------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| R.2               | Result record sequence                                                                                            | Same                                                                                        |
| R.3               | `^^^application^dilution-index^predilution`                                                                       | `^^^host-code/ratio/predilution` - **slashes inside component 4**, not caret components 5/6 |
| R.3 code          | Application number; in the older numbering the last digit marks the generation                                    | Configured host test number; may also identify an analyzer-calculated test                  |
| R.3 predilution   | `0` no / `1` yes                                                                                                  | `not` / `pre-diluted`                                                                       |
| R.4               | Quantitative number; qualitative `call^COI`                                                                       | Quantitative `number^message-code`; qualitative `call^COI`                                  |
| R.4.1 qualitative | `1` positive, `0` borderline, `-1` negative                                                                       | Same codes; the manual words them as reactive/borderline/nonreactive                        |
| R.4.2             | Qualitative cutoff index                                                                                          | Qualitative cutoff index or quantitative result-message code 0-31                           |
| R.5               | Units, max 6 characters                                                                                           | Same                                                                                        |
| R.6               | `low^high` reference interval, QC interval for QC                                                                 | Not effective/blank                                                                         |
| R.7               | `L`, `H`, `<`, `>`, `N`, `A`                                                                                      | `L`, `H`, `LL`, `HH`, `N`, `A`; preserve dialect-specific flags                             |
| R.9               | `F` last result, `X` cannot be done, `R` communicated, `V` released by user, `Y` system-blocked, `+` user-blocked | `F` initial / `C` rerun - **F is not defined here as clinical approval**                    |
| R.11              | Not effective                                                                                                     | Operator ID, max 6                                                                          |
| R.12 / R.13       | Measurement start/completion, `YYYYMMDDHHMMSS`                                                                    | Not effective; use O.23 only as a sample-level time, not per-test completion                |
| R.14              | No effective instrument ID documented                                                                             | Fixed `E1`, not a unique serial number                                                      |
| C.2 / C.3 / C.5   | Sequence / fixed `I` / fixed `I`                                                                                  | Same                                                                                        |
| C.4               | `alarm-number^alarm-message`                                                                                      | Alarm number only                                                                           |

Quantitative results may begin `<` or `>`; keep the comparator. Seven spaces mean no result, **not zero**. Qualitative `-1` is a call, not a negative numeric concentration. Sources: HIM E-15-E-19.

HIM E-18 shows one C record per dialect. Structure in our own notation: Elecsys `C|1|I|<alarm-number>^<alarm-text>|I`, cobas `C|1|I|<alarm-number>|I`. Both E-18 examples use alarm number 50 for a below-measuring-range result. **The cobas example conflicts with the alarm table:** E-19 assigns below-measuring-range code **27** to cobas and **50** to Elecsys. Other examples in the manual also use Elecsys alarm numbers in cobas messages. This must be resolved using the installed manual/captures; do not deploy one shared alarm-number dictionary.

Selected alarm codes from table E-19, **checked as table entries, not validated against a device** (descriptions in our own words):

| Condition (our wording)                      | Elecsys host number | cobas host number |
| -------------------------------------------- | ------------------- | ----------------- |
| Not enough sample / no sample found          | 7 / 53              | 3 / 3             |
| Not enough reagent                           | 8                   | 4                 |
| Calibration missing                          | 25                  | 43                |
| Clot detected in sample                      | 45                  | 72                |
| Run cancelled because of a sample ID error   | 47                  | 121               |
| Result under / over the reference range      | 48 / 49             | 41 / 40           |
| Result under / over the measuring range      | 50 / 51             | 27 / 26           |
| Reagent pack past its expiry date            | 52                  | 101               |

C follows the R it annotates and is absent when there is no data alarm. Do not treat missing C as proof that a result was approved: the cobas Review by Exception option can suppress some result records entirely. Patient and QC results upload; **calibration results do not** (HIM C-2). QC identification is O.4.5 and O.12, not merely a control-looking sample name. No calibrator upload marker is defined because those results are excluded.

### Dilutions, generations and reruns

Elecsys dilution **index** `0,1,2,3,5,7,9` means actual ratio `1,2,5,10,20,50,100`, respectively. cobas sends the actual ratio (`1,2,5,10,20,50,100`), or blank for no dilution. Thus `^2` in an order means 1:5 for Elecsys and 1:2 for cobas. R has an additional predilution marker; never derive a new concentration by multiplying without confirming how the analyzer reports the final value. HIM E-12-E-16.

HIM v2.2 says reruns are defined by hand and the analyzer has no automatic rerun function; the current Roche product page advertises automated sample rerun capability. **Automatic rerun behavior is version dependent/UNVERIFIED**. cobas R.9=`C` explicitly marks a rerun. Elecsys R.9 does not supply an equivalent unambiguous rerun flag in this table. Starting with software 02-04, multiple orders for the same assay within one sample record are not allowed. A new order replaces existing test-selection information rather than providing a verified additive patch (HIM E-13).

## 5. Directions, modes and query lifecycle

### Result uploads

Both dialects support real-time and batch patient/QC result upload. Automatic Result Upload sends results when a sample's results are collected. STAT Sample Result Upload Setting selects By Sample or By Test. Batch result upload is selected on **Workplace/Data Review -> Send to Host**. That dialog has an option to include results held back by Review by Exception, and a way to mark results as not to be sent. Review by Exception suppression is specific to cobas dialect. There is **no supported host inquiry for stored results** in HIM C-2.

### Real-time host query

The analyzer asks for tests when it reads a patient sample identity for which no selections are registered. QC does not generate order queries. The application exchange is analyzer H/Q/L followed by a separate host H/P/O/L message; transport ACK alone is not an order reply.

| Field                          | Elecsys Q                                                                  | cobas Q                                                                    |
| ------------------------------ | -------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| Q.2                            | Record sequence, normally 1                                                | Same                                                                       |
| Q.3                            | `^sampleID^sequence^carrier^position^^SAMPLE^NORMAL`                       | `^^sampleID^sequence^carrier^position^^sampleType^container`               |
| Component positions within Q.3 | Sample ID 2, sequence 3, carrier 4, position 5, sample type 7, container 8 | Sample ID 3, sequence 4, carrier 5, position 6, sample type 8, container 9 |
| Q.5                            | `ALL`                                                                      | `ALL`                                                                      |
| Q.13                           | `O` order inquiry / `A` cancel previous inquiry                            | Same                                                                       |

Q sample ID max 22, sequence max 4 digits, carrier max 5, position max 2. A note on HIM v2.2 E-9 says a later software revision would send **S0** in place of `S1/S2/S5` in queries, because the analyzer cannot tell rack sample types apart; the host then gives the correct type in its order. **Which installed release emits S0 is UNVERIFIED.**

The host must echo **sequence, carrier and position unchanged** (HIM F-2). The lab order belongs to a specimen/sample identity, not the P-record sequence. For cobas use H.11=`TSDWN^REPLY`, O.12=`A`, O.16 actual sample type and O.26=`O`. For Elecsys use O.12=`N`, O.26=`Q` when returning selections.

**Reply deadline:** F-2/F-3 specify 15 s, after which the analyzer sends a cancellation Q.13=`A`. For racks, F-5 adds that the analyzer cancels when rack communication has not finished within about 40 s. These are application timing constraints in addition to link timeouts; do not rely on the larger rack deadline for an individual query.

**No orders:** respond with a matching H/P/O/L and empty O.5. Elecsys sets O.26=`Z`; cobas retains O.12=`A`, O.26=`O`, ending L.3=`N`. An unstructured error or silence makes the analyzer wait and cancel. The Bridge's generic H/L no-information reply is not the e411 reply Roche describes.

### Batch download and cancellation

The host may download patient orders before sample scanning, in either dialect. cobas H.11=`TSDWN^BATCH`. In F-9 the batch O record carries the sample ID in O.3; in O.4 only sample type and container are set, and the analyzer assigns the sequence. The manual covers new orders only. The analyzer stores up to 2,000 tests and overwrites the oldest once full. This is not permission to pre-download orders without considering on-instrument capacity.

**Order cancellation: UNVERIFIED/not documented as a distinct operation.** Q.13=`A` cancels an outstanding **query**, not a test order. Do not assume generic ASTM O.12=`C` or `X` deletes orders; Roche defines its own action values. Replacing selections is documented, but an empty replacement as a reliable delete-all operation is UNVERIFIED. Ask Roche how to cancel a pending sample/test and how cancellation behaves once aspiration starts.

## 6. Sample identification and barcode-free operation

### Readers and barcode constraints

Both disk and rack systems have built-in sample barcode readers; the rack also has rack identification. Patient linear barcode types documented in Roche material are **Code 128, Code 39, NW7/Codabar, Interleaved 2 of 5 (ITF)**. OP p.61 also lists **PDF417** in a system-wide symbology list; reagent/calibrator/control cards use 2D barcodes. **Patient-tube PDF417 support is UNVERIFIED**; do not infer it from the system-wide list.

HIM permits max **22 characters in the host sample-ID field**. This is not evidence that the sample reader accepts a 22-character printed barcode. The older Roche Service Manual v1.0, May 2006, section 1.2.1.9, Tables 1.2-1/2, gives reader limits for the legacy software. Summary in our own words:

| Symbology | Length limits (legacy)                                                                    | Characters and check digit (legacy)                                                                                                     |
| --------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| NW7       | 3 to 13 characters with no check digit; exactly 6 plus a check digit when one is used     | Digits only with no check digit; with a check digit, digits plus `- / . $ : +`                                                          |
| Code 39   | 3 to 13 characters, with or without one extra check character                             | Digits, capital letters, space and `- / . + $ %`; the check uses modulus 43                                                             |
| ITF       | 4 to 12 digits with no check digit; the same table also gives 3 to 13 plus a check digit  | Digits only; total length must be even; the check uses modulus 10 with alternating weights                                              |
| Code 128  | Given for code set C: 4 to 12 plus two check characters                                   | Letters and digits, but not function or set-switch characters; the check uses modulus 103. **The set C limits and the letter/digit description disagree and need confirmation.** |

These are manufacturer-authored legacy tables, not a current-software guarantee. Current min/max lengths, check-digit settings, Code 128 A/B/C handling and allowable manually entered IDs are **UNVERIFIED**. Do not impose the legacy limits on the web app or print labels from them until Roche confirms the actual reader configuration. Avoid raw ASTM delimiter characters in sample IDs unless the complete escape/reader path is verified.

OP pp.160-161 gives a 5 mm minimum quiet zone at both ends, barcode zone up to 50 mm for 100 mm tubes or 38 mm for 75 mm tubes, label width at most 16 mm and barcode width at most 12 mm, with placement diagrams and at least 12 mm upper margin. Take the direction of each width from the diagram; do not guess it from the text. Labels should lie flat, stick firmly and face the reader. Accepted primary tubes are 13/16 mm diameter x 75/100 mm height; use appropriate adapters, especially 13 mm tubes. Roche-certified sample cups are supported; micro cups are not (OP pp.61,160-167). Verify exact cup/tube/adaptor part numbers locally.

### WITHOUT sample barcodes - critical behavior

**Supported, including an interfaced workflow.** OP pp.152-153 covers the host entering tests, the analyzer assigning a sequence, and the disk/rack ID plus position being included. OP pp.173-175 tells staff to load samples at the programmed positions; on the disk, a stop barcode goes in the next free position. Barcode-free patient tubes do not mean an entirely barcode-free analyzer: rack IDs, stop barcodes and reagent/control barcodes still have roles.

| Situation                                    | Identity available to the host                                                              | Consequence                                                                                                                         |
| -------------------------------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Programmed sample without printed barcode    | O.3 may contain a manually/host-programmed sample ID; O.4 carries sequence/carrier/position | Preserve both identity forms; no need to invent an accession from a record counter. Exact O.3 content depends on workflow/software. |
| Rack query without sample ID, Elecsys        | Q.3 sample component empty; sequence + rack number + position supplied                      | HIM F-4 states that **rack number and position are the key**; the query happens after the rack ID is read.                          |
| Same, cobas                                  | Generated sample ID `@<sequence>`, plus sequence, rack and position                         | HIM F-5 shows this query; the generated `@<sequence>` ID is an analyzer-local fallback, not a durable lab specimen ID.              |
| Barcode failure result, cobas                | O.3=`@<sequence>`; O.4 still carries sequence/rack/disk/position                            | Matching requires retained loading/position context; do not automatically match a patient's real ID to a generated `@` ID.          |
| Both sample ID missing and rack ID generated | Neither key is reliable                                                                     | HIM warns the host cannot identify the sample. Hold for operator resolution.                                                        |
| Disk without barcodes                        | Disk ID/position and sequence from programming                                              | Interfaced operation is documented; exact empty-ID real-time query on disk is **UNVERIFIED** (the manual's disk queries carry IDs). |

**Rack query without sample ID, cobas (HIM F-5).** Structure in our own words: Q.3 holds the generated ID `@<sequence>` in component 3, then the sequence, rack ID and position in components 4-6, then sample type and container in components 8-9. Q.5 is `ALL` and Q.13 is `O`. A synthetic example of ours, with made-up values:

```text
Q|1|^^@58^58^0731^4^^S1^SC||ALL||||||||O
```

The response must keep sequence 58, carrier `0731` and position 4 unchanged. For barcode-free results, the payload's specimen context is in the **preceding O record**, not in each R. Elecsys results without an ID may have an empty O.3; cobas may generate `@sequence`. Never confuse O.2=`1` with O.4.1 sample sequence, or reuse a position indefinitely as a globally unique specimen identity. A loading assignment scoped to instrument/run is needed (**inferred integration requirement**, not a proposed app implementation).

## 7. Test codes and LOINC

### Three code concepts must stay separate

1. **Reagent/application test number:** installed by reagent barcode; generation-specific. Elecsys uses the application number in its documented O/R identifiers.
2. **User test number / Host Test Code table:** lab configurable. HIM B-5/B-6 requires host codes to be entered in **both dialects** for result upload. cobas O/R uses the configured host test number; multiple assay generations can share one host code and the newest generation onboard wins.
3. **LOINC:** a separate semantic mapping based on analyte, specimen, property, units and qualitative/quantitative method. It is not the serial host code or reagent REF/product number.

HIM E-13 explicitly gives **TSH application 10 and generation 11**. A download for 10 can yield a result for 11 when only that generation is onboard. This is not evidence that modern TSH kits always transmit 10.

### Known manufacturer test numbers - NOT verified site host-code defaults

Every numeric row below is a Roche **e411 method-sheet test number**, except the explicitly labeled legacy host-manual TSH entry. None establishes the Balkan lab's configured cobas transmission code. Each number comes from the System information section of the method sheet, normally p.1. Preserve leading zeroes in the evidence. Newer four-digit numbers conflict with the old HIM's three-digit maximum: **their on-wire representation and current field limit are UNVERIFIED; never truncate them or substitute the e601/e602 application code.**

| Test / generation                    | Published e411 test number | Evidence / confidence                                                            |
| ------------------------------------ | -------------------------- | -------------------------------------------------------------------------------- |
| TSH, legacy                          | 10 / 11                    | HIM E-13; verified protocol entry                                                |
| TSH, newer kit                       | 1820                       | 08429324500 v3; verified method sheet                                            |
| FT4 IV                               | 2210                       | 09043276500 v2; verified method sheet                                            |
| FT3 III                              | 2450                       | 09005803500; verified method sheet                                               |
| Total T4                             | 021                        | 09007741500 v4; verified method sheet                                            |
| Total T3                             | 050                        | 09007725500 v4; verified method sheet                                            |
| Anti-TPO                             | 720                        | 06368590500 v11; verified method sheet                                           |
| Ferritin                             | 381                        | 04491785500 v14; verified method sheet                                           |
| Vitamin D, older / total III         | 990 / 2200                 | older sheet / 09038078500 v4; verified method sheets                             |
| Vitamin B12 II                       | 1410                       | 07212771 USA v3; verified method sheet; Active B12 is a different assay          |
| Folate III                           | 1520                       | 07559992500 v7; verified method sheet                                            |
| Total PSA, newer kit                 | 2120                       | 08791686500 v3; verified method sheet                                            |
| Free PSA                             | 2140                       | 08828601500 v4; verified method sheet                                            |
| CEA                                  | 301                        | 11731629 v26; verified method sheet                                              |
| AFP, older / newer                   | 311 / 1810                 | 04481798 / 09015060; verified method sheets                                      |
| CA125 II                             | 341                        | 11776223500 v5; verified method sheet                                            |
| CA15-3 II                            | 332                        | 03045838500 v24; verified method sheet                                           |
| CA19-9                               | 351                        | 11776193500 v27; verified method sheet                                           |
| hCG+beta (intact plus free beta)     | 761                        | 03271749500 v20; verified method sheet; not equivalent to a free-beta-only assay |
| hCG STAT                             | 2310                       | 08890587500 v4; verified method sheet                                            |
| Prolactin II                         | 131                        | 03203093500; verified method sheet                                               |
| LH                                   | 140                        | 11732234500 v27; verified method sheet                                           |
| FSH, newer kit                       | 2350                       | 08932352500 v4; verified method sheet                                            |
| Estradiol III                        | 1370                       | 06656021500 v9; verified method sheet                                            |
| Progesterone III                     | 1380                       | 07092539500 v6; verified method sheet                                            |
| Testosterone II                      | 111                        | 08946353500 v2; verified method sheet                                            |
| Cortisol II                          | 1280                       | 06687733500 v8; verified method sheet; urine generation differs                  |
| Insulin                              | 650                        | 12017547500; verified method sheet                                               |
| Troponin T hs                        | 2590                       | 09315322500; verified method sheet; older/STAT generations differ                |
| NT-proBNP / proBNP II, older / newer | 271 / 2570                 | 04842464 / 09315268500; verified method sheets                                   |
| HBsAg II, newer kit                  | 2080                       | 08814856500 v5; verified method sheet                                            |
| Anti-HCV II, older kit               | 1020                       | 06368921500 v9; verified method sheet; newer kit number differs                  |
| HIV combi PT, newer kit              | 2250                       | 08924163; verified method sheet; earlier generation differs                      |

Do not label the numbers above "known default cobas host codes." **No universal default host-code list was verified.** Photograph/export the instrument's complete Host Test Code and Application/Range screens, together with kit REFs and active units. The Application -> Range section of the older User Training Guide says the User Test No starts out equal to the reference Test No. This does not override the dedicated host mapping table or establish the current installation's values.

### LIVD / LOINC availability

CDC LIVD resources provide disease-focused vendor-to-LOINC/SNOMED mappings, including HIV and SARS-CoV-2 files. HL7's Roche quantitative LIVD example demonstrates the format but is explicitly a demo resource; it is not a complete validated e411 catalog. **A comprehensive current Roche e411 LIVD file covering this menu was not verified.** Ask Roche for an instrument/assay-generation-specific mapping export and use Regenstrief mapping guidance to validate specimen, quantity versus call, units and assay method. No speculative LOINCs are supplied here.

## 8. Annotated sessions

The messages below are **our own synthetic examples**. They are built from the field definitions in HIM sections E and F, with made-up sample IDs, positions, host codes, values and times. They are not wire captures, they do not copy the manual's examples, and they do not establish assay codes for the lab. Literal `<CR>` is byte `0D`; notation `<STX>` etc. denotes control bytes. No spaces or display newlines should be inserted in the payload. Checksums in the first session are calculated from these exact synthetic strings.

### Full cobas-dialect result upload session

Synthetic values: sample `SYN0042`, sequence 17, rack `0412`, position 3, serum, made-up host code 25, value 3.70. O.23 is sample-level completion; R.14 is fixed `E1`.

```text
Analyzer -> Host: <ENQ>
Host -> Analyzer: <ACK>
Analyzer -> Host: <STX>1H|\^&|||cobas-e411^1|||||host|RSUPL^REAL|P|1<CR><ETX>4F<CR><LF>
Host -> Analyzer: <ACK>
Analyzer -> Host: <STX>2P|1<CR><ETX>3F<CR><LF>
Host -> Analyzer: <ACK>
Analyzer -> Host: <STX>3O|1|SYN0042|17^0412^3^^S1^SC|^^^25^|R||||||N||||1|||||||20260301093015|||F<CR><ETX>A7<CR><LF>
Host -> Analyzer: <ACK>
Analyzer -> Host: <STX>4R|1|^^^25//not|3.70^|ng/ml||N||F||op01|||E1<CR><ETX>90<CR><LF>
Host -> Analyzer: <ACK>
Analyzer -> Host: <STX>5L|1|N<CR><ETX>08<CR><LF>
Host -> Analyzer: <ACK>
Analyzer -> Host: <EOT>
```

This uses one record per frame (allowed for cobas as well as Elecsys); each frame is below 247 bytes. The unit spelling `ng/ml` is synthetic ASCII, not a normalized UCUM claim. For an alarm, insert its C record after R and renumber subsequent frames. For Elecsys, use its header/O/R fields, retain R.6 and R.12/R.13, and omit cobas-only metadata. Do not copy the slash-form test ID into an Elecsys profile.

### Host query and positive reply

The application bodies below follow the query and reply structure in HIM F-3, with one made-up host code 25 and the same synthetic sample. Each body is exchanged as its own ASTM ENQ -> ACK -> numbered/checked frames with per-frame ACK -> EOT session, as above. On the reply session, **the host becomes the sender**; it must send ENQ and await the analyzer's ACK. Echo the query coordinates exactly and finish within the query deadline.

Analyzer query body:

```text
H|\^&|||cobas-e411^1|||||host|TSREQ^REAL|P|1<CR>
Q|1|^^SYN0042^17^0412^3^^S1^SC||ALL||||||||O<CR>
L|1|N<CR>
```

Host reply body:

```text
H|\^&|||host^1|||||cobas-e411|TSDWN^REPLY|P|1<CR>
P|1<CR>
O|1|SYN0042|17^0412^3^^S1^SC|^^^25^|R||||||A||||1||||||||||O<CR>
L|1|N<CR>
```

For a no-order reply, use the same host message with O.5 empty:

```text
O|1|SYN0042|17^0412^3^^S1^SC||R||||||A||||1||||||||||O<CR>
```

This is a no-order response, not an order-cancel command. If there is no reply, the analyzer's cancellation has the same Q identity and Q.13=`A`; do not answer that cancellation with another order.

### Batch order download

Synthetic body built from the HIM F-9 fields, sent before sample `SYN0057` is scanned:

```text
H|\^&|||host^1|||||cobas-e411|TSDWN^BATCH|P|1<CR>
P|1<CR>
O|1|SYN0057|^^^^S1^SC|^^^25^\^^^31^2\^^^44^|R||||||A||||1||||||||||O<CR>
L|1|N<CR>
```

Wrap this in a host-initiated ASTM session with ENQ/ACK and checksummed frames. The analyzer assigns a sequence for a new batch order. Codes 25/31/44 are made up, not verified identities for three lab assays. In cobas dialect the second requested test has ratio 2. **F-9 puts `/` between requested tests, whereas E-4/E-12 and F-3 define/use `\`**; this example uses the defined repetition delimiter and calls out that source inconsistency rather than copying it silently.

## 9. Analyzer-side setup and access

Documented paths/settings (HIM B-3-B-6; OP orders/results):

1. Record software release, variant, existing communication parameters and mapping table before changing anything.
2. In **Start Conditions -> Host Communication**, set Off before changing parameters.
3. Open **Utility -> System -> Host Communication Setting**. Select Elecsys or cobas, baud, character format, Automatic Result Upload, Communication Trace, and STAT upload By Sample/By Test.
4. Open **Host Test Code** in that dialog. Enter all used applications/host codes; add new applications when introduced. Both dialects require entries to upload results.
5. In cobas settings, **Send Results Only Mode = Yes** suppresses order inquiry; **No** allows the documented query workflow. It does not disable batch transfer. Automatic Recovery of Session and QC Additional Information are also selectable. The manual does not establish their factory defaults.
6. Confirm order selection on **Workplace -> Test Selection**; verify sample IDs, disk/rack ID and positions for non-barcode workflows. Enable Host Communication and confirm the Host status indicator.
7. Use **Data Review -> Send to Host** for selected batch uploads; explicitly inspect exception-result filtering. For initial result-only bench testing, configure results-only behavior deliberately rather than leaving unanswered queries enabled.

The manual documents GUI controls; it does **not** prove that the lab's ordinary operator account may change all of them. The manual's screen images show a service-level user. **Mandatory engineer involvement, licensing/activation fees and operator permissions are UNVERIFIED.** Ask Roche/local service, and use only access that Roche service grants. No analyzer settings were changed during this research.

Per HIM A-2, Communication Trace writes to `d:\e411\data\log\host_trace.log` on the analyzer. Ask for a local redacted/de-identified capture covering patient, QC, abnormal/no-result, rerun, dilution and barcode-free examples. Trace export is evidence about the site, not a replacement for the manufacturer specification.

## 10. Fit with OpenELIS Analyzer Bridge

### Upstream snapshot examined

Read upstream **develop**, resolved to commit **5a3b8a361f873af566bfaae8a549d3b80a4d958e** at research time: [README][BRREADME], [docs/profile-authoring.md][BRAUTHOR], the five shipped profiles under [src/main/resources/analyzer-profiles][BRPROFILES], the ASTM template/schema, and serial/parser/query/order source files. The default branch is develop.

Shipped profiles examined: `cepheid-genexpert-astm.json`, `mindray-bc5380.json`, `hain-fluorocycler-xt.json`, `hain-fluorocycler-xt-madagascar.json`, `thermo-quantstudio.json`. **No e411/Elecsys profile is shipped in this snapshot.** GeneXpert shows ASTM `resultParts`, control recognition and connection configuration, but its assay/subcomponent definitions must not be reused as e411 semantics.

### Can serial ASTM receive results as-is?

**Transport: likely yes, inferred from verified code; device interoperability is UNVERIFIED.** [SerialFrameBuffer][BRBUFFER] receives ENQ, emits ACK, checks FN/checksum, supports ETB/ETX, assembles text until EOT and acknowledges identical retransmitted frames without duplicating content. [SerialMessageHandler][BRHANDLER] supplies ASTM/serial envelopes to the normalizer. [SerialPortListener][BRLISTENER] supports configurable serial parameters. This matches the documented e411 wire envelope; a proprietary physical transport driver does not appear necessary. Match 9600/8-N-1 only if the actual analyzer uses those settings, and configure timely ACKs under load.

**End-to-end as-is: no verified claim.** A model/dialect profile plus parsing checks and site mappings are needed. Raw reception is not proof that patient, QC, qualitative calls or positional identities are correctly normalized.

| Gap                                | Evidence and implication                                                                                                                                                                                                                          |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cobas slash test IDs               | [AstmResultParts][BRPARTS] selects caret-separated components only. `R.3.4` returns e.g. `25//not`, not `25`; no slash selector is documented. A verified transform/parser extension would be needed; a JSON field path alone does not solve this. |
| Mixed quantitative/qualitative R.4 | Quantitative is value-first, qualitative is call-first with COI second. A single fixed `number`/`call` field map is insufficient without assay-aware handling; otherwise `-1` could become a concentration or a COI could become the main result. |
| Reference range representation     | [ASTMResultParser][BRPARSER] reads structured range limits by splitting on `to`; e411 Elecsys sends `low^high`. Preserve raw interval and verify structured range handling.                                                                       |
| Sample context without barcodes    | Parser extracts accession from O.3, first caret component, not O.4 coordinates. Empty/generated IDs require explicit matching/hold behavior. A profile does not create a run/position assignment mechanism.                                       |
| cobas timestamps                   | Profile result parts read R paths for completion; e411 uses O.23 sample-level completion. Do not manufacture R timestamps or call O.23 per-test time.                                                                                             |
| QC                                 | Add dialect-specific recognition: cobas O.12=`Q` or O.4.5=`QC`; Elecsys O.4.5=`CONTROL` / O.12 repetition `X\Q`. A simple exact O.12=`Q` rule alone misses Elecsys QC.                                                                            |
| Status and alarm semantics         | Preserve Roche R.9, R.7 and C.4. Confirm rerun/status treatment and the conflicting cobas alarm numbers; generic "F means final/approved" is unsuitable.                                                                                          |
| Clinical destination               | Bridge normalizes/forwards FHIR to OpenELIS `/analyzer/fhir`. A different lab web app must implement a compatible receiving boundary or an explicitly designed adapter; a profile does not provide that integration.                              |

### Queries and outbound orders

The README and [ASTMBridgeAdapter][BRADAPTER] describe **TCP** query handling. [AstmHostQuery][BRQUERY] builds a generic H/L **no-orders** reply (`L|1|I`), and suppresses replies to Q.13=`A`. It does not retrieve orders and does not construct Roche's H/P/O/L `TSDWN^REPLY`. Its generic comment mentions a 60 s wait; e411's documented query deadline is 15 s.

The inspected serial handler normalizes incoming messages and emits link responses; **no serial host-order reply sender was found in this path**. [OutboundAstmClient][BROUTBOUND] explicitly uses a **TCP socket**, not the managed serial port. It also assumes host priority on ENQ contention, opposite the e411 rule. Therefore:

- Serial result reception is a plausible first use, with queries disabled on the analyzer.
- Answering e411 queries with real selections, or even Roche-format no-orders replies over serial, is **missing in the inspected serial path**.
- Serial batch order transmission is **missing in that path**; generic outbound TCP orders do not establish serial support or e411-specific message generation.
- A serial-to-Ethernet converter does not fix message shape, pending-order availability, contention priority or the query deadline. TCP behavior would still require an e411-specific assessment.

### Profile sketch - research fragment, not deployable JSON

Separate dialect definitions are advisable (**inferred**), because test IDs, status, ranges, QC markers and timestamps differ. A conservative Elecsys quantitative-results fragment using real Bridge field names is:

```json
{
  "schemaVersion": "2.0",
  "profileMeta": {
    "id": "site.roche-cobas-e411-elecsys-research",
    "version": "0.0.1",
    "displayName": "Roche cobas e 411 - Elecsys dialect (research)",
    "confidence": "LOW"
  },
  "analyzer_name": "cobas e 411",
  "manufacturer": "Roche Diagnostics",
  "protocol": {
    "name": "ASTM",
    "version": "E1394-91",
    "lowerLayerVersion": "LIS01_A"
  },
  "transport": ["RS-232"],
  "transport_config": {
    "RS-232": {
      "default_baud_rate": 9600,
      "data_bits": 8,
      "stop_bits": 1,
      "parity": "NONE",
      "flow_control": "NONE",
      "read_timeout_ms": 1000,
      "message_timeout_ms": 15000
    }
  },
  "capabilities": { "inboundResults": true, "outboundOrders": false },
  "default_test_mappings": [],
  "configDefaults": {
    "dataFlow": "RESULTS_ONLY",
    "transport": "RS-232",
    "aggregationMode": "PER_MESSAGE",
    "numberFormat": ".",
    "extractionOverrides": {
      "resultRecordSelection": { "mode": "ALL" },
      "resultParts": {
        "testCode": "R.3.4",
        "number": "R.4.1",
        "unit": "R.5",
        "range": "R.6",
        "flag": "R.7",
        "status": "R.9",
        "started": "R.12",
        "completed": "R.13",
        "note": "C.4"
      }
    }
  },
  "controlResultRecognition": {
    "mode": "RULES",
    "rules": {
      "e411-elecsys-control": {
        "ruleType": "FIELD_EQUALS",
        "targetField": "O.4.5",
        "operand": "CONTROL"
      }
    }
  }
}
```

This intentionally incomplete sketch has not been schema-validated or run. `lowerLayerVersion=LIS01_A` names the Bridge engine, not a Roche declaration of the newer standard. Read/message timeout values are connector policy suggestions, not analyzer defaults; whole-message timeout behavior must be measured because a legal multi-frame session can exceed 15 s overall. Add required identity/category/communication/connection fields and serial reconnect/control-line settings from the current template after verification. Do not claim connection-test support without testing it.

A complete profile would also declare verified assay mappings, result types, units, categorical values `1/0/-1`, vendor source sections, approved standard codings, per-site code overrides and run-failure behavior. Empty `default_test_mappings` here avoids invented LOINCs. Profile-authoring requires standard codings for shipped assays and says unverified facts stay out of the profile. Elecsys uploads lack a reliable sender ID: bind identity to the serial connection, not an invented header regex. cobas variant needs slash parsing, assay-aware R.4 handling, O.23 preservation and QC rules before the analogous sketch could be safe to use.

### Other open-source leads checked

- [senaite/senaite.astm][SENAITE], default branch `2.x`: inspected tree exposes `roche_cobas_c111.py`, `roche_cobas_c311.py` and corresponding fixtures. No e411/Elecsys-named driver was found. Useful generic ASTM/Roche context, **not verified e411 support**. c111/c311 are different analyzers, and c111 manufacturer records must not be imported into e411.
- [hmislk/limsmw][HMIS], master: inspected tree, README, Analyzer and SerialCommHandler. No e411/Elecsys match in these inspected files/tree names. Generic serial middleware is not evidence of an implemented e411 driver. Other handlers/forks may contain work; **not exhaustively ruled out**. Repository log files were not used as patient/test-code evidence.
- Elecsys Host Interface Manual v4.2, January 2005, is Roche-authored. It includes 2010/1010 and manufacturer-specific records. Use it for lineage only where the e411 manual does not override it. It is not an interchangeable e411 specification.

## 11. Questions for the lab and Roche

1. Disk or rack? Photograph model plate/HOST socket and report software release, installed host-manual revision, current maintenance contract and regional service/end-of-support dates.
2. Which dialect is selected, and what are actual baud, character format, Automatic Result Upload, Send Results Only, STAT upload and exception-filter settings? Who can change them; does service need to enable/license host communication?
3. What exact cable/adapter is installed? Confirm analyzer-end 7-8 loop and host pinout. Is a converter Roche-approved, and how is its byte/timing configuration specified?
4. Are patient tubes barcoded today? Which symbology, length, check digit and characters? Confirm current reader limits, Code 128 sets and PDF417 scope. Obtain the actual label/tube/adaptor specification.
5. If no barcodes, how are sample IDs entered, sequences assigned, positions loaded and racks/disks reused? Can staff provide a de-identified barcode-free query/result pair? What happens when both sample and rack barcode reads fail?
6. Export every active Application/User Test No/Host Test Code mapping, assay kit REF/generation, unit and dilution setting. How do current four-digit test numbers appear on the wire, given the older manual's three-digit table?
7. Confirm current S0 query behavior; exact no-order response; per-query/rack deadlines; order replacement rules; and an explicitly supported cancel operation, including after aspiration.
8. Resolve alarm-table versus example code differences, timestamp O.7/O.8 discrepancy, and F-9 repetition delimiter. Obtain the matching protocol errata.
9. Confirm rerun/automatic rerun behavior and duplicates: how are reruns, manual resends, predilutions, qualitative COIs, no-result spaces, blocked results and QC lots represented? Which outputs are suppressed?
10. Request a current Roche LIVD/LOINC mapping package for the actual assay generations and specimen types. Obtain protocol conformance material and an agreed bench replay covering both patient and QC data.

## 12. Sources and confidence ledger

| Key claim                                 | Source / locator                                            | Confidence                                                          |
| ----------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------- |
| Variant identity/current public listing   | Roche product page, ordering information                    | Verified-document; local sales/service horizon UNVERIFIED           |
| Serial choices and recommendation         | HIM v2.2 B-2/B-6; HIM v2.3 corroboration                    | Verified-document; factory/site defaults UNVERIFIED                 |
| Connector and pinout                      | HIM B-7 Fig. B-1/B-2                                        | Verified-document; PC pin numbers and NONE flow control inferred    |
| ASTM frame/checksum/priority/timers       | HIM D-2-D-10                                                | Verified-document; not bench-tested                                 |
| Record positions and dialect distinctions | HIM E-2-E-19                                                | Verified-document; identified table/example conflicts unresolved    |
| Non-barcode rack queries                  | HIM F-4/F-5                                                 | Verified-document, explicit example; disk empty-ID query UNVERIFIED |
| Interfaced non-barcode loading            | OP v3.2 pp.152-153,173-175                                  | Verified-document; installed workflow UNVERIFIED                    |
| Reader types/legacy limits                | OP p.61; Service Manual section 1.2.1.9                     | Verified-document legacy data; current lengths/settings UNVERIFIED  |
| Host mapping and generations              | HIM B-5/B-6/E-13                                            | Verified-document; installed host codes UNVERIFIED                  |
| Assay reference test numbers              | Roche method sheets, System information                     | Verified-document; transmission-code equivalence UNVERIFIED         |
| Bridge receiver/parser gaps               | Pinned source links below                                   | Verified-code; e411 interoperability inferred, not tested           |
| Bridge serial query/order gap             | Serial handler versus TCP adapter/query/outbound client     | Verified-code path inspection; no claim about all future releases   |
| Broad Roche e411 LIVD availability        | CDC LIVD resources, HL7 Roche LIVD example                  | Comprehensive package UNVERIFIED; demo availability verified        |

### Bridge source links

[SENAITE]: https://github.com/senaite/senaite.astm/tree/2.x/src/senaite/astm
[HMIS]: https://github.com/hmislk/limsmw
[BRREADME]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md
[BRAUTHOR]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md
[BRPROFILES]: https://github.com/DIGI-UW/openelis-analyzer-bridge/tree/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/resources/analyzer-profiles
[BRBUFFER]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialFrameBuffer.java
[BRHANDLER]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialMessageHandler.java
[BRLISTENER]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/serial/SerialPortListener.java
[BRPARTS]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/profile/AstmResultParts.java
[BRPARSER]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/fhir/ASTMResultParser.java
[BRADAPTER]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/normalizer/ASTMBridgeAdapter.java
[BRQUERY]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/normalizer/AstmHostQuery.java
[BROUTBOUND]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/order/OutboundAstmClient.java

No device session, replay, profile validation or successful app integration test was performed. The checksums of the five synthetic upload frames in section 8 and of the checksum example in section 3 match the rule in section 3. The profile fragment is valid JSON syntax.
