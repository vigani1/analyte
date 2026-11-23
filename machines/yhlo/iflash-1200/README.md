# YHLO iFlash 1200

## Machine

This is a blocked machine definition, with TCP capture available through
[`record.toml`](record.toml). Use capture only to collect evidence. Normal operation needs vendor-specific handling
for result categories, qualitative results, and order replies. The iFlash
1200-A runs from an analyzer PC using iFlash 3000 software. Staff can enter a
tube barcode with a handheld scanner at the PC.

## Connection

For the TCP profile the iFlash PC initiates the connection. In **Utility →
System settings → LIS settings**, select TCP/IP and enter the Analyte
listener's address and port. Put that listener endpoint and the iFlash PC's
source IP in the site's `endpoint` and `allowed_peers` (ADR 0026). The
manual gives no fixed port. HL7 2.3.1 messages use CR segment endings and MLLP
with VT before the payload and FS CR after it (LIS V1.1 §§2.1–2.3).

The service manual's **Respond timeout limit** defaults to 15 s for the
analyzer's connection to the LIS server. Automatic reconnection is an
analyzer setting with a 30–60 s interval. Startup connection is another
analyzer setting. The manuals give no separate result ACK, QCK, DSR or
ACK^Q03 deadline. Use the installed settings and measured exchanges to
resolve those deadlines before enabling normal operation.

## Settings on the machine

- Choose one-way results or two-way results and queries on the analyzer
  (service §6.4.1). The future definition can use an ADR 0032 `mode` choice
  with `orders = "off"` and `orders = "on"`. Leave the suggestion empty
  so the lab supplies the setting. Both alternatives must validate before
  Setup can offer this choice.
- Record the encoding. LIS V1.1 §1.3 offers UTF-8 and an “ASCII” option
  that actually uses the analyzer PC's local code page. Confirm the
  emitted MSH-18 and high-byte behavior for the installed software.
- Record the selection for real-time uploads of completed and uncompleted
  tests. These switches independently trigger current-day uploads on
  a state change (service §6.4.2.1). The manual gives no matching wire-code
  definitions for uncompleted results, so there is no Setup choice for
  interpreting them.
- Keep the assay Channel No. list with the lab app team. The proposed
  `test_system` is `yhlo/iflash-1200` (ADR 0029). Pass codes through
  exactly as sent. Match outgoing orders to the configured channel codes.

## Supported workflows

| Documented wire behavior | Fields and handling |
| --- | --- |
| Patient result upload | ORU^R01 with MSH-16 = `0`, then PID, OBR and one or more OBX. OBR-2 is the tube barcode and OBR-3 is the analyzer sample counter. PID identifies the patient. OBR-10 is rack and position, OBR-9 reports manual dilution and OBR-15 carries specimen text. Preserve barcode case and leading zeros. LIS §§2.2.1, 2.3.3–2.3.5. |
| Observation | OBX-3 is the configured Channel No. and can be alphanumeric. OBX-5 holds the main value, OBX-6 the original unit, OBX-7 the reference range and OBX-8 flags such as L, H and N. OBX-9 holds the qualitative call and OBX-10 its reference text. ST can carry a call with OBX-5 empty. BOTH carries numeric and qualitative data. OBX-11 documents F for final. OBX-13 may report the raw result and OBX-14 the observation time. LIS §2.3.5. |
| QC upload | ORU^R01 with MSH-16 = `2`, containing OBR without PID or OBX. OBR-11 counts the controls. OBR-12–15 and OBR-17–20 hold parallel lists of control identifier, name, lot, expiry, level, mean, SD and measured value. Preserve the correspondence by index. LIS §§2.2.1.4, 2.3.4.2 and 2.4.1.2. |
| Result acknowledgment | ACK^R01 follows each ORU, including batches. MSA-2 references the inbound MSH-10. The vendor examples also echo MSH-10 and MSH-16 in the ACK and use MSA-6 for outcome details. AE or AR can trigger up to two retransmission attempts according to the manual. Analyte must durably keep the received bytes before AA and visibly refuse on storage failure. LIS §§2.2.1, 2.3.2 and 2.4.1. |
| Barcode query | QRY^Q02 uses QRD-8 for the tube barcode. The LIS responds QCK^Q02, then sends DSR^Q03 with numbered DSP rows carrying the sample and assays. The analyzer replies ACK^Q03. DSP row 21 carries the barcode, with channel entries starting at row 29. An unavailable lookup requires an error. NF means a trusted lookup found no matches. ACK^Q03 confirms message receipt. LIS §§2.2.2, 2.3.6–2.3.11 and 2.4.2. |
| Receipt and result identity | MSH-10 correlates the upload and its acknowledgment. OBR-3 and OBX-4 identify fields within the reported sample and observation context. The manual supplies no stable key and revision rule that distinguish retransmissions, reruns and corrections. Keep ambiguous result identity unresolved. LIS §§2.3.1–2.3.2 and 2.3.4–2.3.5. |

Record-only keeps TCP chunks before replying. It acknowledges durable result
receipt through the routing header even when MSH-16 carries the vendor
category, and sends an application error for a valid query.

## Departures

The capture profile has no clinical result mapping. A future normal definition
can declare the evidenced MSH-16 and categorized ORU result departures.
The remaining gaps below still block normal operation.

| Layer | Source | Gap and effect |
| --- | --- | --- |
| Transport and Setup | Service §§6.4.1–6.4.2, LIS §1.2 | Serial LIS is documented. The current HL7 stack uses TCP, and ADR 0032 cannot select a different link kind. Capture the selected serial protocol and cable settings before defining a serial profile. |
| Syntax and Setup | LIS §§1.3, 2.3.1 | The vendor's “ASCII” mode can use a local code page such as GBK. Analyte's ASCII, Latin-1 and UTF-8 choices cannot decode that mode generally. Confirm the emitted label against the existing scoped HL7 2.3.1 UTF-8 departure before enabling it. Encoding is outside the current ADR 0032 choice blocks. |
| Syntax and dialog | LIS 2.3.1-2.3.2, 2.4.1 | Reading the MSH-16 category is supported by an evidenced departure. The ACK builder uses its own MSH-10, omits the category echo and uses standard MSA fields. Confirm installed receiver tolerance and retry correlation. |
| Mapping | LIS 2.3.4.1, 2.3.5; service 6.4.2.1 | The existing sample selector reads OBR-2. The categorized layout reads rack and slot from OBR-10, preserves the OBR-9 dilution with unknown application, and keeps OBX-13 as source measurement evidence. It understands OBX-11 F. Incomplete transitions, report completion when OBR-25 is empty, and repeat/revision identity remain open. |
| Syntax and mapping | LIS 2.3.5 | The categorized layout supports BOTH, ST with an OBX-9 call, the OBX-10 qualitative reference and SI's three matched OBX-5/OBX-13 components. Ambiguous component layouts are held. |
| Mapping | LIS 2.3.1, 2.3.4.2, 2.4.1.2 | OBR-only control rows map only when the declared count and every column align. Control rows remain held without an established measurement identity. Calibration uploads remain held in raw form. |
| Syntax and dialog | LIS §§2.3.6–2.3.11, 2.4.2 | The standard handler accepts deferred display format `D`; iFlash sends `R` in QRD-2 and bare `RD` in QRD-7. The parser requires a numeric quantity and populated QRD-10, which YHLO leaves blank. The display encoder limits the units to display lines. It also emits an empty QAK tag, while YHLO specifies `SR`. The extension needs to preserve explicit cache failure as AE. |
| Encoding and order mapping | LIS §§2.3.10–2.3.11, 2.4.2 | The DSR requires positional DSP rows, then repeatable assay rows with component delimiters, plus DSC. The current builder rejects delimiters inside a DSP line, echoes QRD, generates distinct reply IDs and has no DSC output. YHLO's example clears QRD-8 in DSR and reuses the query ID in reply headers. Confirm which differences the installed PC enforces. |
| Dialog and order lifecycle | LIS §§2.2.3, 2.4.2.2–2.4.3 | The analyzer can query by sample number or collection-time range and receive multiple DSRs with continuation pointers and per-DSR ACKs. It also documents unsolicited DSR with MSH-15 = `P`. Current legacy HL7 supports a single barcode lookup and no vendor-style DSR send-ahead flow. Manual section 3 also describes a separate ASTM dialect. This TCP capture profile expects HL7 only. |
| Dialog timing | Service §§6.4.2–6.4.3, LIS §2.2 | Connection and reconnect settings are documented. Protocol phase deadlines and behavior after a lost reply are unconfirmed. A short synthetic event gap or the default connect timeout cannot establish a clinical setting. |

## Evidence

Level: documented

The six [synthetic recordings](recordings/) use invented identifiers and
measurements. The barcode queries expect ACK^Q02 with AE and no DSR in
record-only mode. The four uploads expect an acknowledgment of durable
receipt under ADR 0034. The included replays check the captured exchange and its durable receipt outcome.
Synthetic data does not establish interoperability with the installed analyzer.

## Open questions

- Get the exact model and PC software version from the lab. Record the LIS
  settings screen values for transport, direction, encoding, threshold and
  reconnect settings, and completed/uncompleted upload settings.
- Ask the lab app team for the actual LIS Channel No. assignments and the
  specimen and unit codes they expect to receive.
- Use the protected LIS communication log under
  `D:\iFlash3000\SystemLog` and a de-identified TCP capture to confirm
  response deadlines, ACK fields, result categories, partial uploads, repeat
  and correction behavior, qualitative values, QC arrays and dilution.
  Include a barcode query and its successful and unsuccessful replies.
- If the lab uses a serial cable, confirm the selected wire protocol and
  interface settings, including flow control.

## Result identity evidence

LIS Protocol Manual V1.1 section 2.3.4 gives candidate fields OBR-2
(sample barcode), OBR-3 (sample serial) and OBR-8 (observation end). Confirm
that the 1200 sends these fields and whether the end time distinguishes
successive measurements of the same assay.

## Sources

- YHLO LIS Protocol Manual V1.1 (2018), §§1.1–1.3, 2.1–2.4 and 3. The
  model list in §1.1 names iFlash 3000 variants.
- YHLO Service Manual AF-14-iF10-001/A0, scope and §§6.3–6.4. The manual
  covers the iFlash 1200-A and iFlash 3000-M, gives shared iFlash 3000 PC
  software instructions and references the YHLO HL7 interface in the LIS
  setup. That is the basis for applying V1.1 provisionally to the 1200-A.
  The installed software and wire details still need confirmation.
- [Repository research](../../../docs/research/machines/iflash-1200.md)
  and [HL7 protocol notes](../../../docs/protocols/hl7/README.md).
