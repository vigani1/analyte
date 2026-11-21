# analyte-app

**Kind: program.** Role folders group the work; the Windows shell has its own folder.

**What it does:** The Analyte desktop app on Windows (`analyte-app.exe`, called
"Analyte" everywhere a person sees it): the engine's admin pages in their own
window under the app's own title bar, and a tray icon with the engine's state
([ADR 0019](../../docs/adr/0019-desktop-app.md)).

**Where it sits:** Beside the engine (`analyte.exe`, the service). It opens the
engine's pages at `http://127.0.0.1:<admin port>/`, reads the engine's public
`GET /status` and asks the engine's Windows sign-in pipe
`\\.\pipe\analyte-sign-in-<admin port>`; it talks to nothing else.

**What it must never do:** Run the engine, read the admin-token file or hold
the admin key, change engine settings except through the engine's own pages,
show another site in its window, or say anything about patients or results.

**Main parts:**

- `engine`: where the engine is (`analyte-app.toml` beside the app, written by
  setup), its `/status` client, and asking Windows about the service and to
  start it (`sc.exe`, bounded).
- `watch`: the state the tray shows, when the engine counts as off, and which
  changes earn a notification (only worse ones).
- `navigation`: what the pages may show, and which addresses are Windows
  sign-in for the app to run.
- `sign_in`: Windows sign-in ([ADR 0021](../../docs/adr/0021-windows-sign-in-pipe.md)):
  the pipe's two lines (`line`), the pipe client (`pipe`, Windows), the
  once-per-30-s guard and where each answer leads.
- `title_bar`: what the title bar shows (the engine's dot and word, Maximize
  or Restore and the button's look), how the window splits between the bar
  and the pages, where the maximize button sits, and which window-menu items
  each sizing offers.
- `local`: the app's own pages: Engine off (`templates/engine-off.html`) and
  the title bar (`templates/title-bar.html`, `assets/title-bar.js`), their
  files compiled in from `crates/analyte/assets`, and the Start engine post.
- `shell` (Windows): Tauri, the tray, the window and its frame features
  (`frame`), start at sign-in, the log and the one task that watches the
  engine.

**The window and its two webviews:** the window has no Windows frame. On top
is the title bar, the app's own page in a webview labelled `title-bar`; below
it the engine's pages in a webview labelled `pages`. Only the bar has a
capability (`capabilities/title-bar.json`): minimize, maximize or restore,
and close, from the app's own origin. The pages have none. The bar is a
Windows caption through WebView2's `app-region`, so Windows drags the window,
maximizes it on a double-click and shows the window menu on a right-click.
`shell/frame.rs`, the app's only `unsafe` code (ADR 0022), adds what a page
cannot: a native window over the maximize button so Windows shows snap
layouts on hover (the app gives the page's button its hover and pressed
looks), Alt+Space for the window menu, and the menu's items matching the
window. This uses Tauri's multi-webview API, which needs its
`unstable` feature, so Tauri is pinned exactly in the workspace; a test fails
when Tauri or the crates under it change version, until the bar has been
tried on the new ones and ADR 0019 names them. `build.rs` links the Windows
manifest Tauri needs into the tests as well as the app, so the test that
proves the permissions can start Tauri's mock runtime.

**Windows sign-in:** the pages' webview has the user agent
`Analyte/<version> AnalyteApp`; the title bar keeps WebView2's own. The window
opens at `/login/windows?next=/`. Whenever the pages are sent to
`/login/windows` without a code (the window opening, the engine's refresh on
an unsigned page, or "Sign in with Windows"), the app cancels that load, asks
the pipe at identification level (at most 3 s while it is busy, then 2 s for
the answer, each line at most 1 KiB) and opens `/login/windows?code=…`, the
refusal's `/login?windows=<word>`, or the plain key form `/login` when there
is no pipe or no valid answer. The engine's refresh may trigger this at most
once per 30 s; a click on "Sign in with Windows" always does and starts the
30 s again, and opening the window clears them. The pipe's line format has one
source, `crates/testing/fixtures/windows-sign-in.txt`, which the engine's and
the app's tests both read.

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
