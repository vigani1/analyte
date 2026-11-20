# analyte-app

**Kind: program.** Role folders group the work; the Windows shell has its own folder.

**What it does:** The Analyte desktop app on Windows (`analyte-app.exe`, called
"Analyte" everywhere a person sees it): the engine's admin pages in their own
window, and a tray icon with the engine's state ([ADR 0019](../../docs/adr/0019-desktop-app.md)).

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
- `navigation`: what the window may show.
- `local`: the app's own Engine off screen (`templates/engine-off.html`), its
  files compiled in from `crates/analyte/assets`, and the Start engine post.
- `shell` (Windows): Tauri, the tray, the window, start at sign-in, the log and
  the one task that watches the engine.

**Icons:** the window, taskbar and file use `crates/analyte/assets/icons/analyte.ico`.
The tray PNGs in `icons/` are drawn from the tray SVGs by
`python -I scripts/make-app-icon.py`; 16 and 20 px come from the hand-tuned files.

**Platforms:** only the shell needs Tauri and WebView2, so it builds only on
Windows. Everything else builds and is tested on Linux too, where the program
says it runs on Windows only.

**Try it:** build with `cargo build -p analyte -p analyte-app`, put
`analyte-app.toml` with `admin_port = <port>` beside `target\debug\analyte-app.exe`,
run an engine on that port and start the app. `--tray` starts it in the tray
only. Its log is `%LOCALAPPDATA%\Analyte\analyte-app.log`.
