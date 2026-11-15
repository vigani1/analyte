Summary 1/5: iFlash 1200 is a 120-test/hour, 16-position CLIA analyzer with documented barcode and position workflows.
Summary 2/5: The 2018 iFlash-family manual specifies HL7 2.3.1/MLLP and ASTM; applicability to this newer 1200 remains UNVERIFIED.
Summary 3/5: The 1800 PC connects to a listening LIS server; the same TCP role on this 1200 is inferred, with no verified default port.
Summary 4/5: Barcode-free LIS download is documented for 1200; exact result identity and query packets still need a capture.
Summary 5/5: Bridge has an MLLP listener and matching core ACK logic, but accession selection, query replies, qualitative results and OBR-only QC are gaps.

# YHLO iFlash 1200 integration research

Research date: 2026-11-14. Research only; no device access or executable upstream code run. Installed software version and exact model suffix are unknown. The separate Windows PC and handheld scanner are user-supplied observations.

**V** means verified in the cited document/source, not on the lab instrument. **I** means inferred. **UNVERIFIED** means unresolved. Where a manual's wording or layout is unclear, the note says so. Field maps below describe the 2018 family protocol unless explicitly marked 1200-specific. Sources and confidence are collected in §12; section references locate the evidence.

## 1. Identity, model differences and test menu

| Model       |                                Throughput | Sample/reagent handling                                                                                  | Evidence                                                                                           |
| ----------- | ----------------------------------------: | -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| iFlash 1200 | Up to 120 tests/hour; first result 13 min | 16 sample positions, STAT; microcups/blood collection tubes; 5–150 µL; 15 refrigerated reagent positions | V, manufacturer brochure [B], manual [O]                                                           |
| iFlash 1800 |                      Up to 180 tests/hour | Commonly 50 samples, 20 reagents                                                                         | V, brochure [E]; its specification page also lists 60 samples: configuration/revision needs checking |
| iFlash 3000 |                      Up to 300 tests/hour | 140 samples, 30 reagents                                                                                 | Distributor training FAQ [F]; not evidence of 1200 layout                                          |

The 1200 uses magnetic-particle direct chemiluminescence and acridinium ester. The brochure lists 16 sample positions and a STAT (urgent sample) function [B]. Typical panels include thyroid, fertility/hormones, tumor markers, anemia, bone metabolism, cardiac markers, autoantibodies, TORCH/EBV/respiratory serology, hepatitis/HIV and inflammation. Examples include TSH, FT3/FT4, FSH/LH, prolactin, hCG, ferritin, vitamin D, PCT and IL-6. Availability depends on market, assay approval and brochure date. These are menu names, **not LIS channel codes**. [B, E]

The 1200 operation manual V3.0 identifies **iFlash 1200-A**, software **V1** [O]. That does not identify the installed PC build. YHLO dates the 1200 launch to **2021** [Y]. Protocol V1.1 is dated **2018-01-17** and explicitly names iFlash3000 A/B/C/H/I [P §1.1]. **Identical LIS implementation across 1200/1800/3000 is UNVERIFIED.** Shared reagents and workflow do not prove wire compatibility.

FDA K223690 covers a chemiluminescence immunoassay analyzer, model iFlash 3000-C [FDA]. It is evidence about a different model, not 1200 clearance or protocol. GUDID's iFlash 9000-A record likewise supplies identity, not LIS layout [G].

## 2. Physical connection: roles, port and serial

**1200-specific V:** manual Table A-3 describes LIS via network or serial; the external scanner uses USB to the PC [O]. Distinguish the analyzer-control connection from the PC's LIS connection. Cables in an installation kit do not establish which link carries LIS.

**1800-specific V:** `Utility > System Settings > LIS Setting`, set `Communicate Through = TCP/IP`, enter Server IP/Port and Save. The service manual has the user enter a server IP and port, and describes them as the LIS address and its listening port [S §6.4.1]. Therefore **analyzer PC = TCP client, LIS/connector = TCP server** for that software. **I for 1200:** configure its destination as the connector address/listening port after inspecting its settings.

| Setting                   | Finding                                                                                                                                                                                |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Exact 1200 transport menu | **UNVERIFIED**; 1800 path above is a lead, not a verified 1200 path                                                                                                                    |
| Analyzer default TCP port | **UNVERIFIED**; neither Bridge 2575 nor template 5000 is a YHLO default                                                                                                                |
| Bridge incoming port      | Deployment default 2575, MLLP enabled separately [R]                                                                                                                                   |
| Serial setup              | 1800 exposes COM port, data bits, stop bit, parity and baud [S]; actual 1200 values, flow control, electrical pinout and null-modem requirement **UNVERIFIED**; do not assume 9600/8N1 |
| 1200 TCP-server option    | **UNVERIFIED**                                                                                                                                                                         |
| Serial through Bridge     | Template advertises RS-232; actual runtime/platform/adapter suitability must be checked separately [T]                                                                                 |

## 3. Low-level framing, timing and retries

**HL7 V:** `0B + HL7 payload + 1C 0D` (VT, FS, CR); every segment ends CR `0D`. [P §2.1.3–4] Separators are `| ^ ~ \ &`; escape literal delimiters. MLLP does **not** use ASTM ENQ, single-byte ACK or checksums.

- ORU requires HL7 `ACK^R01`, not merely TCP delivery. AE/AR causes up to **two resends** [P §2.3.2]. Missing-ACK schedule, restart replay and retry spacing remain **UNVERIFIED**.
- Numeric **HL7 ACK, QCK and DSR response deadlines are UNVERIFIED**. Do not reuse ASTM's 15-second timing as HL7 timing.
- **1800** settings give connection timeout default 15 seconds, connection-attempt interval 30–60 seconds, connect-at-startup and auto-reconnect [S §6.4.2]. These are connection settings, not proven application-response deadlines; 1200 values remain **UNVERIFIED**.
- Those settings imply reusable connections on 1800 (**I**); precise 1200 session persistence, keepalive and idle-close rules are **UNVERIFIED**. LIS application push does not imply the analyzer listens for a new TCP connection.
- Encoding offers `UTF8` or `ASCII`; the latter can mean the Windows regional code page (GBK example), despite the MLLP section mentioning ISO-8859-1 [P §1.3, §2.1.3]. Confirm real bytes for non-ASCII names.
- More than **50 assays per sample** splits results into multiple ORU messages, each requiring ACK [P §2.2.1.3]. Do not assume one ORU is the complete order.

**ASTM V:** ENQ `05` requests send rights, ACK `06` accepts, NAK `15` declines, EOT `04` releases. Frames are `STX(02) FN DATA ETB(17)/ETX(03) CHK1 CHK2 CR(0D) LF(0A)`. FN cycles ASCII 0–7. Checksum sums FN through ETB/ETX inclusive modulo 256, encoded as two uppercase hexadecimal characters; STX and trailing CR/LF are excluded. Each record is sent as a separate frame. Acquisition NAK: wait 10 seconds before requesting again. Manual gives 15-second inactivity/inter-byte/frame-response timeouts [P §3.1.2–5]. Retry ceiling and detailed corrupted-frame recovery are **UNVERIFIED**. Some examples inconsistently use ETB for the last record; clarify rather than copying that inconsistency.

## 4. HL7 message and field detail

### Structures and shared fields

| Message           | Segment order                     | Function                                              |
| ----------------- | --------------------------------- | ----------------------------------------------------- |
| Patient `ORU^R01` | MSH PID OBR {OBX}                 | Analyzer results                                      |
| QC `ORU^R01`      | MSH OBR                           | One assay's controls; **no OBX** in documented format |
| `ACK^R01`         | MSH MSA                           | LIS acknowledgment of result                          |
| `QRY^Q02`         | MSH QRD QRF                       | Analyzer asks for orders                              |
| `QCK^Q02`         | MSH MSA ERR QAK                   | Query acknowledgment/status                           |
| `DSR^Q03`         | MSH MSA ERR QAK QRD QRF {DSP} DSC | One sample order, repeated DSR for further samples    |
| `ACK^Q03`         | MSH MSA ERR                       | Analyzer acknowledges DSR                             |

V in [P §2.2]. Calibration is discriminator MSH-16=1; complete calibration-specific layout/example is **UNVERIFIED**.

| Field        | YHLO meaning                                                                    |
| ------------ | ------------------------------------------------------------------------------- |
| MSH-1/-2     | `                                                                               | `/`^~\&` |
| MSH-3/-4     | `YHLO` / model, e.g. `iFlash3000`; model is not proven unique instrument serial |
| MSH-5/-6     | Often blank on analyzer output; LIS replies target YHLO/model                   |
| MSH-7        | Message datetime, typically YYYYMMDDHHmmSS; timezone behavior UNVERIFIED        |
| MSH-9/-10    | Message/event / control ID (table length 20, increasing integer described)      |
| MSH-11/-12   | `P` / `2.3.1`                                                                   |
| MSH-15       | `P` for unsolicited LIS DSR; otherwise blank allowed; proprietary usage         |
| **MSH-16**   | **0 patient, 1 calibration, 2 QC**, not standard HL7 acknowledgment semantics   |
| MSH-18       | Charset; see code-page caveat                                                   |
| **MSA-1/-2** | **AA/AE/AR / inbound MSH-10 exactly**                                           |
| MSA-3/-6     | Free-text description / vendor error code; success shows short text and `0`     |
| MSA-4/-5     | Reserved blank                                                                  |
| ERR-1        | Error/location; success examples `0`                                            |

V in [P §2.3.1–2, §2.3.8]. Error codes include segment sequence 100, missing field 101, datatype 102, unsupported type/event 200/201 and internal error 207. Despite the manual's AA recommendation, do not acknowledge results that have not been durably accepted.

### Patient ORU mapping

| Field              | Meaning / caveat                                                                                                           |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| PID-1/-2/-3        | Sequence / admission / medical record; **PID-3 is not sample barcode**                                                     |
| PID-4/-5/-6        | Bed / patient name / ward (vendor repurposing)                                                                             |
| PID-7/-8/-9        | DOB / sex M,F,O,U / blood group                                                                                            |
| OBR-1              | Segment sequence                                                                                                           |
| **OBR-2**          | **Sample barcode**, table capacity 22; the field description is unclear, but the example puts a barcode here               |
| **OBR-3**          | **Local sample serial/sequence**, integer; not automatically LIS accession                                                 |
| OBR-4              | Example YHLO^iFlash3000, not assay code                                                                                    |
| OBR-5              | STAT Y/N                                                                                                                   |
| OBR-6/-7/-8        | Collection / observation start / observation end datetime                                                                  |
| OBR-9              | Manual sample dilution, floating point                                                                                     |
| OBR-10             | Rack^position, e.g. 1^1; exact 1200 carousel representation UNVERIFIED                                                     |
| OBR-13/-14/-15     | Clinical information / receipt datetime / specimen type (serum, plasma, urine examples)                                    |
| OBR-16/-18/-20/-21 | Ordering provider / icterus-hemolysis-lipemia description / attending doctor / department                                  |
| OBR-22/-25         | Report/status-change datetime / result status; actual population needs capture                                             |
| OBX-1              | Observation sequence                                                                                                       |
| **OBX-2**          | NM, ST or proprietary **BOTH** (number plus qualitative); table length 3 conflicts with four-character BOTH                |
| **OBX-3**          | **Editable LIS assay channel**, numeric or letters                                                                         |
| **OBX-4**          | Table calls it sub-ID; examples repeat English assay names. Dedicated name semantics **UNVERIFIED**; do not assume OBX-3.2 |
| OBX-5/-6/-7/-8     | Value / units / reference range / L,H,N abnormal flag                                                                      |
| **OBX-9**/-10      | **Qualitative call** (-,+,+-) / qualitative reference; vendor repurposes standard fields                                   |
| OBX-11             | Final status F                                                                                                             |
| OBX-13             | Raw measurement; exact physical meaning, e.g. RLU, UNVERIFIED                                                              |
| OBX-14/-16         | Observation datetime / examiner                                                                                            |
| OBX-15/-17         | Reserved in table; examples nonetheless populate department in -15                                                         |
| OBX-18             | Not documented; do not assume instrument serial                                                                            |

V in [P §2.3.3–5, §2.4.1.1]. ST examples leave OBX-5 empty and send the call in -9. Reading only OBX-5 loses qualitative results. Numeric datatype notes conflict with channel letters and symbolic calls; resolve with the current vendor specification.

**Reruns:** §2.3.5 leaves repeat-detection handling to the receiver and defines no reliable HL7 rerun/replicate field. Do not reinterpret OBX-4 as replicate index. Automatic/manual dilution distinction, whether concentrations are already corrected, per-assay dilution and replacement semantics are **UNVERIFIED** on 1200.

### QC and calibration

| QC field           | Meaning                                                                                                          |
| ------------------ | ---------------------------------------------------------------------------------------------------------------- |
| MSH-16             | 2                                                                                                                |
| OBR-2/-3           | Examples contain assay code/name (ALT/ALT); exact assignment needs confirmation because table labels are generic |
| OBR-4/-7           | YHLO/model / observation time in example                                                                         |
| OBR-11             | Count of control entries                                                                                         |
| OBR-12/-13/-14/-15 | Control identifier / name / lot / expiry                                                                         |
| OBR-17             | Level H/M/L                                                                                                      |
| OBR-18/-19/-20     | Mean / SD / measured concentration                                                                               |

Fields 12–15 and 17–20 carry **parallel `^`-separated arrays**, one component index per control entry, not one OBX per control. One assay per ORU; QC batching separates lots [P §2.3.4.2, §2.4.1.2]. Do not parse QC OBR-14 as specimen receipt datetime. Units and per-repeat timestamp associations remain **UNVERIFIED**. MSH-16=1 identifies calibration only; calibrator lot, curve and points layout is **UNVERIFIED**.

## 5. Query/order flow

Family flow is **QRY → QCK → DSR → ACK^Q03**, not ORM/OML [P §2.2.2–3, §2.3.6–11].

| Field             | Meaning                                                                              |
| ----------------- | ------------------------------------------------------------------------------------ |
| QRD-1/-2/-3/-4    | Query datetime / R / D / query ID                                                    |
| QRD-5/-6/-7       | Blank / blank / literal RD in examples                                               |
| **QRD-8**         | **Barcode** for live lookup; blank for number/batch/time query                       |
| QRD-9/-10/-11/-12 | OTH / blank / blank / T                                                              |
| QRF-1             | Model                                                                                |
| QRF-2/-3          | Time bounds                                                                          |
| **QRF-4/-5**      | **Start/end sample number**, equal for one number; nonempty -4 selects number lookup |
| QRF-6/-7/-8/-9    | RCT / COR / ALL / blank                                                              |
| QAK-1/-2          | SR / OK found, NF absent, AE error, AR rejected                                      |

The manual mistitles §2.3.6 ORD; packets use **QRD**. QCK's MSA correlates to QRY MSH-10. DSR includes query/filter and DSP data; examples clear QRD-8 even after barcode queries, so it is not a byte-for-byte query echo. Examples reuse query control ID for DSR; ACK must reference the actual DSR MSH-10.

**DSP-1 is a line index; DSP-3 is payload**; -2, -4 and -5 are generally empty in examples. Preserve blank indices [P §2.3.10]:

| DSP index     | DSP-3 meaning                                                                          |
| ------------- | -------------------------------------------------------------------------------------- |
| 1–10          | Admission, bed, name, DOB, sex, blood group, race, address, postcode, telephone        |
| 11/12         | Rack^position / collection datetime                                                    |
| 13/14         | Unused                                                                                 |
| 15–20         | Patient category, insurance account, charge type, nationality, native place, country   |
| **21/22**     | **Sample code/barcode / sample number**                                                |
| 23/24/25      | Inspection datetime / STAT Y,N / unused                                                |
| **26**        | Specimen type                                                                          |
| 27/28         | Requesting doctor / department                                                         |
| **29 onward** | One assay each: channel^name^unit^reference-range; only channel mandatory inside tuple |

The table marks code, number, STAT, specimen and assay lines with #; prose emphasizes barcode and item number. Whether DSP-21 may be blank in non-barcode mode is **UNVERIFIED**. A logical sample code without a printed label is plausible (**I**), but must be demonstrated on this version.

DSC-1 is nonempty for intermediate samples (1,2,…) and empty for the final sample. Send one DSR per sample, with its ACK exchange. QAK **NF** defines no data found; **whether QCK/NF alone ends the query or an empty terminal DSR is required is UNVERIFIED**. Do not fabricate an order or return OK for an empty lookup.

**Unsolicited orders:** family protocol permits DSR with **MSH-15=P**, acknowledged ACK^Q03 [P §2.4.3]. That does not specify a new inbound TCP session. Numeric query, QCK and DSR timeout/retry values remain **UNVERIFIED**.

## 6. ASTM alternative: record layout and selection

Application standard is **E1394-97**; §3.1 defines an E1381-like link layer. Do not replace this with a vendor directory's LIS2-A2 label without confirmation.

**Numbering warning:** ASTM vendor tables count the record letter itself as field 1. Verify Bridge's field-path convention before translating these positions; HL7 does not count the segment identifier.

| Record | Layout using vendor table numbering                                                                                                                                                            |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| H      | H-2 delimiters `\^&`; H-5 model^software^serial; -11 charset; -12 class PR patient, QR QC, CR calibration, RQ query, QA response, SA application; -13 1394-97; -14 timestamp                   |
| P      | Patient information; P-4 medical-record ID and P-6 name in examples; not sample accession                                                                                                      |
| O      | O-3 sample number^tray^position; -4 barcode; -5 repeated code^name^dilution^repeat-count^rerun-flag; -16 specimen; -26 F result, Q query response, O unsolicited order                         |
| R      | R-2 sequence; -3 code^name^replicate^type (I qualitative/F number/B both); -4 number^call; -5 unit; -6 range; -7 flag; -8 qualitative reference; -9 status; -13 completion time; -14 module ID |
| C      | C-3 I instrument/L LIS; -4 result comment; -5 comment type                                                                                                                                     |
| Q      | Q-3 start number^barcode; -4 end number; -5 ALL; -7/-8 time bounds; -13 O request/A cancel                                                                                                     |
| L      | L-3 N normal, I no information, Q query error                                                                                                                                                  |
| QC O   | O-12 control tuples repeated with `\`: ID^name^lot^expiry^concentration^level^SD^measured-result; exact concentration meaning needs confirmation                                               |

V in [P §§3.2–3.3, P2]. Flows: results H/P/O/R/(C)/L; QC H/P/O/L; query H/Q/L; downloaded orders H/P/O/L. O-5 supports up to 160 assays before splitting. **R-6 inconsistency:** table says upper/lower, examples appear lower/upper; order is **UNVERIFIED**. Some manual examples have unclear separators or status positions. Calibration class CR exists, but its full layout remains unverified.

Prefer ASTM if the installed 1200 actually offers it and a matching vendor document/capture or proven local integration establishes its behavior. It has explicit rerun/replicate and numeric-plus-call components, but still requires YHLO-specific order/QC parsing. Switching protocols is not a compatibility fix by itself. HL7/TCP is a candidate for result-only commissioning, not yet an acceptance-tested choice.

**Differences from LIS01-A2/LIS02-A2 (these go in the iFlash config, never in the standard code):**

- one record per frame (a permitted subset of LIS01-A2 §6.3.1, which allows up to 64,000 characters per frame);
- a 15-second receive timeout, against the standard's 30 seconds (LIS01-A2 §6.5.2.4);
- the first frame number is not stated (the standard starts at 1, §6.3.2.1);
- sample number, tray and position in O-3 and barcode in O-4, where LIS02-A2 §§8.4.3–8.4.4 define specimen ID and instrument specimen ID;
- the local assay code in the first component of the test ID, where LIS02-A2 §5.6.1 places local codes in the fourth;
- R-8 used for a qualitative reference instead of the nature of abnormality testing (LIS02-A2 §9);
- whole QC result tuples carried in O-12, which LIS02-A2 defines as an action code.

All of these are family-manual claims that still need confirming for the 1200.

## 7. Sample identification, especially without barcodes

**1200 V:** onboard scanning is documented separately from the handheld USB reader. Table A-8 lists **Codabar, Code 39, Code 128 and ITF**; barcoded sample workflow scans tubes and assigns serial/rack/position [O §2.2.4]. The external scanner enters sample IDs and scans reagent/method/calibrator labels. Its presence does not imply absence of an internal sample reader.

**Length/characters:** actual 1200 UI maximum, configured symbologies, check digits, case sensitivity, leading zeros and duplicate-ID scope are **UNVERIFIED**. Family OBR-2 capacity 22, QRD-8 capacity 60 and ASTM O-4 capacity 29 are inconsistent protocol capacities, **not proven scanner limits**. Do not declare one a universal device limit or assume delimiter characters are accepted.

**Without printed barcodes:**

1. Interactive/LIS: create orders in LIS, choose **LIS > LIS Apply**, download sample IDs/assays; software assigns serial numbers. Select unpositioned samples in Sample, assign positions, Save, then load those exact positions [W §5.2]. The SOP makes position assignment and saving an explicit step before loading.
2. Manual/local: select Sample, **Position No. mode**, enter Sample ID/Position, sample type, assays/options, Save; load tubes accordingly [O §2.2.4.4].
3. Family result fields are OBR-2 logical code/barcode, OBR-3 local sequence and OBR-10 rack^position. **Which contains the entered Sample ID on 1200 in this mode is UNVERIFIED**. A reusable rack position or daily sequence is not automatically a LIS specimen identifier.
4. **Barcode-free queries exist at family level:** QRF-4/-5 number or QRF-2/-3 time [P]. 1200 instructions document batch download [O, W]. Whether automatic position-triggered lookup happens on this build or explicit predownload is required is **UNVERIFIED**: the operation manual describes download at rack-position scanning, while SOP emphasizes predownload.

Thus no-barcode operation is feasible in the documented UI, but requires position assignment plus a proven sample-identity mapping. Confirm whether DSP-21 holds a logical ID even without a physical label.

## 8. Test codes, names and LOINC

LIS assay channels are **editable**, numbers or letters. OBX-3 and downloaded assay tuple codes must match [P §2.3.5, §2.3.10]. The older manual points to application/system/LIS settings; 1800 service instructions say select assay, enter channel, Save [S §6.4.2]. The 1200 manual explicitly exposes LIS code for calculated assays under `Utility > Assay settings > Calc. assay` [O §2.3.3]; do not generalize that path to standard assays.

**No authoritative 1200 channel list or vendor LOINC catalog located.** Family examples ALT, TP and UREA are example configured channels, not a verified 1200 menu/code list. Reagent catalog numbers are not proven LIS codes. Obtain installed assay/channel export with units, specimen, result types and raw qualitative values. LOINC assignment needs verified analyte/specimen/method/units and concentration-versus-index-versus-call semantics. No codes are guessed here. Bridge profile 2.0 requires sourced test codings [A].

## 9. Annotated examples

These are **synthetic teaching fixtures** we wrote for this note, with made-up values. They follow the documented field positions but do not copy any manual example. They are not captures or verified 1200 packets. **DEMO-1, DemoAssay and arb are invented placeholders**, not known channels/names/units. MSA-3 text is our own free-text placeholder. Model token iFlash3000 retains the documented family context. Each newline means CR. Wrap each message separately in `0B … 1C 0D`.

### Patient ORU and LIS ACK

```hl7
MSH|^~\&|YHLO|iFlash3000|||20261114100000||ORU^R01|101|P|2.3.1||||0||ASCII
PID|1||DEMO-PAT||Example
OBR|1|DEMO-SAMPLE|7|YHLO^iFlash3000|N|20261114095000|20261114095500||1|1^2|||||Serum
OBX|1|NM|DEMO-1|DemoAssay|12.3|arb|5-20|N|||F|||20261114095900
```

OBR-2 is sample identity; -3 local sequence; -10 rack/position. OBX-3 is channel, -4 illustrative name, -14 result time. Vendor-style ACK correlates MSA-2 to 101:

```hl7
MSH|^~\&|LIS|CONNECTOR|YHLO|iFlash3000|20261114100001||ACK^R01|101|P|2.3.1||||0||ASCII
MSA|AA|101|Accepted|||0
```

### QRY, QCK, DSR and analyzer ACK

```hl7
MSH|^~\&|YHLO|iFlash3000|||20261114100100||QRY^Q02|102|P|2.3.1||||||ASCII
QRD|20261114100100|R|D|8|||RD|DEMO-SAMPLE|OTH|||T
QRF|iFlash3000|||||RCT|COR|ALL
```

Barcode is QRD-8. For number-only lookup leave QRD-8 blank and use `QRF|iFlash3000|||7|7|RCT|COR|ALL`.

```hl7
MSH|^~\&|LIS|CONNECTOR|YHLO|iFlash3000|20261114100101||QCK^Q02|102|P|2.3.1||||||ASCII
MSA|AA|102|Accepted|||0
ERR|0
QAK|SR|OK
```

```hl7
MSH|^~\&|LIS|CONNECTOR|YHLO|iFlash3000|20261114100102||DSR^Q03|102|P|2.3.1||||||ASCII
MSA|AA|102|Accepted|||0
ERR|0
QAK|SR|OK
QRD|20261114100100|R|D|8|||RD||OTH|||T
QRF|iFlash3000|||||RCT|COR|ALL
DSP|1||
DSP|2||
DSP|3||Example
DSP|4||
DSP|5||O
DSP|6||
DSP|7||
DSP|8||
DSP|9||
DSP|10||
DSP|11||1^2
DSP|12||20261114095000
DSP|13||
DSP|14||
DSP|15||
DSP|16||
DSP|17||
DSP|18||
DSP|19||
DSP|20||
DSP|21||DEMO-SAMPLE
DSP|22||7
DSP|23||
DSP|24||N
DSP|25||
DSP|26||Serum
DSP|27||
DSP|28||
DSP|29||DEMO-1^DemoAssay^arb^5-20
DSC|
```

DSP-21/-22 identify sample code/number; -29 carries the assay tuple; empty DSC ends this batch.

```hl7
MSH|^~\&|YHLO|iFlash3000|||20261114100103||ACK^Q03|102|P|2.3.1||||||ASCII
MSA|AA|102|Accepted|||0
ERR|0
```

Unsolicited DSR sets MSH-15=P (`…|2.3.1|||P|||ASCII`). No-order status is `QAK|SR|NF`; the complete terminating no-order transcript is **UNVERIFIED**.

### QC ORU: one assay, two control entries

```hl7
MSH|^~\&|YHLO|iFlash3000|||20261114100500||ORU^R01|103|P|2.3.1||||2||ASCII
OBR|1|DEMO-1|DemoAssay|YHLO^iFlash3000|||20261114100400||||2|CTRL-L^CTRL-H|ControlLow^ControlHigh|LOT-A^LOT-A|20261231000000^20261231000000||L^H|10^30|1^2|10.2^29.5
```

MSH-16=2, OBR-11=2; fields 12–20 align by component index. No PID/OBX. ACK is ACK^R01/MSA AA referencing 103, with class 2 in vendor-style header. Calibration fixture is omitted because layout is **UNVERIFIED**.

## 10. Analyzer-side setup and vendor engineer

Proposed commissioning checklist, **not locally verified setup**:

1. Record exact model/build; 1200 status bar exposes software version [O]. Obtain the matching LIS manual and preserve current settings.
2. Inspect transport/protocol settings. The **1800** TCP path is `Utility > System Settings > LIS Setting`; choose TCP/IP, connector IP/listening port, Save [S]. Exact 1200 equivalent, protocol selector and licensing are **UNVERIFIED**.
3. Inspect encoding, timeout, startup connection, reconnect and completed/incomplete result upload. Named options are V for 1800 [S], not verified on 1200.
4. Export channel mappings and choose barcode or position workflow. Verified 1200 download path: `LIS > LIS Apply` [W].
5. Confirm LIS connected status; use approved synthetic/nonpatient commissioning data. 1200 exposes `Result > Send to LIS`; SOP exposes `QC > Results > Send To LIS` [O, W]. Obtain de-identified result, query, no-order, QC, no-barcode, dilution and rerun transcripts.

**Vendor engineer mandatory: UNVERIFIED.** Permissions exist, but no source proves ordinary users can edit all settings or that an engineer/license is always required [O]. Ask YHLO/distributor for version-specific enablement and rights. A handheld scanner is not itself a LIS connector.

## 11. OpenELIS Analyzer Bridge fit

### Version and reviewed resources

Reviewed **develop commit `5a3b8a361f873af566bfaae8a549d3b80a4d958e`**, 2026-11-14. Read [README][R], [profile-authoring][A], [HL7 template][T] and [shipped Mindray HL7 profile][M]. No YHLO profile was found in this commit's resource tree. The template is **templates/hl7.json**, not under src/main/resources; the actual resource example is **src/main/resources/analyzer-profiles/mindray-bc5380.json**. Schema version is **2.0**. Authoring quote: “A profile describes an instrument model and nothing about a site” [A].

### Can MLLP receive iFlash ORU as-is?

**Framing fits in principle; end-to-end compatibility is not established.** Shared listener defaults to 2575, requires enablement and registers all message/events [R, H]. If 1200 behaves like 1800, **Bridge SERVER/analyzer CLIENT** fits. SERVER describes the Bridge, not the analyzer.

Confirmed source-level gaps:

1. **Wrong accession precedence:** `HL7ResultParser.parseAccessionFromOBR` uses **OBR-3 first**, then -2 [J]. YHLO -3 is local sequence, -2 barcode. A simple patient packet can therefore be attributed to the wrong specimen. `Hl7ResultParts` has no accession selector [K]; resultParts alone cannot fix it.
2. **Qualitative/BOTH:** YHLO ST puts call in OBX-9 with empty -5; proprietary BOTH has number plus call. The standard resultParts path classifies -5 using OBX-2 [A, J]. Call -9, reference -10 and raw measurement -13 need explicit handling; generic HL7 support does not prove both outcomes survive.
3. **OBR-only QC:** Bridge produces observations from OBX; YHLO QC has none. Current resultParts cannot expand parallel OBR arrays into control observations. Recognition of class 2 is not extraction. These packets yield no parsed results in this path [J, D].
4. **HAPI validation/version:** dependencies are HAPI base 2.5.1 and structures-v251, not structures-v231 [V]. Generic fallback and acceptance of MSH-16 repurposing/BOTH require replay against the selected deployment. No upstream binaries/code were run here.

### ACK compatibility

**Core semantics match by source:** handler returns `message.generateACK()` after successful routing [H]. In matching HAPI **v2.5.1**, generateACK defaults to AA; fillResponseHeader sets ACK, copies incoming trigger event, and sets MSA-2 to inbound MSH-10 [U]. Accepted ORU^R01 should therefore produce **ACK^R01, MSA-1=AA**, correctly correlated.

**Exact example equivalence is UNVERIFIED:** HAPI generates new MSH-10 rather than vendor-example echo; does not explicitly copy class MSH-16 or success MSA-3/MSA-6; may add MSH-9.3=ACK with available structures. Check whether 1200 accepts these differences. A Bridge AA confirms durable local receipt, not final app persistence/clinical acceptance. Storage/routing failure triggers error handling and possible analyzer retries [H, N].

### QRY behavior today

Listener wildcard and canProcess=true admit QRY. `MessageNormalizer` special-cases **ASTM** queries only; HL7 QRY is durably stored like ordinary input. On accepted receipt HAPI returns generic **ACK^Q02**, **not QCK^Q02**. No YHLO QRD/QRF lookup-to-DSP/DSR path was found. Subsequent result rendering has no OBX and returns **PARSE_NO_RESULTS**, subject to prior attribution checks [H, N, D]. Unknown sources can be held for attribution first. Receiving/ACKing a query is not answering it.

### Partial profile sketch — not installable

This is a **subset** of a future schema-2.0 profile, not a validated configuration. Model/version/protocol applicability remains a candidate. Empty mappings avoid invented tests/LOINC codes; required connectionFields, catalog details and sourced tests must be completed. Unknown serial defaults and analyzer port are omitted.

```json
{
  "schemaVersion": "2.0",
  "profileMeta": {
    "id": "site.yhlo-iflash-1200-candidate",
    "version": "0.1.0",
    "displayName": "YHLO iFlash 1200 UNVERIFIED candidate",
    "confidence": "LOW"
  },
  "analyzer_name": "YHLO iFlash 1200",
  "manufacturer": "YHLO",
  "model": "iFlash 1200",
  "category": "IMMUNOLOGY",
  "protocol": { "name": "HL7", "version": "2.3.1" },
  "msh3_pattern": "^YHLO$",
  "identifier_pattern": "^YHLO$",
  "transport": ["TCP/IP"],
  "transport_config": { "TCP/IP": { "framing": "MLLP" } },
  "communication": {
    "mode": "ANALYZER_INITIATED",
    "supports_lis_initiated": false
  },
  "capabilities": {
    "inboundResults": true,
    "outboundOrders": false,
    "connectionTest": true
  },
  "default_test_mappings": [],
  "configDefaults": {
    "dataFlow": "RESULTS_ONLY",
    "transport": "TCP/IP",
    "connectionRole": "SERVER",
    "aggregationMode": "PER_MESSAGE",
    "extractionOverrides": {
      "resultRecordSelection": { "mode": "ALL" },
      "resultParts": {
        "testCode": "OBX.3.1",
        "value": "OBX.5",
        "unit": "OBX.6.1",
        "range": "OBX.7",
        "flag": "OBX.8",
        "status": "OBX.11",
        "completed": "OBX.14",
        "operator": "OBX.16",
        "patientId": "PID.3.1",
        "patientName": "PID.5",
        "specimenDescriptor": "OBR.15"
      }
    }
  }
}
```

Do not deploy: sketch does not solve accession choice, calls, control recognition or QC extraction. Manufacturer-wide YHLO identifier needs saved-connection/source-IP binding. OBX-4 sub-identity is deliberately omitted until clarified. Do not inherit OBX-18 instrument or SPM-4 specimen just because the template uses them.

Needed capabilities to evaluate: barcode-first configurable accession, ST/BOTH calls, OBR-array QC/calibration parsing, QRY→QCK→DSR→ACK with real order lookup/no-order handling, and session-aware DSR push. A profile describes facts and extraction rules; it does not implement that multi-message state machine. These findings are not authorization to implement changes.

**Role mismatch:** none expected for inbound results under PC-client behavior. Bridge HL7 CLIENT supports certain outbound order sessions [R]; reversing roles does not add YHLO query support. If 1200 listens, or DSR must travel on an existing analyzer-initiated session, verify that exact path. Do not assume opening a second TCP connection works. Bridge's OpenELIS pairing/forwarding contract also needs evaluation before use with a different lab web app; it is not automatically a generic app adapter [R].

## 12. Open questions and source confidence

### Questions for lab/YHLO

1. Exact model suffix/build and current matching LIS manual? Explicit confirmation that V1.1 applies to 1200?
2. Enabled/licensed protocols, exact menus, TCP role/port/encoding, rights and engineer requirement?
3. Numeric ACK/QCK/DSR deadlines, missing-response retries, reconnect/replay and idle policy? Must ACK echo class or MSA-6=0?
4. De-identified barcode/no-barcode result/query transcripts? Which field holds entered Sample ID, how unique is sequence, may DSP-21 be blank, is lookup automatic or predownloaded?
5. Built-in reader configuration, accepted symbologies/length/characters/check digits, leading zeros, duplicate scope and handheld suffix?
6. Installed assay/channel/unit/type export, raw qualitative values, standard-assay mapping menu and official LOINC catalog?
7. QC/calibration captures with lots/levels/units/times/repeated runs; does this build use OBR-only QC?
8. Rerun and manual/automatic dilution captures: flags, corrected concentrations and replacement semantics?
9. Exact no-orders transcript and batch continuation rules? Unsolicited DSR on existing socket? One outstanding DSR at a time?
10. ASTM revision/range ordering/checksum/retries/final ETX behavior, control tuple meanings and serial electrical settings?

### Key-claim confidence

| Claim                                          | Confidence                                                               |
| ---------------------------------------------- | ------------------------------------------------------------------------ |
| 1200 120/h, 16 positions and built-in scanning | High document confidence [B, O]; site configuration UNVERIFIED           |
| Barcode-free download/position assignment      | High document confidence, 1200 SOP/manual [W, O]                         |
| 2018 protocol identical on installed 1200      | **UNVERIFIED**, predates model [P, Y]                                    |
| PC TCP client / LIS server                     | V for 1800 [S], **I** for 1200                                           |
| MLLP bytes and field positions                 | V in family tables/examples [P, P2], not verified on 1200                |
| Bridge ACK^R01/AA                              | Source verified [H, U], wire acceptance UNVERIFIED                       |
| Bridge accession precedence/QC/query gaps      | Source verified/inferred execution paths [J, K, N, D], no runtime replay |
| Commercial integrations exist                  | Vendors claim them; not independent interoperability proof [C1–C6]       |
| Open-source iFlash driver/code list            | None found; not proof none exists                                        |

### Sources and short quotations

- **[P] YHLO LIS Protocol Manual V1.1, 2018-01-17.** Manufacturer-authored. §1.1 scopes it to the iFlash3000 A/B/C/H/I models. §§2.2–2.4 layouts/examples; §§3.1–3.3 ASTM.
- **[B] YHLO 1200 brochure.** Manufacturer-branded product document; specs/menu, not LIS protocol.
- **[O] 1200 Operation Manual V3.0, software V1.** Manufacturer-authored, model 1200-A. A-3 network/serial, A-8 barcode symbols, external scanner, §2.2.4 sample modes, §2.3.3 calculated assay codes. Exact local revision unknown.
- **[W] 1200 SOP V1.0.0, effective 2022-03-08.** Manufacturer SOP; §5.2 barcode-free LIS download, §4.5 QC upload, §5.3 handheld input.
- **[S] 1800 Service Manual, software V1.** Manufacturer-authored, §6.4 pp.149–151: roles, menus, timing, code editing. Strong for 1800, not direct 1200 evidence.
- **[E] iFlash 1800 New.** Four-page brochure, not LIS manual; its sample-position figures conflict, which limits confidence.
- **[F] CPC Diagnostics iFlash FAQ.** Distributor training: family throughput/positions, lower authority than current manufacturer specs.
- **[Y] YHLO launch history.** Manufacturer gives 2021 as the 1200 launch year, which explains the 2018 scope limitation.
- **[FDA] FDA K223690.** Primary regulator; names 3000-C/hCG, no 1200 host interface.
- **[G] AccessGUDID 9000-A DI 06925912718154.** Primary identity record, not communications specification.
- **[C1] Veona directory.** Middleware vendor claims ASTM E1381/E1394 (labelled LIS2-A2) over RS-232 or TCP for the 1200. Gives no packets or version.
- **[C2] Dorays machine list.** Vendor lists the 1200 as a bidirectional LAN connection. Gives no protocol or role details.
- **[C3] LISBridge protocol support**, **[C4] instrument list**. Vendor lists YHLO iFlash ASTM, RS-232/TCP, unsolicited download. Family marketing claim, not model-specific wire proof; generic outbound-dial wording does not override YHLO's client-role instructions.
- **[C5] CGM ANALYTIX list**, **[C6] LabEEM Plus**. Vendors list 1800 and 1200/3000 respectively; no field layouts.
- **Bridge primary code:** [R] README, [A] guide, [T] template, [M] shipped HL7 profile, [H] receive handler, [J] result parser, [K] resultParts, [N] normalizer, [D] renderer, [V] dependencies. Handler quote: `return message.generateACK();` Parser quote: `Try filler (field 3) first, fall back to placer (field 2).` Source inspection, **not executable verification**.
- **[U] [HAPI v2.5.1 AbstractMessage][U].** Exact dependency-version source establishes AA/event/control-ID behavior; actual instrument acceptance remains untested.

[R]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md
[A]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md
[T]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/templates/hl7.json
[M]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/resources/analyzer-profiles/mindray-bc5380.json
[H]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/mllp/HapiReceivingApplication.java
[J]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/fhir/HL7ResultParser.java
[K]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/profile/Hl7ResultParts.java
[N]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/normalizer/MessageNormalizer.java
[D]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/routing/NormalizedBundleRenderer.java
[V]: https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/pom.xml
[U]: https://raw.githubusercontent.com/hapifhir/hapi-hl7v2/v2.5.1/hapi-base/src/main/java/ca/uhn/hl7v2/model/AbstractMessage.java
