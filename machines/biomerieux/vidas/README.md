# bioMérieux VIDAS

## Machine

Original five-section VIDAS (A–E) operated from a VIDAS PC. The host cable
connects to the PC's COM port. `machine.toml` is a full definition written
from the interface manuals (ADR 0034 amendment, 2026-11-26). It reads and
delivers uploaded results. Work-list downloads stay off (see
[Supported workflows](#supported-workflows)). No real analyzer has confirmed
it yet, so it ships at level documented.

## Connection

BCI Link Literal over RS-232 (guide C p. 2-2). The manufacturer's starting
settings are 9600 baud, 8 data bits, no parity and one stop bit (guide C
p. 3-8). The definition suggests these four. The guide gives COM3 as the PC's
default port and names no flow control. The lab selects the port on each end
and the flow control in Setup (ADR 0031).

With CR/LF disabled, the two modes share the same packet structure (guide
C pp. 3-1–3-7). ENQ opens the session and the receiver answers ACK if ready.
STX starts a packet, each RS starts a chunk of up to 80 printable ASCII bytes,
and GS introduces two ASCII hex checksum digits. The maximum data payload
is 1920 bytes. Add the bytes from the first RS through GS inclusive and
retain the low eight bits. STX and ETX are outside the sum. The PC writes
lowercase hex and accepts either case. ETX ends the packet and EOT ends the
session. Literal acknowledges immediately after the checksum, subject to
durable storage. Alternate waits for ETX. The PC opens a new session for each
uploaded result (guide C p. 3-3).

Deadlines are Analyte's engineering bounds, the same as the capture profile:
10 s per transport operation, 60 s per session and dialog wait, and 5 s for
acknowledgment, byte gap and commit. Guide C pp. 3-1–3-4 describes responses
and mentions retries but gives no numeric serial timeout or retry schedule.

## Settings on the machine

Confirm that the BCI link is enabled and that Upload is on (Manual F
pp. 10-11–10-14). Setup asks these questions (ADR 0032). Neither has a
suggestion: the manuals give no factory default for either.

### Setup questions

| Choice | Values | Where staff find it | Source |
| --- | --- | --- | --- |
| `acknowledgment` | `literal`: acknowledge after the checksum. `alternate`: acknowledge after ETX. | The protocol of the RS232 link in the BCI Link settings | Guide C pp. 3-2–3-4 |
| `software_version` | `manual-f`: software 4.0.0, sample ID up to 12 characters. `vu-06`: VU_06, sample ID up to 17 characters on the wire. | The VIDAS PC About screen (Manual F p. 8-54) | Manual F introduction, p. 4-14; guide C pp. 4-1, 4-5 |

The software version band also sets the sample-ID ceiling (`vidas.sample_limit`
12 or 17), so one answer decides both. Builds between Manual F and guide C
are not described by either source; choose the band whose sample-ID limit
the installed PC enforces, and report the version so a later band can cover
it. The VIDAS PC does not send its version on the wire, so Analyte cannot
cross-check the answer (ADR 0036).

Setup also asks the serial port and flow control, as for every serial
definition.

## Supported workflows

The documented upload is an ordered sequence of pipe-terminated, two-character
tags with message type `mt=rsl` (guide C pp. 4-2–4-9). The VIDAS mapper reads
the fixed fields below. Analyte retains original bytes alongside any mapped
text.

| Fields | Meaning and current behavior |
| --- | --- |
| `pi`, `si`, `ci`, `rt` | Patient ID, empty specimen separator, exact sample ID and short assay code (up to four characters). Sample IDs keep leading zeros and case. Assay codes pass through in the `biomerieux-vidas` test system; the lab app owns their names and no test list ships (ADR 0029). The PC assigns tube positions. |
| `ql`, `qn`, `y3` | Qualitative call, written test value and separately sent unit. Qualitative uploads can carry a numeric test value as well as the call and omit `y3`. Quantitative uploads can leave `ql` empty and place the comparator, number and unit together in `qn`. Each stays as sent; no unit is converted. |
| `qd` | PC dilution is an optional integer from 1 to 999. Analyte keeps the original and leaves the applied state unknown. |
| `nc` | Source flag for expired lot or calibration, missing controls, incomplete calibration, control-rule failures or validated calibration (guide C p. B-7; Manual F p. 10-8). The mnemonic stays unchanged. |
| `tt`, `td` | Completion time (`hh:mm`) and date (guide C p. 4-6). They form part of the result identity below. |
| `id`, `sn`, `m4` | Instrument ID, configured serial number and operator. These remain in the source message. |

The upload has no result status tag and no free-text comment tag. Controls
are marked only by the PC's reserved Cx and QCx identifiers
(`result_control_labels = "pc_slots"`); a calibration flag neither classifies
a QC sample nor grants clinical approval. An invalid interpretation is source
text (guide C appendix B).

### Result identity

The definition opts into the ADR 0035 recipe of sample ID, assay code and
completion date and time (guide C p. 4-6). The completion time has minute
precision. Two runs of the same sample and assay that finish in the same
minute have the same key, so Analyte cannot tell them apart:

- with different values, the second is held for an operator, never delivered
  as a correction;
- with identical values, the second counts as a resend and is not delivered
  again. Both receipts stay in the store.

VIDAS CMVA doses stay unresolved because the uploads lack a unique component
or run key. PC-side edits and retransmissions (Manual F chapter 10 and guide
C pp. 10-2–10-3) carry no revision marker; changed content under one key is
held.

### Work lists

Off. The definition sets `download = false` and declares no `[orders]`, and
Setup offers no work-list choice. A complete safe configuration is not
documented: the installed PC's enabled request fields, the assays that expect
a dilution and the operator's way to confirm that the PC accepted a request
are site facts the manuals leave open. The
[work-list recording](recordings/worklist-download.rec.jsonl) is a
protocol-level fixture: the replay test checks that the shipped definition
speaks the same stack and then replays it through the order-enabled VIDAS
test peer.

## Departures

No protocol departure is declared. The implementation gaps are listed below
so they can be fixed at their owning boundary.

### Gap report

Closed by this definition:

| Layer | Source | How it closed |
| --- | --- | --- |
| Result mapping | Guide C pp. 4-5 to 4-9 and appendix B | The definition reads uploads: exact `qn`, `ql`, `nc`, separate `y3` and explicit `qd`, with invalid calls and completion, instrument ID, serial and technologist exposed. |
| QC and identity | Guide C p. 4-6; ADR 0035 | The completion recipe resolves ordinary uploads instead of parking every one. The minute-precision limit is documented above. |
| Config | ADR 0032 | Literal or Alternate and the software band are Setup choices; the band carries the sample-ID ceiling. |
| Replay | ADRs 0009, 0032, 0034 | `record.toml` is gone. Every upload recording replays through the ready path with its choices in `replay.toml`. |

Still open:

| Layer | Source | Gap and consequence |
| --- | --- | --- |
| Serial transport | Guide C pp. 2-2, 3-8, 4-1; appendix A | The PC menu can offer baud rates outside Analyte's standard list, four data bits, forced parity and 1.5 stop bits. Analyte cannot select those combinations. Confirm an overlapping setting, the actual connector and cable wiring. |
| Framing | Guide C pp. 3-1, 3-6–3-7, C-4 | BCI can add CR/LF after selected control characters; Analyte requires one uniform suffix policy and ships none. Captures are needed to confirm the exact span with selective suffixes. |
| Dialog and custody | Guide C pp. 3-1–3-4 | BCI expects negative responses when the receiver cannot accept a session or a checksum or parity check fails. The [bad-checksum replay](recordings/literal-bad-checksum.rec.jsonl) expects a NAK after the rejected packet is stored. Readiness refusal, parity reporting and retry behavior still need verification on the installed link. Analyte never retries or replays orders. |
| Dialog and orders | Guide C pp. 3-2 to 3-4, 4-6, 10-2 to 10-3 | One ENQ/EOT session carries the assay packets for a single order. Each packet requires its own transport ACK. The PC can still refuse unknown assays or dilution settings, coalesce repeats, or prompt for demographic changes. Work lists stay off until this is confirmed. |
| Config and orders | ADR 0032, Guide C pp. 4-4 to 4-6, Manual F pp. 10-7 to 10-9 | Which assays expect a dilution is an installed setting (`vidas.dilution_required_assays`) the shipped definition cannot know. A lab enabling work lists also needs its patient namespace and enabled fields confirmed. |
| Syntax and encoding | Guide C pp. 4-1–4-8, B-1; Manual F pp. 10-7–10-8 | Analyte accepts the printable ASCII subset and fixed pipe/field conventions only. Guide C's ASCII instruction and its ISO-8859-1/UTF-8 codepage appendix need to be reconciled with actual captures. Manual F describes installed field selection, fixed width, case transformation, date format and separator settings. Required omissions or padded structural fields can fail parsing and park the receipt. Dates, times and dilution stay as text. |
| Result identity | Guide C p. 4-6; ADR 0035 | Minute precision cannot separate two runs in the same minute (see [Result identity](#result-identity)). Retransmission, rerun and correction behavior still needs captures. |
| QC | Guide C pp. 4-5, 4-8 to 4-9, B-7; Manual F ch. 10 | Reserved Cx labels split into name and strip lot and reserved QCx IDs classify as controls. `result_control_labels = "appendix_b"` stays off until evidence confirms that controls with other identifiers carry the exact Appendix B flags. |
| Transport and syntax | Guide C pp. 2-6 and chapters 6–9 | BCI Link also describes ASTM-derived XML over FTP for the VIDAS PC. Its file exchange and XML message schema are outside this serial stack. |

## Evidence

Level: documented

Eight synthetic recordings in `recordings/` cover quantitative, qualitative,
invalid and bad-checksum uploads in both modes. See the
[Literal quantitative](recordings/literal-quantitative.rec.jsonl) and
[Alternate bad-checksum](recordings/alternate-bad-checksum.rec.jsonl) cases.
A ninth covers a two-assay work-list download. Each file uses invented IDs,
assay codes and assay data. The deliberately damaged frames expect the
vendor's NAK. These are manual-derived cases with no hardware or official
simulator evidence. A de-identified recording from a real VIDAS PC raises the
model to proven.

## Open questions

- Read the installed VIDAS PC and BCI Link versions. Confirm the sample-ID
  ceiling, patient-ID limit and any identifier character restrictions.
- Confirm Literal or Alternate, active CR/LF switches, serial parameters
  including flow control, both connector genders and cable pinout. Check the
  configured codepage, case, date format, enabled fields and result-release mode.
- Capture a de-identified result of each type, a Cx and QCx control, an
  identical retransmission, a rerun and a correction. Check whether two runs
  can finish in the same minute and whether resends keep `tt` and `td`.
- If the lab uses work lists, confirm its patient namespace, enabled assay
  codes, dilution requirements and the operator's way to verify acceptance.

## Sources

Private reference documents, consulted locally. Sections and page numbers
refer to printed pagination.

- bioMérieux BCI Link LIS Interface Driver Development Guide 161150-486,
  rev A (2014), rev B (2017) and rev C (2020). Chapter 3 retains the same
  PC packet/checksum and ACK boundaries in all three revisions. Rev B
  updates network guidance and adds the VIDAS 3 CMVA behavior. The older
  two-dose PC behavior remains. Rev C adds BCI 4.2.1, FlexPro, VIDAS 3 TB-IGRA
  and FTPS coverage and removes the simulator chapter. Its changes do not
  establish a different Literal packet format for the original VIDAS PC.
- bioMérieux VIDAS PC User Manual F (2008), introduction, chapter 4,
  chapter 8 (About screen, p. 8-54), chapter 10 including the communication
  traces on pp. 10-24–10-26, and p. 12-18.
- [Repository research](../../../docs/research/machines/vidas.md) and
  [VIDAS protocol notes](../../../docs/protocols/vidas/README.md).
