# 0036. Warn when a machine reports unexpected software

Status: accepted, 2026-11-23. Extends ADRs 0014, 0023 and 0032.

A software upgrade may change the analyzer's interface. A definition can opt
into a check with a `[software_version]` table. It names a supported `selector`
and a `source` describing the vendor evidence for that position. ASTM
`astm_sender_third_component` reads H.5 component three (BioCare Biolyte Service
Manual 4.2A, Appendix VI). HL7 and LAW support `hl7_sending_application` and
`hl7_sending_facility`, which read the first component of MSH-3 or MSH-4 where
a vendor documents software there. The HL7 protocol version in MSH-12 is
independent of analyzer software.

The check compares exact wire tokens to known `evidence.capabilities.firmware`
values. If the definition offers a software version choice, its
`[software_version.bands]` table lists the checked tokens for every choice value.
The active band alone supplies the expected set. Each band token also appears
in the evidence. No ordering, wildcard, or prefix matching is inferred.
Blank, malformed, ambiguous or oversized metadata is reported as unknown.
Tokens are at most 64 ASCII bytes and use letters, digits, dots, hyphens,
underscores or plus signs, including at least one digit. A definition without
a selector and a record-only connection perform no comparison.

The warning is operational. It neither changes interpretation nor blocks
results. The store keeps the first receipt time for the currently unexpected
version. Repeated reports preserve that time. Confirmation names the exact
known version and is recorded atomically with the operator audit. A changed
definition or choice invalidates its earlier warning and confirmation. An
unknown report remains visible until a different unexpected report or an
updated definition. A matching receipt alone does not certify the settings.

The public status endpoint carries only the closed reason word
`machine_software_version_changed`. Authenticated views show the bounded
software token, or unknown. The opt-in webhook condition names the machine.
Messages and clinical values stay out of operational logs and events.

## Amendment (2026-11-26)

When the software version choice offers `checked = false` under ADR 0032,
`[software_version.bands]` lists expected tokens for every checked value and
omits the unchecked value. The loader refuses tokens for the unchecked
value. When that value is active, the software version is unchecked. The
engine makes no wire comparison, including when an incoming token appears in a
checked band's evidence.

Overview shows "Software version not checked against a manual" for each
affected machine, even before it sends any data or when it has no wire
selector. Public `GET /status` reports the reason
`machine_software_version_unchecked` without identifying the machine. The
warning remains while the unchecked value is active, and normal result
processing continues.
