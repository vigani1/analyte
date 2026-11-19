# 0018. Server-rendered admin pages with one token stylesheet

Status: accepted, 2026-11-19 (the owner approved the restyle and Askama; the
desktop shell waits for its own approval)

## Context

The admin pages were built as strings in Rust with one small stylesheet. The
owner approved a full design (`docs/design/`): one design-system stylesheet of
tokens and components, two bundled fonts and a later desktop window with a tray
icon. The pages must stay simple to audit, work offline on a lab PC and keep
the strict content security policy.

## Decision

- Pages stay **server-rendered by axum**. Each restyled page is an Askama
  template with one Rust view struct of display-ready values; formatting rules
  live in one Rust module. No React, Node, CSS framework or build step.
- **One global token stylesheet:** `analyte.css` from the design, copied
  unchanged, loaded after `fonts.css` and before `app.css`, which holds only
  Analyte's additions and uses semantic tokens only.
- Every asset is built into the program and served under `/static/` from one
  table; nothing loads from the internet.
- The **CSP stays strict** (`default-src 'self'`): no inline style, no inline
  script, no `|safe`; escaping stays on everywhere.
- **Live updates**, when added, use axum server-sent events plus one script
  served as a file.
- A **desktop shell** (Tauri: window and tray icon around the same pages) is
  planned and needs the owner's approval before any work starts.
- Pages move one PR at a time; until a page is restyled it keeps the old
  builder and `admin.css`, and the two stylesheets never load on one page.
  **No release or installer is built until the last page PR (b6) is merged**,
  because the look is mixed until then.

## Why

Server-rendered templates keep every value escaped and checked at compile time
without a JavaScript toolchain, and one token sheet gives every page the same
look, both themes and reduced motion from one file.

## Consequences

- This is hard to reverse: templates, assets and the view structs become the
  way every page is written.
- Askama is a new dependency (MIT or Apache-2.0); the fonts are SIL OFL 1.1 and
  ship with their licences (`docs/design/fonts.md`).
- A token missing from the design is added to `app.css` for both themes with a
  comment, never as a raw value in a page.
