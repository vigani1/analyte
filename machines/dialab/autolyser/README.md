# Dialab Autolyser

## Machine

Dialab Autolyser REF 20010D, made January 2020, operated from its own PC,
without a sample barcode reader. The lab must verify that the patient code
on the analyzer identifies the tube in its lab app and that the operator
loads the correct cup.

This is a full [machine definition](machine.toml) written from the Operator's
Manual Rev18 under the 2026-11-26 amendment to ADR 0034. It reads results:
each report's patient code, analysis codes and values reach the lab app, held
for staff because the report carries no run identity (see Departures). It is
not blocked any more, but it has never run against an installed analyzer,
so its evidence stays `documented`.

## Connection

Use RS-232 from the analyzer's PC. Rev18 §23.4 p.179 shows a fully wired,
crossed 9-pin female–female cable, including handshake lines. Verify the
ports and cable on site. Use the separate host port on the PC. The
definition suggests no COM port, baud, data bits, parity, stop bits or flow
control: the manual does not state the installed values, so the lab chooses
all five in Setup from what the analyzer's Serial screen shows (ADR 0031).
Rev18 p.119 shows configurable baud and handshaking. The serial utility
settings in §23.6 apply to the utilities. Figure 15-7 shows a TCP/IP
option, but its host interface is undocumented, so only serial is offered.

## Settings on the machine

In Utility / Setup Analyzer / Serial, confirm **Present**, the **internal**
protocol and the serial settings. The variable protocol is refused (Rev18
§23.5; see Departures). Rev18 §§23.1–23.2 set no numeric poll, ACK,
inter-byte or response deadlines, so the definition carries Analyte's own
bounds (5 s acknowledgment, 10 s response, 5 s between bytes, 5 s storage).

Decide whether staff release reports through Utility / RS232 ("Accept
result to be sent") or whether "All results must be sent automatically" is
on. Release makes results eligible for polling; Analyte cannot tell an
unreleased queue from an empty one. Turn on "Send the results with <NC>
instead of the value" so non-calculable results reach the host. With that
option off, the analyzer deletes NC results from the outgoing queue (Rev18
p.119 and §23.2 p.177).

### Setup questions

Setup step 2 asks these for every Autolyser. None has a suggestion: only the
lab can answer them.

| Setup question | Values | Where staff find the answer |
| --- | --- | --- |
| Seconds between polls (`autolyser_poll_seconds`) | whole seconds, 1–300 | Lab policy. Rev18 §23.2 gives no interval. |
| Send also the controls (`controls`) | `off`, `on` | Autolyser PC: Utility > Setup Analyzer > Serial, "Send also the controls" (Rev18 p.119). |
| Last report after damage (`recovery`) | `ask-last`, `stop` | Lab policy on Analyte. Rev18 §23.2 offers `L` but sets no retry rule. |
| Serial cable (`[serial]`) | the five standard settings | Autolyser PC: Utility > Setup Analyzer > Serial, Baud-Rate and Hand-Shake (Rev18 p.119). Confirm data bits, parity and stop bits with Dialab. |

There is no software-version question. The Autolyser does not send its
version on the wire, and Rev18 lists no host-interface change from 21.7
through 21.18. The `M` suffix marks the open-parameters build (Rev18 §7.3 p.38);
it does not change the host interface.

## Supported workflows

**Result polling.** The host sends STX, waits for ACK, then sends `R` and
EOT. The analyzer replies with a report ending in EOT, or a single NAK for
an empty queue. Analyte stores the reply before it reads it. There is no
receipt ACK from the host. Before each `R` Analyte stores a poll guard. A
restart with an unresolved guard needs an operator to reconcile it before
polling resumes.

With `recovery = "ask-last"`, a corrupt or incomplete reply makes Analyte
ask for the last report (`L`) at the next poll. `R` and `A` stay blocked
until a valid `L` reply arrives, and an empty `L` reply does not clear the
block. With `recovery = "stop"`, polling stops until staff reconcile.
Analyte never asks for the archive (`A`), and the program has no manual
`L` or `A` operation (Rev18 §23.2).

**Internal report layout (Rev18 §§23.2–23.3, pp.177–179).** Widths are
in ASCII bytes. The basic header is patient code (15), list (1, `T`
routine or `R` STAT), sample medium (1, `S` serum or `U` urine), and
decimal result count (3). With controls enabled, insert the following
before the result count: control indicator (1, `Y` or `N`), known/unknown
(1, `K` or `U`), level (3) and lot (20). An ordinary sample in that
layout carries `N`, `K`, `001` and twenty spaces. Each result is an
exact four-byte analysis code and a seven-byte value.

The implementation uses one three-digit decimal checksum after the last
pair and before EOT. It sums the payload's ASCII byte values modulo 256,
excluding the handshake, checksum and EOT. This span follows the consistent
examples and needs field verification. A report with `n` observations is
`24 + 11n` bytes, or `49 + 11n` with controls. The three-digit count
implies an envelope ceiling of 11,038 bytes, with controls, checksum and
EOT included, which is the definition's frame limit.

**Mapping.**

- The 15-byte patient code becomes the sample ID exactly as sent: case,
  leading zeros and trailing padding are kept. A blank code, the analyzer's
  `{0}` empty-code marker, or a control report leaves the sample absent.
- Analysis codes are the machine's own four bytes, padding included, in the
  code system `dialab-autolyser` (ADR 0029). The lab app owns the test
  names; the definition ships no test list.
- Values stay as written, including comparators and NC text. The report
  carries no units, so no unit is mapped or supplied.
- Each result keeps the list (`T`/`R`) and the sample medium (`S`/`U`). A
  control report also keeps known/unknown, the exact level and the lot. In
  ordinary reports the control placeholders stay only in the raw receipt.
- The original response is kept with the driver and definition version
  that read it.

**Work lists (Rev18 §23.1, pp.175–176).** Off. The definition sets
`download = false` and has no `[orders]`. The protocol and mapping code
still refuse a missing or `00` cup, accept 1–99 tests and refuse urgent
lists; a lost insertion reply stays uncertain. Turning work lists on needs
the lab app's patient namespace and a clone policy (see Departures).

## Departures

No typed departures are declared (`deviations = []`).

### Gaps closed by this definition

| Rev18 source | Was | Now |
| --- | --- | --- |
| p.119, §§23.1–23.2 | Setup could not ask the poll interval or the controls layout for a normal connection, so the model was record-only. | The definition asks the poll interval (1–300 s, no default), the controls setting and the recovery policy in Setup. |
| §23 introduction p.175, §23.2 | Reports were stored but never read. | The patient code, analysis codes, values, list, medium and QC details are mapped and delivered, held for staff. |
| §§23.2–23.3 | The normal dialog's `L` recovery could not be enabled from a shipped model. | `recovery = "ask-last"` enables same-session `L`; `stop` keeps operator reconciliation. |

### Gaps still open

| Rev18 source | Gap and consequence | Layer |
| --- | --- | --- |
| §§22.6, 23.2 pp.177–178 and §23.5 p.183 | The analyzer can recalculate or rerun results, but the internal report and the documented variable fields have no stable run or revision ID. No identity recipe is declared, so every result stays unresolved and held for staff (ADR 0035). | Mapping / result custody |
| §23.1 pp.175–176 | Work lists need the lab app's patient namespace and an explicit clone policy. Setup cannot ask either for an Autolyser, and the definition cannot know them, so download is off with no choice. A follow-up adds a site namespace (as ADR 0032 decision 9 did for VIDAS) and a clone choice. | Configuration / Setup |
| p.119, Figure 15-7 and §23.4 | TCP/IP exists on the analyzer, but its role, port and equivalence to RS-232 are unknown. A generic TCP transport is insufficient evidence. | Transport |
| p.119, §23.2 | Release and NC sending are analyzer settings. The bridge cannot inspect or set them on the wire or tell an unreleased queue from an empty one. Suppressed NC never reaches the bridge. | Analyzer / operations |
| §23 introduction p.175 | The patient code is the sample association. Recordings are still needed to confirm which code staff enter and how it is padded. | Syntax / identity |
| §23.2 pp.177–178 | Parsing follows the stated field widths because the printed controls example disagrees with the widths and checksum. Both report modes still need qualification on the installed analyzer. | Syntax / mapping |
| §§23.2–23.3 pp.177–179 | The manual gives no retry policy, timing or cursor persistence for `L`. The program has no manual `L` or archive `A` operation. | Dialog / operator controls |
| §23.5 pp.180–187 | The variable protocol is refused. It can change field order and widths, use constants and delimited strings, select repeated analysis fields, change or omit the checksum and use different list codes. It can carry patient details, notes, QC metadata, analysis names, units, flags, ranges and 24-hour urine context that the internal layout lacks. Unit tables cannot supply units that were never sent. | Framing / syntax / mapping |
| §23.6 p.188 | Vendor utilities illustrate communication but do not establish the installed application's serial settings or timing. Analyte has no integration with those utilities. | Qualification |

The [protocol decisions](../../../docs/protocols/autolyser/decisions.md)
explain the checksum boundaries, the controls layout and the recovery rules.

## Evidence

Level: documented

The [synthetic recordings](recordings/) replay through this definition with
the answers in [replay.toml](replay.toml): a split next-report response, an
empty poll, a control report with synthetic NC padding, and a corrupt reply
followed by last-report recovery. The work-list recording replays against
the order-enabled synthetic Autolyser example, because this definition keeps
work lists off. There has been no analyzer or official simulator run.
Shared recordings from an installed analyzer can raise the level to `proven`.

## Open questions

- Confirm the exact software version and whether it is the `M` build.
  Confirm the internal protocol, every serial setting, and which PC
  connector is the host port.
- Confirm the controls setting, how staff release results, whether NC
  transmission is enabled, and the lab's polling interval.
- Confirm patient-code entry, case/padding, and who checks the patient
  code against the tube and cup. For future work lists, confirm the
  lab app's patient namespace, clone policy, cup placement and extra-list use.
- Capture de-identified internal exchanges for release, empty queue,
  ordinary reports, QC and NC. Ask Dialab for the correct controls
  example, safe timing bounds, `L` across restart and any available
  run/revision identity. Confirm approved TCP/IP settings separately
  if the lab wants network operation.

## Sources

- Autolyser / Autolyser ISE Operator's Manual Rev18, software 21.18/21.18M.
  Revision history lists no host-interface change from 21.7 through 21.18.
  See the open/closed parameters notice in §7.3 p.38, Serial setup p.119, §22.6 and
  chapter 23 pp.175–188.
- Autolyser Service Manual Rev01, software 14. Older background for the
  PC and host interface. Rev18 supplies the internal wire details here.
- [Machine research](../../../docs/research/machines/autolyser.md) and
  [protocol notes](../../../docs/protocols/autolyser/README.md).
