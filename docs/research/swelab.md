Likely identity: Swelab Lumi, Boule's 5-part analyzer; confirm the nameplate and software before implementation.
Confirmed connectivity: Ethernet/bidirectional HL7; documented LIS destination port defaults to 5600.
Barcodes are optional: manual or auto-incrementing Sample IDs support operation without a scanner.
Main blocker: the Lumi-specific LIS specification and a de-identified wire capture; field positions/codes remain UNVERIFIED.
Bridge fit: promising inbound MLLP path, conditional on framing and sample ID placement; Dymind/Mindray compatibility is NOT established.

# Swelab analyzer research

Research date: 2026-11-14. Research only; no application implementation or live analyzer testing. **UNVERIFIED** means not established for the lab's actual Lumi/software, even where another analyzer documents the behavior. Confidence distinguishes a published product fact from applicability to this unidentified instrument.

## 1. Identity, parameters, sampling and status

**High confidence:** Boule markets Swelab Lumi as a 5-part hematology system [S1]. Swelab Alfa and Alfa Plus are 3-part systems [S4, S5]. The Swelab brand name alone cannot select a protocol.

To confirm the lab's instrument, obtain front and nameplate photographs, exact model/article number, serial number, software/About screen, LIS settings screenshots and a de-identified printed CBC+DIFF report. Record the manual revision supplied with it. Confirm whether five separately reported populations are NEU, LYM, MON, EOS and BAS, rather than the Alfa family's LYM/MID/GRA grouping [S4]. A report is supporting evidence; the nameplate and version are decisive.

Visual/product clues: Lumi has a 10.4-inch display and open aspiration; Alfa Plus has a 7-inch display and variants including Basic, Standard, Cap and Sampler [S1, S4]. Do not transfer Alfa cap-piercer/autosampler capabilities to Lumi.

| Item                | Evidence and confidence                                                                                                                         |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| CBC throughput      | Up to 64 samples/hour in the 11/2023 and 03/2026 flyers [S2, S3]; high for those revisions, actual lab version UNVERIFIED.                      |
| CBC+DIFF throughput | Up to 48 samples/hour [S2, S3]. An older Boule flyer says 60/45 instead [S6].                                                |
| Specimens           | Venous, capillary and prediluted blood [S2, S3].                                                                                                |
| Aspiration          | Samples are aspirated from an open tube; CBC uses 20 µL, CBC+DIFF 25 µL [S2]. Closed-tube Lumi option UNVERIFIED.                               |
| Loading             | Documented open aspiration supports a manually presented tube. No Lumi autoloader/rack specification found; do not invent slot addressing.      |
| Parameter count     | 2023: 20 diagnostic + 9 research = 29 [S2]. Newer BPS38940-7 (03/2026): 20 + 11 = 31, adding NRBC#/% [S3].                                      |
| Availability        | Boule lists it as a current product [S1]; not evidence of local stock, certification or installed firmware. No discontinuation notice known.    |

The product page is internally inconsistent: its headings say 29 clinical and six research parameters, but its lists contain 20 and nine. Prefer the explicitly enumerated, revisioned flyers; neither proves the installed instrument has newer parameters [S1–S3]. BPS38940-7 is dated **03/2026** in the document itself; cite that date [S3].

## 2. Physical connection

The manufacturer flyer lists four USB ports and one LAN port, and bidirectional HL7 to a LIS or HIS [S2]. Ethernet is the supported integration lead. No serial/RS-232 LIS specification or baud/parity settings found for Lumi: **UNVERIFIED**, not an alternative to assume. USB flash export is explicitly advertised [S1]; it is not a USB-to-PC live HL7 connection.

Lumi manual evidence, condensed across sections 2–8 below [S7, §2.6.5, §3.4, Table 5-5, §5.9, §7, §8]:

- Administrator: Setup → Communicate → Host Communication (analyzer networking); LIS Communication (LIS destination).
- Destination: LIS IP/port, default **5600**, range **1025–65535**; **Reconnect** after disconnection.
- USB flash: CSV result export.

**Inference, high confidence:** analyzer initiates TCP; LIS/connector listens. The service manual independently instructs configuring a LIS monitoring address/port and entering that destination on the analyzer [S8, §5.5]. Bridge's default shared MLLP port is 2575, not Lumi's destination default [B1]. Configure matching ports; neither number is an immutable protocol requirement.

## 3. Framing, ACK, timeout and reconnect

Lumi-specific manual evidence [S7, Table 5-5]:

- The analyzer can wait for an ACK or not. The wait defaults to **10 s** and can be set from **1 to 120 s**.
- If sending fails, the analyzer moves on to the next message.
- Graphs can be sent as PNG or BMP. Sending histograms and scattergrams can be turned off.

**UNVERIFIED for Lumi:** HL7 version; actual MLLP use; encoding; accepted ACK message/event, MSA codes and correlation requirements; retry count, retransmission identity, automatic reconnect/backoff, connection persistence and handling of partial frames. A bare HL7 claim does not establish MLLP. Manual Reconnect does not establish automatic recovery or resend.

For comparison only, the Dymind interface document specifies HL7 **2.3.1**, UTF-8 and standard MLLP framing. Each message starts with one VT byte (0x0B). Each segment ends with CR (0x0D). The message ends with FS then CR (0x1C 0x0D) [S10, §1.4.2].

It uses ACK^R01 with MSA-1 AA/AE/AR and MSA-2 echoing the original MSH-10 [S10, §1.4.2, §3.1–3.2]. These are a capture checklist, **not Lumi configuration facts**.

## 4. Results: messages, field map, parameters, flags and graphics

This research has no Lumi-specific LIS document and no verified Lumi HL7 example. **ORU^R01 is UNVERIFIED for Lumi.** The following comparison must not become a production mapping without vendor confirmation.

| Information                            | Lumi wire location                                    | Documented comparison                                                        |
| -------------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------- |
| Result message                         | UNVERIFIED                                            | Dymind ORU^R01 [S10].                                                        |
| Sample ID / typed ID / barcode text    | UNVERIFIED                                            | Dymind result OBR-3; Bridge accession prefers OBR-3 [S10, B4].               |
| Independent sample/run/sequence number | UNVERIFIED, including whether separate from Sample ID | No Lumi field established. Do not substitute OBX-1, the observation ordinal. |
| Sampling/blood/test mode               | UNVERIFIED                                            | Mindray fixture has separate mode OBXs; see §7 [B7].                         |
| Patient identifier/name                | UNVERIFIED                                            | Dymind PID-3/PID-5; Mindray profile reads PID.3.1/PID.5 [S10, B3].           |
| Birth date / sex                       | UNVERIFIED                                            | Dymind PID-7/PID-8 [S10]; not sample identifiers.                            |
| Analyte/value/unit                     | UNVERIFIED                                            | Dymind OBX-3/5/6 [S10]; Mindray profile reads OBX.3.1/OBX.5/OBX.6.1 [B3].    |
| Reference interval / abnormality       | UNVERIFIED                                            | Mindray OBX-7/8 [B3].                                                        |
| Status / measurement time              | UNVERIFIED                                            | Mindray OBX-11 / OBR-7 [B3].                                                 |
| QC marker                              | UNVERIFIED                                            | Dymind MSH-11=Q; Mindray profile tests MSH.11=Q [S10, B3].                   |
| Graph data                             | UNVERIFIED encoding/segment/code                      | Dymind binary histogram/scattergram payloads are Base64 in OBX-5 [S10].      |

### Complete parameter inventory and code status

Parameter names below are the flyer inventory, **not confirmed HL7 codes** [S2, S3]. The final column contains actual codes from the Bridge's **Mindray BC-5380** profile, solely to make comparison concrete [B3]. A standard LOINC-looking code does not prove Lumi sends it. Every Lumi code and wire unit remains **UNVERIFIED**.

| Lumi parameter | Published category         | Lumi transmitted code / unit | BC-5380 comparison code; explicit profile unit if present   |
| -------------- | -------------------------- | ---------------------------- | ----------------------------------------------------------- |
| WBC            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 6690-2; 10*9/L                                              |
| RBC            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 789-8; 10*12/L                                              |
| HGB            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 718-7; g/L                                                  |
| HCT            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 4544-3; unit omitted                                        |
| MCV            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 787-2; fL                                                   |
| MCH            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 785-6; pg                                                   |
| MCHC           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 786-4; g/L                                                  |
| PLT            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 777-3; 10*9/L                                               |
| MPV            | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 32623-1; fL                                                 |
| RDW-CV         | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 788-0; unit omitted                                         |
| NEU#           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 751-8; 10*9/L                                               |
| NEU%           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 770-8; unit omitted                                         |
| LYM#           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 731-0; 10*9/L                                               |
| LYM%           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 736-9; unit omitted                                         |
| MON#           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 742-7; 10*9/L                                               |
| MON%           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 5905-5; unit omitted                                        |
| EOS#           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 711-2; 10*9/L                                               |
| EOS%           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 713-8; unit omitted                                         |
| BAS#           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 704-7; 10*9/L                                               |
| BAS%           | Diagnostic                 | UNVERIFIED / UNVERIFIED      | 706-2; unit omitted                                         |
| RDW-SD         | Research                   | UNVERIFIED / UNVERIFIED      | 21000-5; fL                                                 |
| PCT            | Research                   | UNVERIFIED / UNVERIFIED      | 10002; mL/L (profile maps to LOINC 51637-7)                 |
| PDW            | Research                   | UNVERIFIED / UNVERIFIED      | 32207-3; unit omitted                                       |
| P-LCC          | Research                   | UNVERIFIED / UNVERIFIED      | No equivalent established from reviewed mapping             |
| P-LCR          | Research                   | UNVERIFIED / UNVERIFIED      | No equivalent established from reviewed mapping             |
| AL#            | Research                   | UNVERIFIED / UNVERIFIED      | ALY# is 26477-0, 10*9/L; equivalence to Lumi AL# UNVERIFIED |
| AL%            | Research                   | UNVERIFIED / UNVERIFIED      | ALY% is 13046-8; equivalence UNVERIFIED                     |
| IG#            | Research                   | UNVERIFIED / UNVERIFIED      | No equivalent established                                   |
| IG%            | Research                   | UNVERIFIED / UNVERIFIED      | No equivalent established                                   |
| NRBC#          | Research; newer flyer only | UNVERIFIED / UNVERIFIED      | No automated equivalent established                         |
| NRBC%          | Research; newer flyer only | UNVERIFIED / UNVERIFIED      | Profile has manual NRBCs% 18309-5; do not equate            |

Published Lumi linearity units include WBC/PLT ×10⁹/L, RBC ×10¹²/L and HGB g/L [S3]. Those printed units do not prove the transmitted spelling or locally selected units. Specifically verify HGB g/L versus g/dL, HCT fraction versus percent, and differential fractions versus percentages. Do not infer scaling from a parameter's `%` suffix; the Mindray fixture itself demonstrates fractional differential reference ranges [B7].

**Flags:** Lumi WBC/RBC/PLT suspect-flag identifiers, raised/clear values, numeric out-of-range encodings and invalid-result markers are all UNVERIFIED. Ask for captures of flagged runs and the vendor code table. The BC-5380 profile separates sample warning OBXs with `sample_flags`, `raisedValue: "T"`, and an explicit code list; that list must not be imported as Lumi's [B3]. Numeric reference flags and sample-level morphology warnings require separate treatment.

**Graphics:** graph-format controls above establish selectable images, not a Lumi ED layout. Base64, bitmap dimensions, compression, image code, OBX-2 datatype and payload size are UNVERIFIED. Disable graphs for the first scalar-result commissioning trial unless the approved slice includes images. The receiving application must never interpret graph payloads as clinical numbers.

**QC:** obtain the actual discriminator, control level/lot, expiry and run identifiers. Do not assume a QC sample ID prefix or MSH-11=Q merely because Mindray/Dymind use it. Unclassified QC must not be accepted into patient results.

## 5. Orders and queries

Lumi manual evidence [S7, Table 5-5]: bidirectional lookup matches Sample ID or medical-record number; timeout **10 s**, configurable **1–600 s**; lookup occurs on analysis start/patient editing.

Thus barcode scanning is not a demonstrated prerequisite to lookup. Exact Lumi query/response messages, request fields, not-found response and pushed-worklist support remain **UNVERIFIED**. Do not label it QRY, ORM or OML without the interface document.

Dymind's comparison document describes analyzer **ORM^O01 → LIS ORR^O02**. In the request, ORC-1 holds the order-control code RF, ORC-3 holds the requested sample ID and ORC-5 holds the order status IP. Its result sample ID is OBR-3, while the query response uses OBR-2 [S10, §3.1–3.2]. This is an investigation lead only. It does not establish QRY/DSR, OML, or unsolicited worklist push for Lumi.

For a result-only first connection, bidirectional lookup should stay disabled until the connector can produce the analyzer's required clinical order response. A generic ACK is not an order response.

## 6. Identification, especially without barcodes

Lumi manual evidence [S7, §3.4, §5.9, §7]: USB 2D scanner; manual/default auto-increment IDs; **1–25** keyboard characters, excluding Chinese/Japanese/Korean. Auto-increment: final digit required; all-zero ID prohibited.

**Practical implication:** scanner hardware is optional for a typed-ID workflow. Staff must enter the specimen identifier that the web app expects, or maintain an explicit association to an analyzer-generated identifier. A sequence that restarts or is reused cannot safely identify a visit by itself. This is a proposed operating rule, not a claim about current lab practice.

**Critical wire gap:** whether manually typed and automatically assigned Sample IDs occupy the same field as scanned IDs is UNVERIFIED. No separate Lumi sample-number field was established. Capture one manual-ID result, one scanned-ID result and two sequential auto-ID results; compare the returned identifier character-for-character, including leading zeros. Do not identify a specimen by patient name, PID patient number or OBX set ID.

**Scanner/symbology:** no built-in sample barcode reader established. Supported Code 128/39, EAN, Interleaved 2 of 5, QR/Data Matrix, scanner terminators and configured keyboard layout are **UNVERIFIED**. The manual's mention of a 2D scanner is not a tested symbology list. Confirm the actual scanner model and its manual. The Lumi's reagent RFID reader is a different function, not evidence of sample barcode reading [S2].

**Positions:** no Lumi rack/slot wire field established. Open aspiration supports a single manually presented tube [S2]. A UI mention of position [S7, §3.5] does not establish an autoloader. Photograph the actual sampler before deciding.

## 7. Annotated examples and OEM investigation

### Example availability

No verified Lumi wire example exists. The example below is **synthetic**: we wrote it with made-up values and local codes. It shows the segment layout that the Bridge's BC-5380 fixture uses [B7]. It is not a Lumi message, not copied from any vendor document, and not an interoperability test. Framing is omitted.

```hl7
OBR|1||SYN0001|PANEL^Synthetic panel^L|||20260101091500
OBX|1|IS|MODE^Sampling mode^L||X||||||F
OBX|2|NM|6690-2^WBC^LN||7.25|10*9/L|3.50-9.50|N|||F
```

Annotations: OBR-3 is the specimen identifier the Bridge reads. OBR-7 is the observation time. OBX-3 names the observation, OBX-5 holds the value, OBX-6 the unit, OBX-7 the reference interval, OBX-8 the flag and OBX-11 the result status (F = final). The first OBX stands for a mode record, not a numeric test. In the Bridge fixture that role is `08001^Take Mode^99MRC`, which the BC-5380 profile excludes [B3, B7]. The fixture's header declares `BC-5380`, `Mindray`, ORU^R01 and 2.3.1 [B7]. None of these literals should be substituted into a Lumi profile without evidence.

### Is Lumi a Dymind DF50 or Mindray-family OEM?

**UNVERIFIED. Moderate evidence of related software/interface conventions; low confidence in exact OEM identity; no evidence sufficient for protocol substitution.**

- A Boule-branded BM500 service manual explicitly covers both Medonic M51 and Swelab Lumi and names a separate **Description of LIS Communication Protocol for Boule BM500 Hematology Analyzers** [S8]. This supports a Boule platform/document-family lead, not a Dymind manufacturing claim. Ask Boule for the current controlled revision.
- Dymind DF50's own operator manual uses closely similar Setup/Communication organization, port 5600, ACK controls and bitmap transmission options [S9, §5.6]. Its explicit protocol defaults are HL7/2.3.1. Its bidirectional description triggers on ID input/scan, whereas Lumi's reviewed manual describes analysis start/editing. Similar wording/settings support a common software lineage hypothesis; differences prevent treating the manuals as interchangeable.
- The Bridge BC-5380 example/profile has similar result organization and distinguishes modes, warnings, graphs and QC [B3, B7]. Shared HL7 conventions and `99MRC`-style observations are not proof that Mindray manufactured Lumi or that proprietary codes match.
- We have no authoritative Boule, Dymind or Mindray statement that **Lumi = DF50**. No controlled document states Lumi's OEM or licensing relationship. Do not call the analyzer Swedish-made just because Boule is based in Sweden.

Ask Boule whether BM500 is the appropriate platform designation for this serial/software and whether a Dymind document is formally applicable. Even confirmed hardware ancestry would not establish firmware protocol equivalence.

## 8. Analyzer-side enablement and engineer involvement

The Lumi manual sends readers to their local distributor for the LIS specification [S7, §2.6.5].

The service manual's LIS procedure is: configure the workstation listener; connect Ethernet; set analyzer networking and LIS destination; save; check the LIS icon, which changes gray→black on connection [S8, §5.5].

Recommended commissioning sequence, subject to the actual version's vendor procedure:

1. Confirm model/software and acquire the correct LIS document before creating mappings.
2. Configure a dedicated connector listener and matching analyzer destination using the documented settings in §2.
3. Enable automatic result transmission and ACK checking; keep bidirectional queries disabled for a result-only trial.
4. Initially disable images; record all changes so the lab can restore its approved configuration.
5. Use de-identified commissioning specimens/records to verify identifiers, values, units, flags and receipt behavior end-to-end.
6. Test repeat transmission, network interruption/reconnect and connector restart before using live results.

**Engineer requirement:** initial installation/service belongs with Boule/distributor support; the service manual is written for that context [S8]. Whether an already installed analyzer needs a paid license, service password or engineer visit solely to enable LIS is **UNVERIFIED**. Administrator-accessible settings do not prove permission, licensing or service obligations for the lab's version. Ask the local Boule representative rather than bypass access controls.

## 9. Fit with OpenELIS Analyzer Bridge

Source inspected, pinned to **develop commit `5a3b8a361f873af566bfaae8a549d3b80a4d958e`** [B1–B7]. The repository's default branch is `develop`. Current code resides in `mllp/`, `fhir/` and `profile/`, not an `hl7/` package. This is a source review, not a replay or bench test.

### Transport and ACK

`HapiMLLPListener` creates a HAPI server and registers all message/event types [B5]. Therefore **if** Lumi sends standard MLLP to a LIS server, the existing transport is structurally suitable; no new socket listener is indicated. Actual as-is compatibility remains **UNVERIFIED** until the Lumi framing, encoding and messages are confirmed.

`HapiReceivingApplication` obtains source identity and MSH-3, calls `router.route(envelope)`, and returns `message.generateACK()` only when routing succeeds; failure throws a receiving exception [B6]. This is positive evidence of ordinary HAPI ACK handling, not proof of Lumi's acceptance of its event/header fields. Verify original MSH-10 correlation and actual response bytes with the instrument. ACK means Bridge durably holds the message; it does not mean the web app has saved results [B1, B6].

Create an active saved connection pinned to a profile; merely enabling shared port 2575 does not register the analyzer [B1]. Use Bridge SERVER role for the inferred analyzer-client topology. Do not use CLIENT role to receive persistent results: README explicitly says that is unimplemented [B1].

### Accession extraction: decisive compatibility check

`HL7ResultParser.parseAccessionFromOBR` returns the **whole trimmed OBR-3** when nonblank, otherwise **whole trimmed OBR-2**, otherwise null [B4]. It does **not** use an OBX, PID-3 or an independently configurable accession result-part path. Missing accession eventually displays as `HL7-UNKNOWN`; this is not a safe specimen match.

Consequences:

- If Lumi result Sample ID is a plain identifier in OBR-3, existing accession extraction should fit.
- If it is in OBR-2 with OBR-3 empty, fallback should fit.
- If OBR-3 contains a different run number while the app ID is elsewhere, fallback will not help.
- If Lumi sends a composite EI with assigning-authority components, the current function retains the complete field, rather than extracting EI.1. Determine whether that matches the app's accession contract.
- Lumi's actual placement remains **UNVERIFIED**; Dymind's OBR-3 result convention is only suggestive [S10].

### Scalar results, warnings, QC and graphics

The current HL7 template/profile can configure `resultParts` paths for test code, value, unit, reference interval, flags, status, completion, operator, patient and specimen description [B2, B3]. The parser treats NM/SN as numeric where valid, retains other values as text, and supports flag repetitions [B4]. There is no inspected bitmap/Base64-to-attachment implementation in this parser; an unexcluded ED OBX can become a text result. Raw message retention is not a usable graph viewer.

The BC-5380 profile is **results-only**, HL7 2.3.1, analyzer-initiated, Bridge SERVER. It excludes documented mode/age/graph/discriminator observations by OBX.3.1, recognizes sample warnings separately, and recognizes QC using MSH.11=Q [B3]. This is a useful design reference, not a Lumi profile to enable. Unknown observations otherwise reach the receiving OpenELIS workflow for review [B2]; our web app cannot assume it already provides that workflow.

### Profile sketch, deliberately not deployable JSON

| Profile element            | What to populate after obtaining the vendor specification                                                                            |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Identity                   | `profileMeta.id` such as `swelab-lumi`, manufacturer Boule, exact model/revision, documented confidence and sender identifier.       |
| Protocol/transport         | HL7 version and MLLP only when verified; TCP/IP, SERVER, analyzer-initiated.                                                         |
| Connection defaults        | Results-only, per-message aggregation; matching listener port; do not inherit template serial options.                               |
| `default_test_mappings`    | Exact transmitted codes, documented units/types and standard codings with vendor document/section per test. No copied BC-5380 codes. |
| `resultParts`              | Vendor-defined OBX fields and relevant PID/OBR fields; do not assume the template is a Lumi specification.                           |
| `resultRecordSelection`    | Exclude only documented non-result records, especially graph/mode records.                                                           |
| `sample_flags`             | Exact warning codes and raised value, with names where absent.                                                                       |
| `controlResultRecognition` | Vendor-defined QC evidence and levels; do not invent prefixes or claim a universal MSH-11 rule.                                      |

Profile authoring uses schemaVersion 2.0 and requires sourced model facts; undocumented items belong in a “Not verified” note and stay out of the profile [B2]. A capture checks actual configuration but does not replace the vendor specification as mapping authority.

### What code could be needed

1. **Potentially profile only for inbound scalar results:** if verified MLLP, OBR accession and OBX layouts fit existing behavior. Add vendor-example replay coverage and separately verify a site capture when implementation is authorized.
2. **Accession parsing adjustment:** only if the vendor puts the intended sample ID outside the supported OBR fields, gives competing identifiers, or requires EI component selection. Current `resultParts` cannot silently fix that.
3. **Bidirectional order-query handler:** current MLLP receiving code forwards messages and returns ACK; it does not implement a Lumi clinical lookup response. Existing outbound order support sends orders through outbound sessions [B1]. That is not an analyzer-requested ORM/ORR exchange on the inbound connection. Implement only after exact Lumi semantics are known and the slice is approved.
4. **Image decoding/storage/rendering:** additional work if graphs are required; otherwise configure omission/explicit profile exclusion. Do not build it speculatively.
5. **Web app receiving boundary:** Bridge currently delivers normalized FHIR bundles to OpenELIS `/analyzer/fhir`, not directly to this app's result model [B1]. A compatible app endpoint/adapter, explicit specimen matching, durable acceptance and mapping/review rules are separate future integration work. A profile does not supply those features.

## 10. Open questions, evidence register and next handoff

### Ask the lab

- Exact nameplate/model and software version; actual sampler and scanner photographs; locally supplied manual revision?
- Are tubes identified by barcode, handwritten number, patient number or a daily analyzer sequence? Who creates that number and when does it reset?
- Can staff type the app's specimen ID before aspiration? Preserve leading zeros and verify punctuation handling.
- Which fields/units appear on the report? Are research parameters enabled or approved for reporting locally?
- Is LIS already enabled/connected? Obtain settings and an export without disturbing the existing connection.
- Can the lab provide de-identified scalar, flagged, prediluted, repeat/corrected and QC messages, plus an ACK exchange? Do not upload identifiable records to public research services.

### Ask Boule/local distributor

- Provide **Description of LIS Communication Protocol for Swelab Lumi**, exact revision for this software; is the BM500 protocol title applicable?
- Confirm TCP client/server, port, framing, encoding, HL7 version, ACK event/status/correlation, retry/reconnect and duplicate behavior.
- Identify typed/scanned/auto-incremented Sample ID versus independent sample/run number, precise fields and maximum transmitted length.
- Supply every numeric/research parameter code, unit/scaling, flag and invalid-value table; confirm NRBC availability.
- Provide result, manual-ID, QC and order-query examples; distinguish specimen ID from medical-record matching.
- Confirm query trigger/types, not-found behavior, barcode-independent lookup and pushed-worklist support.
- Define graph datatypes/codes, encoding and maximum sizes; does disabling graphs remove all graph records?
- Confirm supported scanner/symbologies, actual sampler variants and any slot identifier.
- State whether LIS enablement needs a license, engineer, password or service agreement.
- Is there a formally supported Dymind/OEM protocol equivalence? Specify exact model/software/document; appearance is insufficient.

### Sources and confidence

| ID   | Source and locator                                                                                                                                                                                                                       | Confidence / use                                                                                          |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| S1   | Boule Lumi product page                                                                                                                                                                        | High for current listing, 5-part identity and export options; internally inconsistent parameter headings. |
| S2   | Boule flyer BPS38940-5, 11/2023, p.2                                                                                                                   | High: 20+9 inventory, 64/48 throughput, open aspiration, ports/HL7.                                       |
| S3   | Boule flyer BPS38940-7, internally 03/2026, p.2                                                                                                        | High for that revision: 20+11 inventory; installed version UNVERIFIED.                                    |
| S4   | Boule Alfa Plus Sampler, Specifications; Alfa Plus manual, intended purpose             | High: Alfa Plus is 3-part; model-dependent cap/sampler features.                                          |
| S5   | Boule: Philippines hematology market                                                                                                                                         | High: calls Alfa systems 3-part.                                                                          |
| S6   | Older Boule flyer, technical specifications                                                                                                                                      | Medium: 60/45 throughput; version applicability unknown.                                                  |
| S7   | Lumi user manual, Boule, November 2018, 1504529 (English and Spanish 1504529_ES) | Applicability UNVERIFIED.                                                                                 |
| S8   | Boule Medical BM500 service manual, title/contents and §5.5, pp.108–110                                                                                               | Medonic M51/Swelab Lumi family and separate protocol document; not an OEM declaration. |
| S9   | Dymind DF50 operator manual, §5.6, pp.62–66                                                                                            | Strong comparator for UI/settings, low for Lumi applicability.                         |
| S10  | Description of LIS Communication Protocol for Dymind Hematology Analyzers, §§1.4, 3.1–3.2                         | Example header identifies DH56, not Lumi/DF50. Applicable Lumi behavior UNVERIFIED.    |

Bridge sources below are pinned to the inspected commit. **High confidence in inspected behavior; no Lumi interoperability test performed.**

- B1: [README](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/README.md), Architecture, Result Delivery, orders and transport configuration. Source and README differ on timestamps: README says HL7 does not carry analyzer test time; current profile-driven parser reads `completed` and calls `withTimestamp`. Use code for that capability and verify its emitted output later.
- B2: [Profile authoring](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md) and [HL7 template](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/templates/hl7.json).
- B3: [Mindray BC-5380 profile](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/resources/analyzer-profiles/mindray-bc5380.json), mappings, configDefaults, sample_flags and controlResultRecognition.
- B4: [HL7ResultParser.java](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/fhir/HL7ResultParser.java), `parseAccessionFromOBR`, `readObservation`, `readPatient` and main parse loop.
- B5: [HapiMLLPListener.java](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/mllp/HapiMLLPListener.java), `start`.
- B6: [HapiReceivingApplication.java](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/main/java/org/itech/ahb/mllp/HapiReceivingApplication.java), `processMessage` and `extractAnalyzerId`.
- B7: [BC-5380 complete-message fixture](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/5a3b8a361f873af566bfaae8a549d3b80a4d958e/src/test/resources/mindray-examples/bc5380-c.3.3.hl7).

