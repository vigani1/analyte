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
work uses a serum specimen, one test, and an explicit rack position. Responsibility
arguments model hypothetical successful storage; this example performs no host workflow.

Select the matching profile in Gazelle and submit each file manually:

| Actor | Message / files | Profile OID |
| --- | --- | --- |
| Analyzer Manager | RSP^K11: `rsp_k11_found.hl7`, `rsp_k11_empty.hl7` | 1.3.6.1.4.12559.11.1.1.177 |
| Analyzer Manager | OML^O33 NW/CA: `oml_o33_new.hl7`, `oml_o33_cancel.hl7` | 1.3.6.1.4.12559.11.1.1.178 |
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
Retain failures and their diagnostics as well as successes. No Gazelle run or
conformance claim is included in this change; passing local parsers is not full
LAW conformance or simulator evidence.
