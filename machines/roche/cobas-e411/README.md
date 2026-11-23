# Roche cobas e 411

## Machine

The Roche cobas e 411 has disk and rack models and two ASTM host protocol
types, Elecsys and cobas. This is a blocked machine definition, with
`record.toml` available for a record-only connection. Normal operation needs
the changes listed below and confirmation of the installed settings.

## Connection

The analyzer uses the RS-232C HOST port. Roche recommends 9600 baud, eight
data bits, no parity and one stop bit. Match the installed settings in Setup.
The cable drawing loops the analyzer's handshake pins locally. It does not
specify the host's flow-control setting, so Setup leaves that choice open.
See HIM v2.3 §§3 and 5, and HIM v2.2 B.

The analyzer uses ASTM E1381-91 framing. Each frame has up to 240 text bytes
(247 bytes including the envelope) and an uppercase hexadecimal checksum.
The instrument wins simultaneous ENQ. The manual describes a 1-second
contention delay, a 10-second retry delay after a non-ACK to ENQ, 15-second
establishment and frame waits, and a six-count error retry limit. The
15-second link timers do not specify the application query deadline. See
HIM v2.3 §5, pp. 14–20.

## Settings on the machine

| Setting | What staff confirm | What Setup can ask today |
| --- | --- | --- |
| Protocol type | Elecsys for separate records or cobas for optional packing and different fields | Record-only asks for the exact installed type. |
| RS-232C speed and character format | 4800, 9600 or 19200, and a supported bits/parity/stop combination | Record-only asks for the port and the five cable settings. |
| Automatic result upload | Whether the machine initiates real-time uploads | Machine-side only. |
| STAT upload | One upload per test or an upload after all tests finish | Machine-side only. |
| Automatic session recovery (cobas) | Whether the machine reconnects after an error and rejects messages during recovery | Machine-side only. |
| QC additional information (cobas) | Whether uploads include control name alone or include lot number | Machine-side only. |
| Send result only (cobas) | Suppresses real-time order inquiries. Batch transfer is still possible. | Full definition blocked. |
| Host test codes | Which codes are installed for the assays and generations in use | The lab app owns the code list. |
| Communication trace | Whether the machine keeps a trace for review | Machine-side only. |

See HIM v2.3 §3, Table 3. The separate batch upload option for results marked
by exception review affects which results arrive (§6, p. 41).

## Supported workflows

Record-only keeps the wire bytes and sends link acknowledgments after
storage. A query gets a LIS02 query error. Roche documents real-time and
batch patient and QC uploads, real-time patient order inquiries and batch
order downloads. It does not send calibration results or query the host for
QC orders. See HIM v2.3 §4, Tables 4–5.

HIM v2.3 §6 field positions below count the record type as field 1.

| Source | Elecsys | cobas |
| --- | --- | --- |
| Header | H.12 processing ID | H.5 sender and program version, H.10 receiver, H.11 purpose and real-time/batch subtype, H.12 processing ID, H.13 wire version |
| Sample and position | O.3 sample ID, O.4 components 1/2/3 sequence/carrier/position, 5/6 sample/container type | Same positions with cobas codes. A barcode error can generate an `@` ID. |
| Patient or QC | O.4.5 distinguishes SAMPLE and CONTROL. O.12 second repetition Q also marks QC. | O.4.5 QC and O.12 Q indicate QC. O.3 can include control and lot data. |
| Test namespace (ADR 0029) | R.3.4 carries the application number including generation. | First slash-delimited part of R.3.4 carries the site-assigned host code, which can name a calculated result. The lab app must distinguish the two code systems. |
| Dilution | R.3.5 is the dilution selector and R.3.6 the predilution indicator. | Second and third slash-delimited parts of R.3.4 carry those facts. |
| Result | R.4 value or qualitative code with cutoff index, R.5 unit, R.6 range, R.7 flag, R.9 status, R.12/R.13 times | R.4 value with optional message code or qualitative code with cutoff index, R.5 unit, R.7 flag, R.9 initial/rerun status, R.11 operator, R.14 module identifier |
| Query | Q.3 components 2–6, 8 and 9 carry ID and specimen context. | Q.3 components 3–7, 9 and 10 carry the same context in cobas form. |
| Order | O.3 ID, O.4 echo context, O.5 repeated test and dilution, O.6 priority, O.12 action and O.26 report type | Same destinations with cobas codes, plus O.16 specimen descriptor. |

### Implementation gaps

| Manual locator | Layer | What needs support and why |
| --- | --- | --- |
| §5 pp. 14–15, §6 Tables 14, 16 and 18 | Config and framing | A normal definition can select mappings by mode, but its packing departure and scope are fixed. Applying packing to Elecsys would accept invalid framing. Record-only already scopes packing by mode, but uses the shared ASTM capture frame limit and timers. Normal framing can declare the 247-byte limit through an evidenced departure. |
| §5 pp. 17–20; §3 Table 3 | Dialog and settings | The normal receive timer can be set through a departure. Roche describes its receiver's 15-second countdown from the last ACK/NAK, separate contention and busy retry delays, error counts, and optional recovery. The normal stack exposes aggregate send/receive limits, not the whole timing and recovery policy. Check host-side applicability and retry counts on the installed revision. A timeout can leave order acceptance uncertain. |
| §3 Table 3; §4 Tables 4–5; §6 p. 41 | Setup and order policy | Available choice blocks cannot distinguish disabling real-time inquiries from disabling all orders. Automatic upload, per-test STAT, exception filtering and trace settings are analyzer-owned. A per-test upload or a filtered upload cannot establish that an order is complete. |
| §6 Tables 14 and 16; §7 pp. 48–50 | Query and order dialog | The current Q reader only accepts a plain Q.3.2 specimen selector. Roche requires structured Q.3 in both modes and exact echo of sequence, carrier and position in O.4. This includes barcode-free queries, generated IDs and unknown carrier markers. Changed coordinates cause cancellation and skipped measurements. Do not use slot alone or a generated ID for patient matching. |
| §6 Tables 11–14 and 16; §7 pp. 48–50 | Order encoding | The generic header cannot write Roche purpose/subtype and version or the Elecsys header form. The encoder stops before O.26 and cannot select Roche reply/report types or Elecsys normal/negative response shapes. The generic Q error is safe as a record-only refusal. Its acceptance by this analyzer is unverified. |
| §6 Tables 14, 16–17; §7 | Order mapping and lifecycle | O.4 needs a structured echo. O.5 needs per-test dilution and mode-specific encoding. Roche specifies order-count and duplicate-test constraints, specimen-type precedence, generation substitution, and calculated codes for uploads only. The current placement knobs and pass-through descriptions do not enforce these. Roche downloads can replace an existing test list. Link ACK confirms receipt at the link, so application acceptance and replacement must be established separately. Q.13 A cancels the inquiry, not an already-issued test order. |
| §6 Tables 16 and 18 | Mapping and result identity | O.3 can be a barcode, a typed ID, a generated ID or a QC label. Fixed selectors cannot express those conditional meanings. Physical position, O/R record counters, and the cobas module identifier do not establish a unique measurement. Elecsys measurement times are evidence, but cobas lacks those R timestamps and O.23 is sample-level. Keep ambiguous repeats unresolved. |
| §6 Table 18 | Status and identity mapping | Roche calls cobas R.9 F the initial result and C a rerun. Generic ASTM maps C to corrected, even when the correction identity list is empty. Elecsys also uses release/block and communication states whose meanings differ from generic LIS02. A status policy and separate run evidence are needed to avoid overwriting the previous result. The manual gives no correction code. |
| §6 Tables 16–18 | Dilution and value mapping | The slash splitter can retain cobas dilution text. Elecsys places the evidence in separate caret components and uses a selector-to-ratio table. Predilution and selected ratio cannot establish whether a reported value already includes multiplication. Preserve the values and evidence without conversion. |
| §6 Tables 18–20 | Value, QC and alarm mapping | R.4 needs mode- and test-aware handling of qualitative code, cutoff index, quantitative message code and a seven-space absent result. Site code lists are needed to distinguish numeric from coded results. C.4 carries an alarm number and text with dialect-specific codes. Generic comments preserve text but do not provide a structured alarm. The QC equality selector can recognize QC. The model lacks typed control-name/lot separation and the manual's §3 vs §6 descriptions disagree on the lot-mode O.3 representation. |
| §6 Table 16 and §7 | Syntax and site evidence | Resolve Elecsys O.7 versus O.8 request-time placement, the advertised transition to S0 in cobas queries, and dialect/firmware-specific text encoding for alarms. Preserve raw bytes and times with their source positions until those meanings are confirmed. |

## Departures

`record.toml` declares ADR 0015 `astm_packed_records_in_frame` at documented
level for inbound cobas mode only, with unknown firmware covered by HIM v2.3
§5 p. 14 and HIM v2.2 D §1. Valid packed frames are stored and ACKed in
cobas mode. Elecsys mode rejects packed frames. Invalid checksums are
rejected and retained in either mode.

## Evidence

Level: documented

Synthetic ADR 0009 recordings cover both upload types, QC, an initial result
followed by a rerun, bad checksum and retry, and a Roche query followed by
Analyte's record-only error. The query fixture does not demonstrate an order
download or analyzer acceptance. No hardware or vendor simulator was used.

## Open questions

- Confirm the chassis, installed software and matching manual/errata.
- Read the installed settings and host test-code table, including active assay
  generations, units and quantitative/qualitative value types.
- Collect de-identified traces for barcode read failure, barcode-free orders,
  both QC settings, manual resends and reruns, and complete real-time and
  batch order exchanges. Confirm the total query deadline and safe
  failure behavior with the installed software.
- Confirm how the lab app assigns order and result identities, which units
  have explicit mappings, and whether any published value is dilution-adjusted.

## Result identity evidence

HIM v2.3 section 6, Table 18 supports a candidate Elecsys recipe using a
confirmed sample ID, assay and R.13 completion time. In cobas mode, configure
R.9 C as a rerun when the full recipe can be qualified. C identifies a rerun
status, and a separate source field is still needed to tell successive reruns
apart. The manual gives no correction code for this mode.

## Sources

- Roche cobas e 411 Host Interface Manual v2.3 (March 2016), §§3–7.
- Roche cobas e 411 Host Interface Manual v2.2 (November 2013), sections B–F.
- CLSI LIS01-A2 §§3, 6 and LIS02-A2 §§5–12 for the Analyte baseline.
- Roche Immunoassays test menu, for assay context. Installed host codes must
  come from the analyzer.
- [Background research](../../../docs/research/machines/cobas-e411.md) and
  [ASTM policy](../../../docs/protocols/astm/decisions.md).
