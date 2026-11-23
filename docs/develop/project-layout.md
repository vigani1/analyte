# Project layout

The whole repository on one page: where the code, the machine settings, the
docs and the installers live, and how they fit together. Details of each code
part are in [the crates](../../crates/README.md); the rules behind the layout
are in [ADR 0012](../adr/0012-uniform-layout-and-naming.md).

```text
analyte/
  crates/          the code (Rust)
    analyte/         the engine: runs on the lab PC and connects all parts
    app/             the Windows desktop app
    steps/           the layers every message passes through
      transport/       cable, network or folder
      framing/         message boundaries and checksums      (one file per language)
      dialog/          who talks when, acknowledgements, time limits (one per language)
      mapping/         turns messages into the common model   (one per language)
      sink/            delivers results to lab apps, polls their orders
    protocols/       the languages: reading and writing their messages
      astm/            standard
      hl7/             standard (IHE LAW lives inside it, as an HL7 profile)
      poct/            standard
      vidas/           vendor (bioMérieux)
      autolyser/       vendor (Dialab)
    core/            shared foundations: model, store, config, contract
    testing/         fake analyzers, the reference lab app, contract checker
  machines/        one folder per analyzer model: settings only, no code
    <vendor>/<model>/
      README.md        what is known, sources, open questions, evidence level
      machine.toml     the model's settings (when the model is ready)
      record.toml      enough to connect in Record only (first visit)
      recordings/      real or synthetic conversations, replayed as tests
  docs/            documentation (start at docs/README.md)
  packaging/       Windows installer and Linux packages
  fuzz/            random-input tests for every message reader
  scripts/         repository checks (layout, links, test names)
  .github/         CI, release workflow, issue forms
  references/      private, never committed: standards and vendor manuals
```

## The languages

| Language | Kind | Rules come from | Machines in the first lab |
| --- | --- | --- | --- |
| ASTM | Standard | CLSI LIS01-A2 and LIS02-A2 | cobas e 411, Biolyte 2000 |
| HL7 v2 | Standard | HL7 v2.3.1 and v2.5.1, MLLP | iFlash 1200, Swelab Lumi |
| IHE LAW | Standard (an HL7 profile) | IHE PaLM Technical Framework | none yet |
| POCT1-A | Standard | CLSI POCT01-A2 | none yet |
| VIDAS Literal | Vendor | bioMérieux BCI Link guide | VIDAS |
| Autolyser | Vendor | Dialab operator's manual | Autolyser |

Standard and vendor languages have the same layout and the same safety rules;
only the source of their rules differs.

## How a machine's settings meet the code

A `machine.toml` holds values only. Its `protocol` picks a language, and each
section becomes typed arguments for one layer of that language's code:

| Section | Layer that uses it |
| --- | --- |
| `[transport]` | `steps/transport` (link kind; the lab picks port and cable settings in Setup) |
| `[framing]` | `steps/framing/<language>` |
| `[dialog]` | `steps/dialog/<language>` |
| `[syntax]` | `protocols/<language>` |
| `[mappings]` | `steps/mapping/<language>` |
| `[[choices]]` | switches between behaviours the code already has (ADR 0032) |
| departures | named, evidenced exceptions to a standard (ADR 0015) |

Analyte checks every value when it loads and refuses anything unknown,
impossible or missing; a setting can only choose among behaviours the code
already has. The lab's own answers (port, cable settings, choices) are saved
in its site file on the lab PC, never here. How to write one:
[adding a machine](adding-a-machine.md).
