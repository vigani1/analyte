# Dialab Autolyser

## Machine

Dialab Autolyser REF 20010D, made January 2020, operated from its own PC,
without a sample barcode reader. The lab must verify that the patient code
on the analyzer identifies the tube in its lab app and that the operator
loads the correct cup.

This is a blocked machine definition, with [record-only settings](record.toml)
for the first visit. A normal definition must supply a poll interval and choose
the report layout before it can load. Setup cannot yet collect those choices
for a normal Autolyser connection. The record-only profile can collect both
answers and keeps the exact bytes in protected storage.

## Connection

Use RS-232 from the analyzer's PC. Rev18 §23.4 p.179 shows a fully wired,
crossed 9-pin female–female cable, including handshake lines. Verify the
ports and cable on site. Use the separate host port on the PC. Keep COM
port, baud, data bits, parity, stop bits and flow control as lab values
without suggestions. Rev18 p.119 shows configurable baud and handshaking.
The serial utility settings in §23.6 apply to the utilities. Figure 15-7
shows a TCP/IP option, but its host interface is undocumented.

## Settings on the machine

In Utility / Setup Analyzer / Serial, confirm the **internal** protocol,
the serial settings, and whether **Send also the controls** is enabled.
Record the controls answer and the lab's polling interval in the site
file's `[recording]` section. The controls answer determines the layout
of every report. The capture profile supplies Analyte's own communication
deadlines. Rev18 §§23.1–23.2 sets no numeric poll, ACK, inter-byte or
response deadlines. Storage deadlines are local safeguards.

Record whether staff manually release reports through Utility / RS232 or
whether the analyzer releases them automatically. Release makes results
eligible for polling. Enable transmission of non-calculable (NC) results
so they remain visible to the host. With that option off, the analyzer
removes NC results from the outgoing queue (Rev18 p.119 and §23.2 p.177).
Preserve queued reports during qualification and recovery.

## Supported workflows

**Record only.** The host sends STX, waits for ACK, then sends `R` and EOT.
The analyzer replies with a report ending in EOT, or a single NAK for an
empty queue. Analyte keeps the reply before the next poll. There is no
receipt ACK from the host. Corrupt or incomplete input stops record-only
polling and leaves its cursor guard unresolved across restarts. An operator
must reconcile that guard before polling resumes. Rev18 §23.2 assigns
`L` to last-report recovery and `A` to restart retrieval at the first
available report. Archive retrieval requires an explicit operation and
is unavailable from this model's record-only connection.

**Internal report layout (Rev18 §§23.2–23.3, pp.177–179).** Widths are
in ASCII bytes. The basic header is patient code (15), list (1, `T`
routine or `R` STAT), sample medium (1, `S` serum or `U` urine), and
decimal result count (3). With controls enabled, insert the following
before the result count: control indicator (1, `Y` or `N`), known/unknown
(1, `K` or `U`), level (3) and lot (20). An ordinary sample in that
layout carries `N`, `K`, `001` and twenty spaces. Each result is an
exact four-byte analysis code and a seven-byte value. Keep case, leading
zeros and padding. The future definition's code namespace is
`dialab-autolyser` (ADR 0029). The lab app owns the test catalogue.

The implementation uses one three-digit decimal checksum after the last
pair and before EOT. It sums the payload's ASCII byte values modulo 256,
excluding the handshake, checksum and EOT. This span follows the consistent
examples and needs field verification. A report with `n` observations is
`24 + 11n` bytes, or `49 + 11n` with controls. The three-digit count
implies an envelope ceiling of 11,038 bytes, with controls, checksum and
EOT included. The manual does not establish how many tests the installed
tray can run or which textual forms and padding appear for NC. Keep the
seven bytes as text, including comparators, and keep the original wire
response. The internal format omits units, flags, ranges, timestamps,
dilution evidence and immutable run/revision identifiers.

**Work lists (Rev18 §23.1, pp.175–176).** The protocol driver can send one
patient per handshake, with the 15-byte code, list, sample medium, clone
(`Y`/`N`), two decimal cup digits, two decimal test-count digits, exact
four-byte analysis codes, checksum and EOT. The reply is `Y` and one
position byte, or `N` and one error byte (documented range 0x01–0x18).
Analyte requires an explicit app patient namespace, specimen code,
priority, clone policy and cup in 01–99. The driver accepts 1–99 tests
within its configured bound. A lost insertion reply leaves the outcome
uncertain for reconciliation. This model ships with record-only polling.

## Departures

### Gap report

| Rev18 source | Gap and consequence | Layer |
| --- | --- | --- |
| p.119 and §§23.1–23.2 | Normal definitions require a concrete poll interval and communication deadlines. ADR 0032 choices can switch the orders table, but cannot select `autolyser.controls`, `recover_last_on_poll` or `download`. Switching the orders table alone cannot express both states of the Autolyser download gate. The lab needs to select controls, normal work lists on/off and the interval without fabricated defaults. | Configuration / Setup |
| p.119, Figure 15-7 and §23.4 | TCP/IP exists on the analyzer, but the role, port and equivalence to RS-232 are unknown. A generic TCP transport is insufficient evidence. | Transport |
| p.119, §23.2 | Release and NC sending are analyzer settings. The bridge cannot inspect or set them on the wire or tell an unreleased queue from an empty one. Suppressed NC never reaches the bridge. | Analyzer / operations |
| §23 introduction p.175 | The manual describes stripping boundary control bytes and a fallback for an empty patient code. Analyte requires printable data and does not reproduce the fallback. Exact wire padding and patient-code use must be confirmed before matching samples. | Syntax / identity |
| §23.1 pp.175–176 | The analyzer can choose a position with `00`; Analyte requires an explicit cup. It preserves the reply's position byte without interpreting possible extra-list encodings. Routine and STAT are defined; a separate urgent list has no internal code. This protects placement on a machine with no reader. | Order mapping / dialog |
| §§22.6, 23.2 and §23.5 p.183 | The analyzer can recalculate or rerun results. Internal reports carry a patient code, but current mapping does not surface it as a resolved sample association. Neither report layout nor the listed variable fields yield a stable run or revision ID. Analyte parks results as unresolved, including after a successful wire recovery. | Mapping / result custody |
| §23.2 pp.177–178 | Current mapping preserves test/value text and classifies controls, but leaves list, medium, known-control status, level and lot in the parsed/raw evidence. It does not project those fields into a structured result. The printed controls example disagrees with the field layout and checksum; qualify both ordinary and QC reports in controls mode before relying on those offsets. | Syntax / mapping |
| §§23.2–23.3 pp.177–179 | The manual gives `L` as a recovery command but does not specify retry policy, exact timing or cursor persistence. The normal driver supports same-session automatic `L` when enabled and the dialog can accept an explicit `L`, but the program exposes no manual `L` or archive `A` operation. A restart with an unresolved poll requires operator reconciliation; record-only mode also requires that after a corrupt reply. | Dialog / operator controls |
| §23.5 pp.180–187 | The variable protocol is unavailable. It can change field order and widths, use constants and delimited strings, select repeated analysis fields, exclude string terminators from the checksum or omit checksum checking, and uses different list codes. It can carry patient details and notes, QC metadata, analysis names and kinds, paired concentrations/units/flags, conversion factor, ranges and 24-hour urine context. Its receiving script can also carry demographics, notes and control fields absent from internal work lists. Internal records cannot recover those fields, and unit tables cannot supply units that were never transmitted. | Framing / syntax / mapping |
| §23.6 p.188 | Vendor utilities illustrate communication, but do not establish the installed application's serial settings or timing. Analyte has no integration with those utilities. | Qualification |

The [protocol decisions](../../../docs/protocols/autolyser/decisions.md)
explain the selected checksum boundaries and the evidence needed to
revisit the controls layout. A normal connection also needs a replay
site for required choices. The current machine replay inventory
discovers only folders with `machine.toml` and supplies no Autolyser
choice answers. The record-only inventory requires `record.toml` and
forbids keeping both definitions side by side.

## Evidence

Level: documented

The [synthetic recordings](recordings/) cover a split next-report
response, an empty poll, a control report with synthetic NC padding,
and last-report recovery after checksum and length failures. The last
recording exercises the normal dialog's recovery, which is outside
record-only operation. There has been no analyzer or official simulator
run.

## Open questions

- Confirm the exact software version and whether it has an M suffix.
  Confirm the internal protocol, every serial setting, and which PC
  connector is the host port.
- Confirm the controls setting, how staff release results, whether NC
  transmission is enabled, and the lab's polling interval.
- Confirm patient-code entry, case/padding, and who checks the patient
  code against the tube and cup. For future work lists, confirm the
  desired on/off setting, clone policy, cup placement and extra-list use.
- Capture de-identified internal exchanges for release, empty queue,
  ordinary reports, QC and NC. Ask Dialab for the correct controls
  example, safe timing bounds, `L` across restart and any available
  run/revision identity. Confirm approved TCP/IP settings separately
  if the lab wants network operation.

## Sources

- Autolyser / Autolyser ISE Operator's Manual Rev18, software 21.18/21.18M.
  Revision history lists no LIS change from 21.7 through 21.18. See Serial
  setup p.119, §22.6 and chapter 23 pp.175–188.
- Autolyser Service Manual Rev01, software 14. Older background for the
  PC and host interface. Rev18 supplies the internal wire details here.
- [Machine research](../../../docs/research/machines/autolyser.md) and
  [protocol notes](../../../docs/protocols/autolyser/README.md).
