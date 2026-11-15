# Manual Gazelle LAW message validation

Gazelle's HL7v2 validator checks an individual message against a selected
IHE message profile: its structure, field usage, datatypes and constraints.
It does not prove durable storage, clinical correctness or a complete actor workflow.
These exports are original synthetic data based on IHE PaLM TF-2b Rev 11,
transactions 3.27-3.29 and Appendix C. No external validator is called by the example.
Evidence remains **documented** until a qualified run is recorded.

Generate from the repository root:

```sh
cargo run -p analyte-hl7 --example law_messages -- <dir>
```

Each file contains one unframed UTF-8 message with CR segment terminators and
no LF. Manager messages come directly from the public builders; analyzer messages
pass the public transaction parsers. The example fails on local validation errors.
The peer supports LAW_SPECIMEN; other enhanced options are disabled. The synthetic
work uses a serum specimen, one test. The parent/carrier variant uses SAC-4 plus SAC-10/11 with SAC-3 empty;
the identified-container variants omit carrier/tray positions (Appendix C.11). Responsibility
arguments model hypothetical successful storage; this example performs no host workflow.

Select the matching profile in Gazelle and submit each file manually:

| Actor | Message / files | Profile OID |
| --- | --- | --- |
| Analyzer Manager | RSP^K11: `rsp_k11_found.hl7`, `rsp_k11_empty.hl7` | 1.3.6.1.4.12559.11.1.1.177 |
| Analyzer Manager | OML^O33 NW/CA: `oml_o33_new.hl7`, `oml_o33_cancel.hl7`, `oml_o33_new_parent_carrier.hl7` | 1.3.6.1.4.12559.11.1.1.178 |
| Analyzer Manager | ACK^R22: `ack_r22_aa.hl7`, `ack_r22_ae.hl7` | 1.3.6.1.4.12559.11.1.1.179 |
| Analyzer Manager | OML^O33 DC: `oml_o33_negative.hl7` | 1.3.6.1.4.12559.11.1.1.215 |
| Analyzer | QBP^Q11: `qbp_q11_specimen.hl7`, `qbp_q11_all_pending.hl7` | 1.3.6.1.4.12559.11.1.1.180 |
| Analyzer | ORL^O34 (ORL_O42 structure): `orl_o34_accept.hl7` | 1.3.6.1.4.12559.11.1.1.181 |
| Analyzer | OUL^R22: `oul_r22.hl7` | 1.3.6.1.4.12559.11.1.1.182 |

The found and empty RSP both acknowledge query processing with QAK OK; availability
is expressed by the subsequent new or restricted negative LAB-28, respectively.
The AA and AE ACK files are alternative outcomes for the same synthetic result.

Save reports in `docs/conformance/gazelle/results/`. Record the tool build, profile
revision/OID, actor/options, date, source commit and filename with each report.
Retain failures and their diagnostics as well as successes. Message-level validation is recorded below. A simulator conversation and
full LAW actor qualification remain outstanding.

## Recorded Gazelle run (2026-11-15)

Service: IHE Catalyst EVS 10.0.2, "HL7v2 Validation service" 4.0.3, submitted
by `check.py` from the exports of commit 24350ca. Reports and `summary.json`
are in `results/`.

| File | Sent by | Profile | Result |
| --- | --- | --- | --- |
| rsp_k11_found, rsp_k11_empty | Analyzer Manager | ...177 | PASSED, 0 errors |
| oml_o33_new, oml_o33_cancel, oml_o33_new_parent_carrier | Analyzer Manager | ...178 | PASSED, 0 errors |
| oml_o33_negative | Analyzer Manager | ...215 | PASSED, 0 errors |
| ack_r22_aa, ack_r22_ae | Analyzer Manager | ...179 | PASSED, 0 errors |
| orl_o34_accept | Analyzer (synthetic) | ...181 | PASSED, 0 errors |
| qbp_q11_specimen, qbp_q11_all_pending | Analyzer (synthetic) | ...180 | FAILED, Gazelle profile conflict (below) |
| oul_r22 | Analyzer (synthetic) | ...182 | UNDEFINED, validator exception (below) |

Every message Analyte sends as Analyzer Manager passes with no errors. The
first run found one real defect: OML NW/CA carried SAC-3 together with
SAC-10/11, which Appendix C.11 forbids for the manager (carrier/tray only when
SAC-3 is absent and SAC-4 is present). It was fixed in this change and the
parent/carrier export exercises the permitted form.

Remaining warnings are advisory: conditional elements for which the profile
defines no rule, and coding-system names such as `IHELAW` that the LAW profile
itself requires.

Two service-side issues; the exports keep their values:

- QBP profile 1.3.6.1.4.12559.11.1.1.180 limits MSH-21 EI-1 to five characters.
  Rev 11 section 2.2.2 and Table C.4-14 allow length 50 and require `LAB-27`,
  which cannot fit that profile limit. This is a Gazelle profile conflict,
  not an Analyte defect.
- OUL profile 1.3.6.1.4.12559.11.1.1.182 raised an internal `ProfileException`
  at OBX-4 and returned UNDEFINED. This is a validator failure, not an
  Analyte validation result or defect.

This is message-level validation only. It does not raise the evidence level of
the LAW implementation; that needs a recorded simulator conversation.

To repeat: generate the exports, then `python -I docs/conformance/gazelle/check.py
<exports dir> docs/conformance/gazelle/results`. Submit synthetic messages only.
