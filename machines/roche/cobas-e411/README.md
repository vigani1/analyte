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
| §5 pp. 14–15, §6 Tables 14, 16 and 18 | Config and framing | Normal definitions can now select the packing departure only in the cobas mode. Record-only already scopes packing by mode, but uses the shared ASTM capture frame limit and timers. Normal framing can declare the 247-byte limit through an evidenced departure. |
| §5 pp. 17–20; §3 Table 3 | Dialog and settings | The normal receive timer can be set through a departure. Roche describes its receiver's 15-second countdown from the last ACK/NAK, separate contention and busy retry delays, error counts, and optional recovery. The normal stack exposes aggregate send/receive limits, not the whole timing and recovery policy. Check host-side applicability and retry counts on the installed revision. A timeout can leave order acceptance uncertain. |
| §3 Table 3; §4 Tables 4–5; §6 p. 41 | Setup and order policy | Available choice blocks cannot distinguish disabling real-time inquiries from disabling all orders. Automatic upload, per-test STAT, exception filtering and trace settings are analyzer-owned. A per-test upload or a filtered upload cannot establish that an order is complete. |
| §6 Tables 14 and 16; §7 pp. 48–50 | Query and order dialog | **Closed for documented layouts:** both Q.3 shapes are read; O.4 echoes the original coordinates. Missing/generated barcodes never reach the cache. Carrier markers are preserved with reliable barcodes. **Open:** installed failure traces and exact deadlines. |
| §6 Tables 11–14 and 16; §7 pp. 48–50 | Order encoding | **Closed:** both real-time forms, O.26 and documented empty replies, plus the cobas batch form (HIM v2.3 p. 54). **Open:** the Elecsys batch wire details and installed behavior after a LIS02 Q error. |
| §6 Tables 14, 16–17; §7 | Order mapping and lifecycle | **Closed:** per-test dilution, 18-test limit, duplicate refusal, O.16 precedence and Q.13 A inquiry cancellation. Duplicate orders are disallowed by the analyzer from 02-04, and the bridge refuses them in both profiles. Confirmed transfer records an uncertain replacement until reconciled. **Open:** installed assay codes, calculated upload-only codes and application acceptance evidence. |
| section 6 Tables 16 and 18 | Mapping and result identity | **Closed for reading:** distinguishes patient labels from generated and control labels, preserving O.3. Confirm barcode or operator entry at the site. **Open:** reliable run evidence, especially for cobas mode. O.23 and the module identifier cannot distinguish successive measurements. |
| section 6 Table 18 | Status and identity mapping | **Closed for reading:** cobas mode preserves C as rerun evidence without treating it as a correction. Elecsys recognizes the explicit release and unavailable codes. **Open:** a qualified run identity recipe for each installed mode. |
| section 6 Tables 16–18 | Dilution and value mapping | **Closed:** both layouts retain the selector and predilution separately and read documented ratios. Whether the result already includes dilution remains unknown. No conversion is applied. |
| section 3 Table 3; section 6 Tables 18–20 | Value, QC and alarm mapping | **Closed for reading:** interprets quantitative values and optional message codes, qualitative codes and cutoff indices, the explicit absent value, and structured alarm numbers and transmitted text. Recognizes controls and preserves the control name in name-only mode. **Open:** the installed test-kind table and alarm text encoding. The manual disagrees on lot-mode O.3, so the complete label is held without splitting it. |
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
Analyte's record-only error. Two additional synthetic conversations replay
complete query replies in the documented cobas and Elecsys order profiles.
Analyzer acceptance and test-list replacement remain unverified. No hardware
or vendor simulator was used.

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
