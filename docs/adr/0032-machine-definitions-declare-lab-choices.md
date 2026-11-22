# 0032. Machine definitions declare the choices the lab makes

Status: accepted, 2026-11-22 (the owner asked that a machine definition can
declare its own choices and that Setup asks exactly those, and approved the
recommended route without waiting). Refines
[ADR 0002](0002-config-first.md) (which layer owns what) and extends
[ADR 0031](0031-lab-chooses-serial-cable-settings.md) (the lab chooses,
the definition suggests) beyond the serial cable. Keeps
[ADR 0015](0015-named-machine-departures.md) (departures) unchanged.

## Context

Several analyzers offer modes on their own screen that change how they talk:
a host dialect (which ASTM field positions they use), "send results only",
VIDAS Literal or Alternate acknowledgment, serial or network link. Some facts
depend on the installed software version: the VIDAS sample ID limit is 12 or
17 characters by the VIDAS PC version. Until now a definition could hold only
one value for each, so a lab whose analyzer was set differently needed a
second definition or a hand-written overrides file. Other facts (where the
sample ID sits, QC rules, correction codes, deadlines, size limits) belong to
the model and must not be the lab's to change.

## Decision

1. **The definition declares each choice; the protocol code owns the
   behaviour.** A definition lists `[[choices]]`: a name, a kind (`mode` or
   `software_version`, at most one), a plain-language question, an optional
   suggested value from the manual, and 2 to 16 values. Each value has its own
   words and selects existing building blocks from a closed list: the whole
   `[mappings]` table (a host dialect), `orders = "on" | "off"` (keep or turn
   off the definition's `[orders]`), and `vidas.alternate` and
   `vidas.sample_limit`. No field path, pattern or expression can be written.
   Every value of a choice selects the same blocks, a block belongs to one
   choice, and a setting a choice selects is written only in the choice (the
   definition's own `[mappings]` or `[vidas]` key beside it is refused). At
   most 16 choices and 64 combinations; **every combination is validated as a
   whole stack when the definition loads**, so Setup never offers one the
   protocol code refuses. A new building block is a code change in the config
   crate with its own tests, never a config feature.
2. **A software version selects facts, not free values.** The lab picks its
   version from the declared bands (read on the About screen); the band
   decides the facts, such as the sample ID limit. A version band usually has
   no suggestion, because only the lab can know it.
3. **Nothing is assumed.** The lab's value for each choice lives in the site
   file's `[choices]` (`name = "value"`), written only through Setup's one
   validated write path, as in ADR 0031. A choice without a selection takes
   the suggestion; without either it is refused by name
   (`choices.software_version: no value`). An undeclared choice, a value not
   offered, or a name in another spelling is refused. Save writes every
   choice's value, so a later definition update never silently changes a
   working setup.
4. **The snapshot holds what runs.** The effective snapshot holds the
   definition with the chosen values applied, plus a `[choices]` table of the
   values in use, and not the other alternatives. The digest changes when a
   choice changes; the same values give the same bytes whether chosen or
   suggested; a definition without choices keeps its earlier bytes.
5. **Evidence and departures stay out.** A choice cannot write `evidence`,
   `deviations` or `departure_scope` (refused by name). Departures remain the
   closed, evidenced list of ADR 0015; a mode that needs a departure (for
   example packed ASTM records) still needs its own declaration with scope and
   evidence.
6. **One place, the same everywhere.** An overrides file that replaces a block
   a choice owns is refused ("keep it in one place"). Setup step 2 builds one
   list per declared choice from the selected model's definition, with no
   per-machine page code, and marks values that differ from the suggestion.
   `analyte machines set` and `test` take a repeatable `--choice NAME=VALUE`
   through the same draft and checks (asking at a terminal for a choice
   without a value); `machines models` names each model's choices;
   `machines list` (and `--json`), `doctor` and the machine's page show the
   values in use and which differ. `machines add` takes none: the model is
   chosen later.
7. **The serial cable stays its own table.** ADR 0031's `[serial]` and its
   flags are unchanged: the cable's five settings come from standard lists for
   every serial machine, not from a list each definition declares, so making
   them a declared choice would add a list per definition for no gain. Both
   show the same way (the suggestion, the choice, what differs) and are saved
   through the same write.

## Consequences

- One definition can serve an analyzer in each of its documented modes and
  software versions; the lab chooses on the Setup page or with one command.
- A model with a version band cannot run until the lab picks its version.
- A link-kind choice (serial or network, as the Autolyser may offer) is not a
  building block yet: Setup asks for a different connection per link kind,
  and the Autolyser network option is undocumented. It can be added as a
  block when a documented machine needs it.
- The six first-lab machines are still blocked definitions; their choices are
  written when their definitions are (`docs/develop/adding-a-machine.md`).
