# analyte-app

**Kind: program.** Role folders group the work; the Windows shell has its own folder.

**What it does:** The Analyte desktop app on Windows (`analyte-app.exe`, called
"Analyte" everywhere a person sees it): the engine's admin pages in their own
window under the app's own title bar, and a tray icon with the engine's state
([ADR 0019](../../docs/adr/0019-desktop-app.md)).

**Where it sits:** Beside the engine (`analyte.exe`, the service). It opens the
engine's pages at `http://127.0.0.1:<admin port>/` and reads the engine's public
`GET /status`; it talks to nothing else.

**What it must never do:** Run the engine, read the admin-token file or hold
the admin key, change engine settings except through the engine's own pages,
show another site in its window, or say anything about patients or results.

**Main parts:**

- `engine`: where the engine is (`analyte-app.toml` beside the app, written by
  setup), its `/status` client, and asking Windows about the service and to
  start it (`sc.exe`, bounded).
- `watch`: the state the tray shows, when the engine counts as off, and which
  changes earn a notification (only worse ones).
- `navigation`: what the pages may show.
- `title_bar`: what the title bar shows (the engine's dot and word, Maximize
  or Restore) and how the window splits between the bar and the pages.
- `local`: the app's own pages: Engine off (`templates/engine-off.html`) and
  the title bar (`templates/title-bar.html`, `assets/title-bar.js`), their
  files compiled in from `crates/analyte/assets`, and the Start engine post.
- `shell` (Windows): Tauri, the tray, the window, start at sign-in, the log and
  the one task that watches the engine.

**The window and its two webviews:** the window has no Windows frame. On top
is the title bar, the app's own page in a webview labelled `title-bar`; below
it the engine's pages in a webview labelled `pages`. Only the bar has a
capability (`capabilities/title-bar.json`): drag, double-click to maximize,
minimize, maximize or restore, and close, from the app's own origin. The
pages have none. This uses Tauri's multi-webview API, which needs its
`unstable` feature, so Tauri is pinned exactly in the workspace; a test fails
when Tauri or the crates under it change version, until the bar has been
tried on the new ones and ADR 0019 names them. `build.rs` links the Windows
manifest Tauri needs into the tests as well as the app, so the test that
proves the permissions can start Tauri's mock runtime.

**Icons:** the window, taskbar and file use `crates/analyte/analyte.ico`.
The tray PNGs in `icons/` are drawn from the tray SVGs in `icons/svg/` by
`python -I scripts/make-app-icon.py`; 16 and 20 px come from the hand-tuned files.

**Platforms:** only the shell needs Tauri and WebView2, so it builds only on
Windows. Everything else builds and is tested on Linux too, where the program
says it runs on Windows only.

**Try it:** build with `cargo build -p analyte -p analyte-app`, put
`analyte-app.toml` with `admin_port = <port>` beside `target\debug\analyte-app.exe`,
run an engine on that port and start the app. `--tray` starts it in the tray
only. Its log is `%LOCALAPPDATA%\Analyte\analyte-app.log`.
