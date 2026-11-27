# YHLO iFlash 1200

## Machine

The [machine definition](machine.toml) is written from the interface manuals
and ships at evidence level documented (ADR 0034 amendment, 2026-11-26). No
real analyzer has confirmed it yet. A de-identified recording can support
a higher evidence level for the capabilities it exercises.

The iFlash 1200-A is run from an analyzer PC. The service manual installs it
with the iFlash 3000 PC software: the installer offers the 1200 as one model,
and the PC's folders, logs and LIS screens are those of the iFlash 3000
(service §§6.3.2 and 6.4). The LIS manual V1.1 is written for the iFlash 3000
series. This definition applies it to the 1200 on that basis. Staff can enter
a tube barcode with a handheld scanner at the PC.

The service manual's iFlash 1200 installer shows PC build V1.01.10.01.10
on the confirmation and completion screens (§6.3.2.1, p. 93, Figures 6-39
and 6-40). The LIS settings screenshot shows only a short V1 label (§6.4.1,
p. 96, Figure 6-48). Read the full build from the installer completion
screen or confirm it with the installer. LIS V1.1 §2.3.1, p. 11 assigns
MSH-3 to the application and MSH-4 to the model. The definition has no wire
selector for the PC build.

## Connection

The definition offers the TCP profile. The iFlash PC connects to the LIS, so
Analyte listens. In **Utility → System settings → LIS settings**, set
Transmission mode to TCP/IP and enter the Analyte listener's address and port
(service §6.4.1). Put that listener and the iFlash PC's source IP in the
site's `endpoint` and `allowed_peers` (ADR 0026). The manuals give no fixed
port.

HL7 2.3.1 messages end each segment with CR and travel in MLLP blocks, with
VT before the message and FS CR after it (LIS V1.1 §§2.1.1–2.1.4). On the
same screen the Protocol field must read HL7: the screen also offers ASTM
(LIS V1.1 §3), which this definition does not cover.

The service manual's **Respond timeout limit** defaults to 15 s and describes
the timeout for connecting to the LIS (service §6.4.2.1, p. 97). Analyte uses
15 s as its own limit for a query, sending a reply and waiting for each
ACK^Q03. Confirm transaction timing on the installed PC. Auto reconnection
uses an interval of 30 to 60 s. Connect to LIS after startup is a separate
checkbox. Both are analyzer settings that Analyte does not need to know.

## Settings on the machine

### Setup questions

Setup asks exactly these. Staff read each answer on the iFlash PC.

| Choice | Values | Where staff find it | Suggested |
| --- | --- | --- | --- |
| `communication_type` | `one-way` (results only), `two-way` (results and barcode queries) | iFlash PC → Utility → System settings → LIS settings, Communication type | None: the manuals give no factory default |
| `software_version` | V1.01.10.01.10, Other version (not checked) | iFlash PC installer completion screen | None |

Choose V1.01.10.01.10 only when the full build is confirmed. Choose Other
version (not checked) for any other or unconfirmed build. Overview reports
that choice as unchecked. Both choices use the same result and order rules.

`one-way` turns orders off: a query gets an application error (AE). `two-way`
answers barcode queries, but only from a trusted order cache. If the lab app
cannot be reached or the cache is stale, the reply is an AE and never "no
orders" (ADR 0003 rule 9). Query replies stay off until the lab picks
`two-way`.

### Other settings to check on the iFlash PC

- **Protocol:** HL7. ASTM is not covered.
- **Encoding:** ASCII. LIS V1.1 §1.3 offers UTF-8 and an "ASCII" option.
  ASCII actually uses the PC's local code page, for example GBK on a Chinese
  Windows. Analyte reads strict ASCII, so a byte outside ASCII is refused
  visibly and kept. The manual does not say which MSH-18 label the UTF-8
  option sends, so it is not offered.
- **Real-time transmit completed test / uncompleted test:** each checkbox
  sends the current day's results when a sample changes state
  (service §6.4.2.1). Uncompleted results have no documented wire marker.
  They arrive like any upload and stay held while report completion is
  unknown (see Departures).
- **Channel No.** per assay (service §6.4.2.2). This is the OBX-3 code
  Analyte delivers. Give the lab app team the list. The `test_system` is
  `yhlo/iflash-1200` (ADR 0029). No test list ships with Analyte.

## Supported workflows

| Wire behavior | Handling | Source |
| --- | --- | --- |
| Patient result upload | ORU^R01 with MSH-16 `0`, then PID, OBR and one or more OBX. The sample ID is OBR-2, the tube barcode, kept as written with case and leading zeros. OBR-10 gives rack and position. OBR-9 keeps the manual dilution factor, with its application unknown. OBR-15 is the specimen text. | LIS V1.1 §§2.2.1, 2.3.4.1 |
| Observation | OBX-3 is the Channel No., kept as sent. OBX-5 is the value as written. OBX-6 keeps the unit, with no conversion. OBX-7 is the reference range and OBX-8 the flag (L, H, N). OBX-9 is the qualitative call with OBX-10 as its reference. BOTH keeps the number and the call. ST can carry only a call. The SI assay's three components keep their matching OBX-13 raw values. OBX-13 is the raw result and OBX-14 the observation time. | LIS V1.1 §2.3.5 |
| QC upload | ORU^R01 with MSH-16 `2`: OBR only. OBR-11 counts the controls. OBR-12 to OBR-15 and OBR-17 to OBR-20 list each control's number, name, lot, expiry, level, mean, SD and value. A row maps only when the count and every list agree. Without a documented measurement identity, controls stay held. | LIS V1.1 §§2.2.1.4, 2.3.4.2, 2.4.1.2 |
| Calibration upload | MSH-16 `1` is kept raw and held. | LIS V1.1 §2.3.1 |
| Result acknowledgment | AA after the raw upload and its accepted or held outcome are durably stored. MSA-2 echoes the upload's MSH-10. Held results appear in Needs review. An HL7 syntax error found while interpreting a parsed message or an unsupported message type gets AE. The manual says AE or AR can trigger up to two resend attempts. | ADR 0010, LIS V1.1 §2.3.2, p. 12 |
| Barcode query (two-way only) | QRY^Q02 with the barcode in QRD-8, QRD-9 `OTH`, QRD-12 `T` and QRF-1 `iFlash3000`. The LIS answers QCK^Q02, then one DSR^Q03 page per matching order. The barcode goes in DSP-21 and the channel numbers from DSP-29. The PC confirms each page with ACK^Q03, which makes that order Sent. Sample-number and time-range queries get AE: the order cache cannot answer them. | LIS V1.1 §§2.2.2, 2.3.6–2.3.11, 2.4.2 |
| Result identity | The barcode (OBR-2), the sample serial number (OBR-3), the observation end time (OBR-8) and the channel number. If any of them is missing, the result is held. Content that changes under the same key is held. | LIS V1.1 §2.3.4.1, p. 17, ADR 0035 |

LIS V1.1 §2.3.4.1, p. 17 calls OBR-3 the sample serial number and
OBR-8 the observation end time. It gives no guarantee that OBR-3 changes
on each rerun. The resend rule (§2.3.2, p. 12) gives no guarantee that
OBR-3 stays stable on a resend. The recipe therefore requires OBR-8 as
additional run evidence. The sample upload (§2.4.1.1, p. 30) leaves OBR-8
blank, so an upload like it is held. An iFlash 1200 recording must
confirm which fields it sends and how their values behave.

Patient uploads missing run evidence or a final report status and QC
uploads without measurement identity stay in Needs review in Analyte.
The iFlash PC receives AA for those uploads after storage. An operator
releases or resolves the held results in Analyte.

## Departures

All three rules are ADR 0015 declarations for model `yhlo/iflash-1200`,
firmware `unknown`, HL7 2.3.1, machine to host, at level documented. Each
cites LIS V1.1. That manual describes the iFlash series interface rather
than one software build, so it covers the build the lab has not read yet.
Each communication type selects its own declarations
(`[[choice_departures]]`).

| Rule | Modes | What it allows | Source |
| --- | --- | --- | --- |
| `hl7_msh16_message_category` | one-way, two-way | MSH-16 of an ORU^R01 carries the upload category: 0 patient, 1 calibration, 2 QC | LIS V1.1 §§2.2.1, 2.3.1, 2.4.1 |
| `hl7_categorized_oru_results` | one-way, two-way | The documented patient, SI and OBR-only QC layouts | LIS V1.1 §§2.3.4.1, 2.3.4.2, 2.3.5, 2.4.1 |
| `hl7_record_display_query` | two-way | The record-format barcode query and positional DSR reply | LIS V1.1 §§2.2.2, 2.3.6–2.3.11, 2.4.2.1 |

No rule reads a blank OBR-25 as a final report. The manual lists OBR-25 but
never says what a blank one means, and only F is documented for OBX-11.
Patient results with a blank OBR-25 therefore stay held for an operator. No
`hl7_absent_obr25_means_final` rule is declared.

### Gaps

| Gap | Status |
| --- | --- |
| Normal operation was blocked | **Closed:** the definition ships at documented level |
| MSH-16 category and categorized results | **Closed** by the two departures above |
| Barcode query, paging and per-page ACK | **Closed** for two-way, by `hl7_record_display_query` |
| Lab choice of one-way or two-way | **Closed:** `communication_type` |
| Result identity recipe | **Open:** the configured key requires OBR-8. Confirm the fields and their behavior across resends and reruns on a 1200 |
| Report completion when OBR-25 is blank | **Open:** such results stay held |
| Uncompleted-test uploads | **Open:** no wire marker documented; held with the rest |
| Corrections and reruns | **Open:** no correction or rerun code documented; changed content is held |
| QC measurement identity and calibration uploads | **Open:** held raw |
| Serial link (LIS V1.1 §1.2) | **Open:** cable settings unconfirmed, not offered |
| UTF-8 option and local code pages | **Open:** MSH-18 label for UTF-8 undocumented; only strict ASCII offered |
| ACK fields | **Open:** Analyte's ACK uses its own MSH-10 and leaves out the MSH-16 echo and MSA-6 code that the vendor examples show. Confirm the PC accepts it |
| DSR reply IDs and blank DSP-22 | **Open:** the manual's examples reuse IDs while §2.3.1 calls MSH-10 unique |
| QRF-1 on a 1200 | **Open:** the manual shows `iFlash3000`. A different value gets AE |
| Unsolicited DSR (LIS V1.1 §2.4.3) | **Open:** not sent |
| ASTM protocol (LIS V1.1 §3) | **Open:** not covered |
| Software-version choice | **Closed:** the documented installer build and Other version (not checked) |

## Evidence

Level: documented

The definition comes from the manuals only. The
[synthetic recordings](recordings/) use invented identifiers and values and
replay through the definition. The four uploads expect AA after durable
storage: each is held (missing OBR-8, missing OBX-3, or QC without a
measurement identity). Both plain barcode queries expect an AE when the lab
chose one-way. [Synthetic order replays](replay-orders.toml)
check a two-page barcode answer with a durable ACK per page, and an AE when
the cache is unavailable. Synthetic data does not establish interoperability
with an installed analyzer.

## Open questions

- Ask the lab for the PC software build and the LIS settings screen values:
  protocol, transmission mode, communication type, encoding, timeouts and
  the real-time upload checkboxes.
- Ask the lab app team for the Channel No. assignments and the specimen and
  unit spellings they expect.
- From the protected LIS log under `D:\iFlash3000\SystemLog` or a
  de-identified TCP capture, confirm these:
  - whether OBR-3 and OBR-8 are filled and how they behave across resends
    and reruns;
  - what OBR-25 holds;
  - how repeats, corrections and uncompleted tests look;
  - the QRF-1 value;
  - that the PC accepts Analyte's ACK fields;
  - the DSR reply IDs.

## Sources

- YHLO LIS Protocol Manual V1.1 (2018), §§1.1–1.3 and 2.1–2.4. The model list
  in §1.1 names iFlash 3000 variants. §3 (ASTM) is not used.
- YHLO Service Manual AF-14-iF10-001/A0 (2020), scope and §§6.3–6.4. It
  covers the iFlash 1200-A and iFlash 3000-M, installs the 1200 with the
  iFlash 3000 PC software and gives the LIS settings screen.
- [Repository research](../../../docs/research/machines/iflash-1200.md)
  and [HL7 protocol notes](../../../docs/protocols/hl7/README.md).
