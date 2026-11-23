# bioMérieux VIDAS

## Machine

Original five-section VIDAS (A–E) operated from a VIDAS PC. The host cable
connects to the PC's COM port. This is a blocked machine definition, with a
record-only profile for gathering evidence. Every normal upload currently
parks with unresolved result identity. The link also has settings the
normal loader cannot represent as site choices. Shipping an active
definition would offer Setup a connection that cannot deliver these results.

## Connection

BCI Link Literal over RS-232. The manufacturer's starting settings are
9600 baud, 8 data bits, no parity and one stop bit (guide C p. 3-8). The PC
guide gives COM3 as its default. The lab selects the actual port on each
end and confirms flow control, for which the guide gives no setting. The
record-only profile suggests the four documented cable settings and leaves
flow control to Setup (ADR 0031).

With CR/LF disabled, the two modes share the same packet structure (guide
C pp. 3-1–3-7). ENQ opens the session and the receiver answers ACK if ready.
STX starts a packet, each RS starts a chunk of up to 80 printable ASCII bytes,
and GS introduces two ASCII hex checksum digits. The maximum data payload
is 1920 bytes. Add the bytes from the first RS through GS inclusive and
retain the low eight bits. STX and ETX are outside the sum. The PC writes
lowercase hex and accepts either case. ETX ends the packet and EOT ends the
session. Literal acknowledges immediately after the checksum, subject to
durable storage. Alternate waits for ETX. The PC opens a new session for each
uploaded result.

## Settings on the machine

Confirm that the BCI link is enabled. Read the VIDAS PC software version on
the About screen and the BCI Link version and link settings in the
configuration. When the model is ready, Setup needs these choices (ADR 0032).

| Choice | Values | Source |
| --- | --- | --- |
| Acknowledgment | Literal or Alternate. Match the installed setting. | Guide C pp. 3-2–3-4 |
| Software version | Manual F profile, documented with software 4.0.0 and a 12-character entry limit, or guide C VU_06 profile with a 17-character wire limit. No suggestion. Confirm the profile for builds between these sources. | Manual F introduction, pp. 4-14, 12-18; guide C pp. 4-1, 4-5 |
| Direction | Results only or explicit work-list downloads as well. Analyte has a one-request download path. Keep downloads off until the site settings and acceptance workflow are confirmed. | Guide C pp. 3-2–3-4, 4-2, 10-2; Manual F chapter 10 |

Record only asks for the acknowledgment mode and serial settings now.
It retains bytes without attempting delivery or sending work lists (ADR 0034).
The shipped capture profile uses Analyte's 10 s transport-operation and
60 s session bounds and 5 s acknowledgment, byte-gap and commit bounds.
These are engineering limits. Guide C pp. 3-1–3-4 describes responses and
mentions retries but gives no numeric serial timeout or retry schedule.
A/B simulator retry examples are script arguments. The normal loader
requires retry count, backoff and deadlines and offers no site choice for
them. Confirm the installed behavior before qualifying a normal profile.

## Supported workflows

The documented upload is an ordered sequence of pipe-terminated, two-character
tags with message type `mt=rsl` (guide C pp. 4-2–4-9). The parser owns the
following fixed fields. Analyte retains original bytes alongside any mapped text.

| Fields | Meaning and current behavior |
| --- | --- |
| `pi`, `si`, `ci`, `rt` | Patient ID, empty specimen separator, exact sample ID and short assay code (up to four characters). The lab app owns the assay catalogue (ADR 0029). A normal definition would declare `biomerieux-vidas` as its `test_system`. The PC assigns tube positions. |
| `ql`, `qn`, `y3` | Qualitative call, written test value and separately sent unit. Qualitative uploads can carry a numeric test value as well as the call and omit `y3`. Quantitative uploads can leave `ql` empty and place the comparator, number and unit together in `qn`. Keep each as sent. |
| `qd` | PC dilution is an optional integer from 1 to 999. Analyte keeps the original and leaves the applied state unknown. |
| `nc` | Source flag for expired lot or calibration, missing controls, incomplete calibration, control-rule failures or validated calibration. Guide C p. B-7 identifies these as `lotexp`, `calexp`, `contneff`, `calinc`, `conthwst`, `conthnor` and `valid`. Keep the mnemonic unchanged. |
| `tt`, `td`, `id`, `sn`, `m4` | Completion time and date, instrument ID, configured serial number and operator. These remain in the source message. They do not establish a unique run or revision. |

An invalid interpretation is source text (guide C appendix B). A calibration
flag neither classifies a QC sample nor grants clinical approval. The guide
describes PC-side edits and retransmission (Manual F chapter 10 and guide C
pp. 10-2–10-3). Those do not supply a Literal result key or revision marker.
Analyte parks ambiguous receipts and does not reconstruct runs or revisions
from sample, assay, minute of completion or value (ADR 0003).

## Departures

No protocol departure is declared. The current implementation gaps are
listed below so they can be fixed at their owning boundary.

### Gap report

| Layer | Source | Gap and consequence |
| --- | --- | --- |
| Serial transport | Guide C pp. 2-2, 3-8, 4-1; appendix A | The PC menu can offer baud rates outside Analyte's standard list, four data bits, forced parity and 1.5 stop bits. Analyte cannot select those combinations. Confirm an overlapping setting, the actual connector and cable wiring. |
| Framing | Guide C pp. 3-1, 3-6–3-7, C-4 | BCI can add CR/LF after selected control characters; Analyte requires one uniform suffix policy. The unsuffixed checksum is clear. Captures are needed to confirm the exact span with selective suffixes. |
| Dialog and custody | Guide C pp. 3-1–3-4 | BCI expects negative responses when the receiver cannot accept a session or a checksum or parity check fails. Analyte stops on a framing error without sending the documented NAK. The bad-checksum fixtures expose this. Verify readiness refusal, parity reporting and retry behavior on the actual link. Retry configuration never replays orders. |
| Dialog and orders | Guide C pp. 3-2–3-4, 3-9, 4-6, 10-2–10-3 | Analyte opens a separate session per validated request. It cannot batch packets under one ENQ/EOT, identify installed main vs sub-assay codes, or learn that a work-list entry was created or accepted from its transport ACK. The PC can reject unknown codes or dilution settings, silently coalesce repeat requests, or prompt for a demographic change. |
| Config and orders | ADR 0032; guide C pp. 4-4–4-6; Manual F pp. 10-7–10-9 | The mode and sample-length selectors exist. The orders selector can turn off a complete work-list configuration, but its enabled branch still needs a lab-specific patient namespace, which the choice schema cannot supply. The outbound mapper emits neither optional demographics and origin nor configured dilution. A quantitative order can be refused if the instrument requires dilution. |
| Syntax and encoding | Guide C pp. 4-1–4-8, B-1; Manual F pp. 10-7–10-8 | Analyte accepts the printable ASCII subset and fixed pipe/field conventions only. Guide C's ASCII instruction and its ISO-8859-1/UTF-8 codepage appendix need to be reconciled with actual captures. Manual F describes installed field selection, fixed width, case transformation and separator settings. Required omissions or padded structural fields can fail parsing. Dates, times and dilution stay as text without validating the installed date format or dilution range. |
| Result mapping | Guide C pp. 4-5 to 4-9 and appendix B | Result mapping retains exact `qn`, `ql`, `nc`, separate `y3` and explicit `qd`. Known printable-ASCII invalid calls have an invalid state. Completion and transmitted instrument ID, serial and technologist are exposed. Short or ambiguous dates stay raw. Installed units, locales and time format still need capture evidence. |
| QC and identity | Guide C pp. 4-5, 4-8 to 4-9, B-7; Manual F ch. 10 | Result mapping splits reserved Cx labels into name and strip lot, classifies reserved QCx IDs and enforces the independent widths. Definitions can select `vidas.result_control_labels = "appendix_b"` if evidence confirms controls with other identifiers carry the exact Appendix B flags. CMVA doses stay separate and held because the uploads lack a unique component or run key. Other installed identifier layouts and the retransmission, rerun and correction rules still need captures. |
| Transport and syntax | Guide C pp. 2-6 and chapters 6–9 | BCI Link also describes ASTM-derived XML over FTP for the VIDAS PC. Its file exchange and XML message schema are outside this serial stack. |
| Replay | ADRs 0009, 0032, 0034 | The standard machine replay enumerates active definitions and supplies one site without machine choices or per-recording modes. It does not discover fixtures for a blocked model. |

## Evidence

Level: documented

Eight synthetic recordings in `recordings/` cover quantitative, qualitative,
invalid and bad-checksum uploads in both modes. See the
[Literal quantitative](recordings/literal-quantitative.rec.jsonl) and
[Alternate bad-checksum](recordings/alternate-bad-checksum.rec.jsonl) cases. Each file uses
invented IDs, assay codes and assay data. The deliberately damaged frames
expect the vendor's NAK and can be used to reproduce the current discrepancy.
These are manual-derived cases with no hardware or official simulator evidence.

## Open questions

- Read the installed VIDAS PC and BCI Link versions. Confirm the sample-ID
  ceiling, patient-ID limit and any identifier character restrictions.
- Confirm Literal or Alternate, active CR/LF switches, serial parameters
  including flow control, both connector genders and cable pinout. Check the
  configured codepage, case, date format, enabled fields and result-release mode.
- Capture a de-identified result of each type, a Cx and QCx control, an
  identical retransmission, a rerun and a correction. Check whether the
  captures supply stable identifiers or require operator reconciliation.
- If the lab uses work lists, confirm its patient namespace, enabled assay
  codes, dilution requirements and the operator's way to verify acceptance.

## Result identity evidence

BCI Link 161150-486 rev C, pp. 4-5 to 4-6 supports a candidate key of
ci sample ID, rt assay, td completion date and tt completion time. Confirm
timestamp precision and stability across resends, and confirm the sample ID
format for controls before enabling this recipe.

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
  chapter 10 including the communication traces on pp. 10-24–10-26, and
  p. 12-18.
- [Repository research](../../../docs/research/machines/vidas.md) and
  [VIDAS protocol notes](../../../docs/protocols/vidas/README.md).
