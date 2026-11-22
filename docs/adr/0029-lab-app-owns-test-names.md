# 0029. The lab app owns test names

Status: accepted, 2026-11-22 (owner decision: "Analyte provides only the
connection and the translation of machine things. Test names and test lists
belong to the lab app."). Changes [ADR 0002](0002-config-first.md) (code
mappings), [ADR 0003](0003-clinical-safety-rules.md) rule 2 (unknown tests),
[ADR 0008](0008-app-contract.md) (test fields) and the example cause in
[ADR 0024](0024-one-review-item-per-cause.md). Changed by
[ADR 0030](0030-lab-app-owns-specimen-codes.md): specimen types follow the
same rule.

## Context

Each machine definition carried a test table: `[mappings] tests` (or
`[codes] tests` for vendor formats) turned each machine code into the lab
app's code and code system before delivery, and `[orders] tests` turned the
lab app's codes back into machine codes, with the test's display text for HL7
and LAW. A code missing from the table parked the result for review.

So a lab kept the same list twice: in its lab app, where tests, names and
panels are defined, and again in Analyte. A test newly enabled on a machine
was held back until someone edited Analyte's file. The table held the lab's
knowledge (its codes and names), not the machine's. Lab information systems
already keep a test mapping per instrument model; that is where it belongs.

## Decision

1. **Results carry the machine's own code.** A result's `test` is the code
   exactly as the machine sent it (case, leading zeros and padding kept), in
   the code system the message names for it (HL7 and IHE LAW OBX-3.3, the
   POCT observation's code system). When the message names none (ASTM, VIDAS,
   Autolyser, an empty OBX-3.3), the namespace is the machine definition's
   declared `test_system`. The same namespaced code identifies the
   observation, and the delivery's `source` says which machine sent it.
2. **The definition keeps only machine facts.** `[mappings]` has a required
   `test_system`, for ASTM a `test_component` (`universal` or `manufacturer`,
   the R.3 component that holds the code, LIS02 §5.6.1) and optional
   `value_kinds` (the kind of value a machine code carries). `[codes]` has
   `test_system` and the unit table. Units, component parts, QC rules,
   identity selectors and dilution evidence stay as they were.
3. **No table, so no unknown code.** A result is no longer parked because its
   code is not listed. Only an observation that sends no code at all parks
   (`MissingTest`): there is nothing to name, and nothing is invented. All
   other safety rules stand: values as written, unreadable messages parked, a
   lab app's refusal (400, 409, 422) parked as before. A code the lab app does
   not know is now refused by the lab app, and that refusal parks the delivery.
4. **Orders arrive in the machine's code system.** `[orders] tests` is
   removed. An order test is sent only when its namespace is exactly the
   machine's `test_system`; its value goes out as written, within the
   protocol's own limits (VIDAS 1 to 4 printable bytes without a pipe,
   Autolyser exactly 4 bytes, the HL7 and LAW OBR-4 component rules). A test
   in another code system, or one the protocol cannot carry, is refused
   through the existing order-rejection path before any reservation, so
   nothing is sent and nothing is resent. `[orders.astm] test` names the
   component the code goes in (with optional `test_components` for empty
   components after it), and `[orders.hl7_send] test_version` gives the
   version CWE editions write in OBR-4.7.
5. **Names come from the lab app.** An order test may carry an optional
   `name`, the lab app's own name for it. IHE LAW requires OBR-4.2 (PaLM
   TF-2b), so a LAW order test without a name is refused; HL7 orders write it
   in OBR-4.2 when given and leave it empty otherwise; other protocols have
   no place for it.
6. **Contract v1 changes in place.** Version 1 is unpublished (0.1.0 is a
   draft), so this is not a version bump: `test` keeps its shape and gains its
   meaning, and an order test gains the optional `name`.

## Why

- Analyte is a bridge: it moves and reads machine data. Test lists, names
  and panels are the lab's decisions and live in the lab app.
- One way per thing: one test list, kept by the lab app, instead of two that
  can drift apart.
- It is how labs already work: a lab system maps each instrument model's
  codes to its own tests.
- Nothing is lost or changed: the code arrives exactly, namespaced, and the
  lab app decides. A new machine test no longer waits for an Analyte edit.

## Consequences

- Lab apps keep one table per machine model: code system and code to their
  test. The lab-app guide and contract say so.
- Machine definitions get shorter, and the lab no longer gives Analyte its
  code for each test.
- Stored reasons written before this change (`UnknownTest`, "unknown test
  mapping") show as stored text; no released store holds them.
- Specimen-type translations: changed by
  [ADR 0030](0030-lab-app-owns-specimen-codes.md); orders now carry the
  machine's own specimen code too. Priority translations stay: the contract's
  three priorities are its own meanings, not the lab's codes, and the wire
  codes are protocol meanings Analyte must read (ADR 0030 lists every
  remaining table and why it stays).
- The 2026-11-15 Gazelle recording stays as captured; its rerun inputs use
  the new form.
