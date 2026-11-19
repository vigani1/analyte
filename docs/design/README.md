# Admin page design (reference only)

The approved design for the admin pages, kept so later page PRs restyle from
the same source. Nothing here is served or built.

- [HANDOFF.md](HANDOFF.md): the designer's notes (build rules, page skeleton,
  live-update behaviour).
- [mockups/](mockups/): one design-tool file per screen or state. These are
  **not production HTML**: the real structure is the markup between `<x-dc>`
  and `</x-dc>`, the sample data in each file's `renderVals()` is invented, and
  `<helmet>`, Google Fonts links, `support.js`, `/_blob/` links, `dc-import`,
  `sc-for` and `sc-if` are design-tool parts. Pages turn them into Askama
  layouts, includes, loops and conditions and show only data Analyte has.
- [fonts.md](fonts.md): where each bundled font file came from, with hashes.

What ships lives in `crates/analyte/assets/` (stylesheets, fonts with their
licences, icons) and `crates/analyte/templates/`; see
[ADR 0018](../adr/0018-server-rendered-pages-and-one-stylesheet.md).
