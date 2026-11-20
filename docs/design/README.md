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

## Logo and icons

The mark is "Aperture" (HANDOFF.md, "Logo"). The icon files ship unchanged,
each as a large SVG and as hand-tuned `-16.svg` and `-20.svg` files: the app
icon in `crates/analyte/assets/icons/`, which pages serve, and the tray icons
in `crates/app/icons/svg/`, the sources of the desktop app's tray PNGs. Pages
draw the mark inline with the theme colours (the `brand_mark` macro) and link
the app icon SVGs as the browser-tab icon, the hand-tuned files at 16 and 20 px.

`crates/analyte/analyte.ico` is the Windows icon, used by the installer and the Apps list. It
holds PNG frames at 16 and 20 px drawn from the `-16`/`-20` files and at 24,
32, 48 and 256 px drawn from the large icon. After changing an icon, make it
again on Windows with Microsoft Edge installed, then commit the new file:

```
python -I scripts/make-app-icon.py
```

The script uses only the Python standard library; it has Edge draw each frame
and packs the PNGs into the `.ico`. The same SVGs give the same file.

What ships lives in `crates/analyte/assets/` (stylesheets, fonts with their
licences, icons) and `crates/analyte/templates/`; see
[ADR 0018](../adr/0018-server-rendered-pages-and-one-stylesheet.md).
