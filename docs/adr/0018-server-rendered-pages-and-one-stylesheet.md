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
- Pages moved one PR at a time, and the two looks never loaded on one page.
  **The last page PR (b6) is done:** sign-in and every refusal without a page
  of its own use a plain centred frame with no sidebar, so a page shown before
  sign-in names no machine, lab app or count. The old builder and its
  stylesheet are deleted, so the hold on release and installer builds is
  lifted.
- **Activity (PR c1) is done:** it is read from the tables the store already
  keeps, newest first; the store records when an item was parked from schema
  16 on, and an older item shows "Not recorded".
- **Live updates (PR c2) are done:** `GET /events` sends only which areas
  changed and the time, with a heartbeat; `/static/live.js` fetches the page
  again and swaps its marked regions. The stream checks the store's marks and
  the machines' link lights every 2 s, beats every 10 s, lives 5 minutes, and
  at most 4 are open (of 32 connection slots); shutdown ends them at once.
  Every page stays whole without the script. `futures-core` was added for the
  stream type axum takes.
- **Still to come:** PR d, the Engine off page and the Tauri desktop shell,
  which waits for the owner's approval before any work starts.

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
