# Roche cobas e 411

## Machine

The disk and rack versions use the same serial host interface. The analyzer
offers two ASTM protocol types. Elecsys sends at most one record per frame
and provides a per-test completion time. cobas can pack records and sends
site-assigned host test codes. This definition is for result reception.

## Connection

Use the analyzer's RS-232C HOST port. Roche recommends 9600 baud, eight
data bits, no parity and one stop bit. The cable drawing loops the
analyzer's handshake pins locally. Confirm the host flow-control setting
against the installed cable and host configuration. See HIM v2.3 §3,
Tables 1–3 and the cable drawing, and HIM v2.2 §B.

The stack uses LIS01 framing and link dialog and LIS02 records for the
analyzer's E1381-91 and E1394-era interface. It accepts up to 240 text bytes
per frame, uses uppercase checksum digits and yields to the analyzer on
simultaneous ENQ. The host send and receive bounds are 15 seconds. Roche
describes separate busy and contention retries and an optional analyzer
recovery setting. See HIM v2.3 §5, pp. 14–20.

## Settings on the machine

### Setup questions

| Setup setting | Where to read it |
| --- | --- |
| Protocol type and QC additional information | On the analyzer, open Utility > System > Host Communication Setting > Condition. Read Protocol type. For cobas, also read QC additional information and choose Control name or Lot No. |
| Serial speed, data bits, parity and stop bits | In the same Condition window, match RS232C Speed and Frame. |
| Serial flow control | Confirm from the installed HOST cable and host serial configuration. The analyzer's documented menu has no flow-control field. |

HIM v2.3 §3 recommends Elecsys and leaves the protocol selection to the
installation. The definition requires an answer. Automatic upload, STAT
upload by sample or test, communication trace, automatic recovery and
Send result only are analyzer settings. Confirm them on the same screen.
On the analyzer, Send result only suppresses real-time inquiries while
batch transfer remains possible. See HIM v2.3 §3, Table 3, §4, Tables 4–5,
and §6, p. 41 for exception filtering.

Both HIM revisions name analyzer software 02-04 only in the note about
duplicate orders. They describe one communication-program token, 1, in
cobas H.5.2. Analyte checks that token and shows unknown when Elecsys omits
the field. The token identifies the host communication program. An
analyzer software Setup choice needs a second documented band.

## Supported workflows

Normal connections interpret patient and QC uploads and retain the raw
wire data. Outbound orders and query replies are disabled. When an inquiry
arrives, Analyte journals and acknowledges the frames and sends no
application reply. Roche also documents real-time and batch patient and QC
uploads, real-time patient inquiries and batch order downloads. Calibration
results and QC order queries are outside these workflows. See HIM v2.3
§4, Tables 4–5.

Ready models use the shared ASTM profile in record-only connections
(ADR 0034). That profile stores chunks and refuses a packed frame with
NAK. A normal cobas connection accepts verified packed frames.

### Gaps closed by this definition

Field positions count the record identifier as position 1. See HIM v2.3 §6.

| Manual locator | What the definition handles |
| --- | --- |
| §5, pp. 14–20 | Both modes use the documented frame size and host receive bound. Only cobas allows packed inbound records. |
| Tables 16 and 18 | Sample ID is O.3.1. O.4.5 retains the source specimen code and marks SAMPLE/CONTROL or cobas patient/control types. Order action O.12 also marks QC. QC labels and generated IDs are held without making them patient IDs. |
| Table 18 | Elecsys uses the application number in R.3.4, including any generation digits sent. cobas uses the first slash part of R.3.4, which can identify a calculated result. The respective code systems are `roche.cobas-e411.elecsys-application` and `roche.cobas-e411.host-test-code`. The lab app owns assay and specimen names. |
| Tables 17–18 | Elecsys reads dilution and predilution in R.3.5–6. cobas reads the remaining slash parts of R.3.4. Whether the analyzer already applied dilution remains unknown. |
| Table 18 | The exact R.4 value, R.5 unit, R.6 range where present, R.7 flags and R.9 status are kept. No unit conversion is configured. Elecsys R.12–13 and cobas O.23 are retained as source times. O.23 describes sample-level reporting. |
| Table 18 and ADR 0035 | The Elecsys identity requires the exact sample ID, test code and per-test R.13. The cobas R.9 C marks a rerun. Both initial cobas results and reruns are held for review when no per-measurement run key is available. |
| Tables 19–20 | C.4 carries the alarm number and Elecsys can include transmitted text. The raw comment is kept. |
| Table 11 | cobas H.5.2 is checked against the documented program token. H.13 is the wire-format version. |

### Gaps still open

| Manual locator | What still needs evidence |
| --- | --- |
| §3, Table 3 and §6, Tables 16 and 18 | Confirm the lab's exact host code table and which assays report quantitative or qualitative data. An ambiguous cobas value with a second component is kept for review without a confirmed test kind. The manuals describe the lot-mode QC label inconsistently, so the full label is retained unsplit. |
| §5, pp. 17–20 | Confirm installed behavior for retry delays, counts and automatic session recovery. The shared link bounds do not model every Roche timer. |
| §6, Table 18 | cobas omits R.13. O.23 and the module ID cannot establish separate runs. The manual gives no cobas correction code. |
| §6, Tables 16 and 19 and §7 | Confirm Elecsys O.7 versus O.8 request time, the version that changes cobas query specimen types to S0, and alarm text encoding. |
| §4, Tables 4–5, §6, Tables 11–17, and §7 | Confirm analyzer acceptance of host replies, exact query deadlines, handling of LIS02 query errors, and Elecsys batch download. The documented structured real-time order layouts have synthetic replay coverage only. |

## Departures

The definition declares `astm_frame_limit` (247 bytes, HIM v2.3 §5 Table
6 and HIM v2.2 §D p. D-3) and `astm_receive_timeout` (15 seconds,
HIM v2.3 §5 p. 20 and HIM v2.2 §D Transfer Phase) in every mode. Only
the two cobas profiles declare `astm_packed_records_in_frame` (HIM
v2.3 §5 p. 14 and HIM v2.2 §D.1 p. D-2). The departures are documented,
for inbound ASTM result connections, with the selected mode and firmware
coverage stated in the definition. A bad checksum still receives NAK.

## Evidence

Level: documented

Synthetic replays through the normal stack check Elecsys upload and
delivery, cobas packed uploads, held control and rerun readings, bad-checksum
rejection and retransmission, and query silence with orders disabled. The
two structured-query recordings replay exact replies and cancellation
with test-only order overrides. The supported communication-program token
for cobas is 1 in both manual revisions. There are no hardware or
vendor-simulator recordings.

## Open questions

- Confirm the installed software, host-code and specimen settings, along
  with the analyzer's alarm text encoding.
- Collect de-identified conversations for barcode failures, barcode-free
  orders, both QC label settings, manual resends and reruns, and order
  exchanges. Confirm whether cobas provides any separate run evidence.
- Confirm whether published values already include dilution and whether the
  Elecsys completion time distinguishes successive runs on the installed unit.

## Sources

- Roche cobas e 411 Host Interface Manual v2.3 (March 2016), §§3–7.
- Roche cobas e 411 Host Interface Manual v2.2 (November 2013), §§B–F.
- CLSI LIS01-A2 §§3, 6 and LIS02-A2 §§5–12 for the Analyte baseline.
- [Background research](../../../docs/research/machines/cobas-e411.md) and
  [ASTM policy](../../../docs/protocols/astm/decisions.md).
