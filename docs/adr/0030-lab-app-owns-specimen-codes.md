# 0030. The lab app owns specimen codes

Status: accepted, 2026-11-22 (applies the owner decision behind
[ADR 0029](0029-lab-app-owns-test-names.md): "Analyte provides only the
connection and the translation of machine things; the lab's own codes belong
to the lab app"). Changes the specimen-type consequence of ADR 0029,
[ADR 0002](0002-config-first.md) (code mappings) and the order
`specimen_type` field of [ADR 0008](0008-app-contract.md).

## Context

ADR 0029 removed the test tables and left the other order translations in
place. One of them, `[orders] specimen_types`, still turned the lab app's own
specimen codes (its code system and code, for example `lab` `SERUM`) into the
machine's code (`SER`, or `S` on the Autolyser). That table is the lab's
specimen list, kept a second time in Analyte, exactly like the test table was.

Every other remaining table was checked against the same rule: does it hold
the lab's own codes, or a machine or protocol fact Analyte must understand to
keep a safety rule?

| Table | Translates | Verdict and reason |
| --- | --- | --- |
| `[orders] specimen_types` | lab-app specimen code to machine code | **Moves to the lab app.** It is the lab's specimen list. |
| `[orders] priorities` | contract `routine`/`urgent`/`stat` to the machine's wire code | **Stays.** The three values are the contract's own meanings, not a lab code system, and the wire codes are protocol meanings Analyte must read: the Autolyser routine or STAT list, the HL7 table 0027 priority typed into ORM/OML, the ASTM O-6 code. Moving it would push protocol knowledge into every lab app. |
| `units` (`[mappings]`, `[codes]`) | machine unit spelling to UCUM | **Stays.** A machine spelling to a standard (rule 6); the original unit is always kept and an unlisted unit is delivered as written. |
| `qc` rules | machine field value to patient, control or calibration | **Stays.** A machine fact; it keeps control results from being delivered as patient results. |
| `identity` (selectors, `correction_status_codes`, `order_namespace`) | where the machine puts identity, and its correction code | **Stays.** Machine facts that keep the three identities and revisions (rule 4). |
| `component_parts`, `applied` yes/no, `value_kinds` | machine tokens and value kinds | **Stays.** Machine facts for dilution (rule 6) and values as written (rule 5). |
| `patient_namespace`, `placer_namespace`, LAW `specimen_namespace`, `container_namespace`, `specimen_role` | nothing; namespace checks and protocol identity fields | **Stays.** Not translations. |
| Result flags and result status | nothing | No table: flags pass through as sent; vendor status meanings stay an open gap (context.md). |

## Decision

1. **Orders carry the machine's own specimen code.** `[orders] specimen_types`
   is removed. The machine definition declares one `[orders] specimen_system`,
   the machine's own specimen-type code system. An order's `specimen_type` is
   sent only when its namespace is exactly that system; its value goes out as
   written, within the protocol's own limits: the ASTM text repertoire for
   O-16.1 (no delimiters, LIS02), `S` or `U` for the Autolyser work list, the
   HL7 SPM-4 / OBR-15 and LAW SPM-4 component rules. HL7 and LAW write the
   system as the coding system (SPM-4.3).
2. **Refused before reservation.** A specimen type in another code system
   (`foreign-specimen-type`), a code the protocol cannot carry
   (`unrepresentable-specimen-type`, or the ASTM text-repertoire refusal) and
   any specimen type sent to VIDAS, which has no place for one
   (`unsupported-specimen-type`), reject the order through the existing
   order-rejection path before any reservation: nothing is sent, nothing is
   resent. A required but missing specimen type stays `missing-specimen-type`;
   nothing is defaulted.
3. **One place.** `[orders.hl7_send] specimen_system` and `[law.work]
   specimen_system` move into `[orders] specimen_system`; the HL7 CWE
   `specimen_version` stays in `[orders.hl7_send]`.
4. **Declared exactly where written.** `specimen_system` is required for LAW,
   the Autolyser, an ASTM `specimen_type` destination, an HL7 `specimen_type`
   display line and HL7 CWE send editions, and refused where nothing writes it
   (VIDAS, ASTM without that destination, HL7 with neither send-ahead nor that
   line).
5. **Contract v1 changes in place.** Version 1 is unpublished (0.1.0 is a
   draft): `specimen_type` keeps its shape and gains its meaning, the
   machine's own specimen code in the machine's specimen code system.
6. **Priorities stay**, for the reason in the table.

## Why

- The same rule as ADR 0029: one specimen list, kept by the lab app, instead
  of two that can drift apart.
- Nothing is guessed: the code arrives as the lab app wrote it, namespaced,
  and a mismatch is refused visibly before anything reaches the machine.

## Consequences

- Lab apps keep, per machine model, the machine's specimen codes beside its
  test codes. The contract and the lab-app guide say so.
- Machine definitions name `specimen_system` instead of a specimen table.
- Stored reasons written before this change (`unmapped-specimen-type`) show
  as stored text; no released store holds them.
- The 2026-11-15 Gazelle recording stays as captured: the rerun inputs send
  `HL70487` `SER`, which writes the same `SER^^HL70487` SPM-4.
