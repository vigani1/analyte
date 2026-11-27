# 0037. De-identified recording copies made on the lab PC

Status: accepted, 2026-11-26. Extends ADRs 0009 and 0034.

## Context

The ADR 0034 amendment of 2026-11-26 lets any lab share a recording, and a
shared recording can raise a machine model to proven (ADR 0005). Until now a
lab had to remove patient details by hand on its own PC, or send the original
away to have it done. Either way patient data could leave the lab, or a
hand-made copy could miss a field or break a checksum.

## Decision

- `analyte captures de-identify <machine> <file>.rec.jsonl --to <folder>
  --reason "..."` writes a new file, `<name>.deidentified.rec.jsonl`, into an
  existing folder. It never changes or replaces the original (safety rule 3)
  and never overwrites a file. It is audited first, with the actor and the
  reason, like `captures export`.
- The original is read through the bounded ADR 0009 reader. The whole copy is
  made in memory before anything is written, so a refusal writes nothing.
  The work stops after 120 seconds.
- The protocol comes from the recording's own stack. Patient fields are found
  from the standard's positions only:
  - ASTM (LIS01-A2 frames, LIS02-A2 records): every P field after the
    sequence; O specimen ID, the ID part of the instrument specimen ID,
    collector, clinical information and physician; every component of the Q
    range IDs, because the manufacturer defines the later components; comment
    text not attached to a result. An M record or an unknown record type
    refuses the file.
  - HL7 v2 over MLLP, IHE LAW included: PID, PD1, PV1 (after the class),
    PV2, NK1 and insurance and guarantor segments whole; ORC and OBR order
    numbers, people and clinical information; SPM and SAC identifiers; NTE
    notes, PRT persons, QRD and QRF filters, QPD parameters and DSP display
    lines. OBX, acknowledgment, timing and equipment segments are kept. An
    unknown segment, Z segments included, refuses the file.
  - VIDAS Literal: `pi`, `pn`, `pb`, `ps`, and `ci` unless it names a kit or
    quality control; a tag outside the documented set is free text.
  - Autolyser: the fifteen-byte patient code of reports and work lists.
  - POCT1-A and watched-folder recordings are refused by name: POCT carries
    patient data in many optional elements and vendor extensions, and a
    partial clean is worse than none.
- A synthetic value keeps the byte length and the class of every byte: digits,
  upper and lower case letters and non-ASCII letters of the same encoded width
  are replaced at random from the system random source; every other byte stays.
  Leading zeros stay zeros. Birth dates become valid dates of the same layout;
  a one-letter sex code becomes `U`. Within one file the same real value always
  gets the same stand-in, in both directions, and no stand-in spells another
  real value of the file.
- Because every length is kept, frame numbers, MLLP framing, event boundaries,
  split and merged reads and times stay exactly as recorded. Each checksum
  that covers a replaced byte gets the change added: ASTM and VIDAS hex, the
  Autolyser three-digit sum. A checksum that was wrong stays wrong by the same
  amount, so a recorded fault still replays as one. ASTM hex is upper case
  (LIS01-A2). VIDAS case is a site setting: it is read from the recording, and
  when the recording cannot show it the command refuses until the operator
  states it with `--checksum-case`.
- Anything that cannot be read well enough to place every patient field
  refuses the whole file with the event index: bytes outside frames, a record
  before its header, a repeated frame that differs from its first send, an
  escape sequence that could spell text (hex, character set or local), or
  machine bytes nobody asked for. Messages name an index and a reason, never
  a value.
- The copy's header stays format version 1 with the same evidence. Its
  de-identification statement starts with the fixed words "De-identified by
  Analyte captures de-identify", and its source ends with "de-identified copy".
  The existing field carries the mark, so no format change is needed.
- The command prints the counts and "Review the copy before sharing; free-text
  fields were replaced."

## Why

Making the copy on the lab PC keeps the original there. Same-length
replacement is what lets the copy replay through Analyte's own tests without
rebuilding any frame. Refusing a file is safer than a copy that looks clean
and is not.

## Consequences

- Results, units, flags, times and every structural byte stay as sent. Free
  text inside a result (an OBX text value, an ASTM result comment from the
  instrument) is kept, which is why the copy still needs a human review.
- Host replies built from lab app orders contain stand-ins, so a query
  conversation replays only against orders seeded with the stand-in values.
- A machine that puts patient data outside the standard positions, or sends M
  records, Z segments or vendor tags, needs those positions added here before
  its recordings can be cleaned.
