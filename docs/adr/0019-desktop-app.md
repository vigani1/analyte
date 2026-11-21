# 0019. The Analyte desktop app: a window and a tray beside the engine

Status: accepted, 2026-11-20 (the owner approved Tauri 2 and its official
plugins on 2026-11-20; the app's own title bar, decision 11, on 2026-11-20).
Completes PR d of
[ADR 0018](0018-server-rendered-pages-and-one-stylesheet.md). Refined by
[ADR 0021](0021-windows-sign-in-pipe.md): the app signs people in with their
Windows account; the key form stays the way back in. Refined by
[ADR 0022](0022-title-bar-snap-layouts-and-window-menu.md): the title bar
regains snap layouts and the window menu. Refined by
[ADR 0023](0023-status-reasons-and-notifications.md): `/status` names the
kinds of thing that need someone, notifications say what happened, a click
opens the Overview and the title bar keeps the last notification.

## Context

The lab manager opened `http://127.0.0.1:<admin port>/` in a browser to reach
the admin pages. The owner wants a desktop program instead: the same pages in
its own window and a tray icon with the engine's state. The engine itself
stays as it is: the `analyte.exe run` Windows service (or Linux unit) serving
the pages on 127.0.0.1. Switching analyzers over is not part of this.

## Decision

1. **Two programs, two names.** The engine stays `analyte.exe`, now described
   as "Analyte engine" in its version details. The app is `analyte-app.exe`
   (crate `crates/app`, a program kind), and everything a person sees calls it
   **Analyte**: the Start menu entry, window title, taskbar, tray tooltip,
   Apps list and notifications. Its identifier, `io.github.vigani1.analyte.app`,
   is also the notification identity the Start menu entry carries.
2. **Tauri 2 on Windows only.** Tauri 2.12 with WebView2 and the official
   single-instance, notification and opener plugins. Tauri is pinned exactly
   and built with its `unstable` feature for the title bar (decision 11).
   The app's code that decides what to show (state mapping, notifications,
   navigation, the Engine off screen, the title bar's words and layout)
   builds and is tested on every platform; only the shell needs Tauri. Linux keeps the browser: its packages and CI are unchanged.
3. **The window shows the engine's origin and nothing else.** It opens the
   engine's pages; a link to another site opens in the default browser and
   anything else is refused. People sign in with the normal form: the app
   never reads the admin-token file and never holds the key. The app grants
   the pages no Tauri commands. Closing the window destroys its webviews; the
   app stays in the tray and "Open Analyte" makes a new window. One instance
   runs at a time.
4. **Where the engine is.** The program file is readable only by
   administrators and the service, so setup writes `analyte-app.toml` with the
   admin port beside the app, in a folder only administrators may change.
   Without the file the app uses the starter's port 8080; a file that exists
   but is wrong is shown as such, never replaced by a guess.
5. **`GET /status` on the engine.** A public route, like sign-in, with the same
   Host and Origin checks and `no-store`. It answers
   `{"state": "...", "version": "..."}` and nothing else: no names, counts,
   identifiers or values. The state is the Overview verdict's own word, made
   in one place (`overview::Overall`): `good`, `attention`, `problem`, or
   `unknown` with status 503 when the engine could not read its state in time.
   It reads what the verdict reads, except receipts, which only draw machine
   nodes.
6. **Tray: three states plus off.** All good (green tick), needs attention
   (amber triangle), problem (red octagon, also for `unknown`) and engine off
   (faded mark, grey pause). Each is the Aperture mark with a badge, 16 and
   20 px drawn by hand. The app asks `/status` every 5 s, each question within
   3 s; the engine counts as off after two unanswered questions in a row, or
   one before the first answer. Something else answering on the port is not
   the engine.
7. **Notifications only when it gets worse:** good, attention, problem, off;
   one per change, never for the first state seen, never when it gets better.
   They say the state in plain words and nothing about patients or results;
   clicking one opens the window.
8. **Engine off screen.** Bundled in the app, with the engine's own
   `analyte.css`, `app.css` and fonts compiled in from `crates/analyte/assets`,
   no script and no remote loads. It states only what the app observed: when
   it last reached the engine, the state it saw then, what the last question
   and the service manager said. The app keeps checking and returns to the
   pages when the engine answers.
9. **Start engine.** The installer adds one entry to the service's access
   list, `(A;;RP;;;IU)`: interactive users may start the service, never stop
   or change it. When the service is stopped, the Engine off screen offers
   Start engine, a plain form post accepted only from the app's own page; the
   app runs `sc.exe start Analyte` within 10 s and shows the outcome, or who
   to ask when Windows refuses. The app never runs the engine itself.
10. **Start at sign-in** is on after install for the user who installs (the
    installer's ticked choice) and switched in the tray. It is one value,
    "Analyte", in that user's `HKCU\...\Run`, holding the quoted path and
    `--tray`, written by the installer and by the app alike; uninstall removes
    the installing user's value.
11. **The app's own title bar.** The window has no Windows frame; a 32 px bar
    in the design's look sits on top: the Aperture mark from the hand-drawn
    16 and 20 px files, "Analyte", a dot with the tray's four states and
    their words on hover, and minimize, maximize or restore, and close at
    Windows 11's 46 by 32 px, close turning red on hover. The window is two
    webviews:
    - **the bar** (label `title-bar`), the app's own page
      `http://analyte.localhost/title-bar` with one script file. It is the
      only webview any capability names: `capabilities/title-bar.json`
      grants it, from the app's own origin only, `minimize`,
      `toggle-maximize` and `close`, and nothing else (ADR 0022 dropped
      `start-dragging` and `internal-toggle-maximize`). It may navigate only
      to its own page and asks the app nothing: the app pushes the state, the
      button's name and its look into it.
    - **the pages** (label `pages`), the engine's pages or Engine off, below
      the bar, with no capability, as before. The engine is unchanged.
    The app serves the bar's page only to the bar's webview, and Start engine
    only to the pages'. Windows keeps the frameless window's shadow, resizing
    from every edge (Tauri adds a strip above the bar for the top edge),
    snapping by dragging to the screen's edges and with Win+arrows and
    Win+Z, and Alt+F4. The bar is a Windows caption through WebView2's
    `app-region`, and the snap layouts on the maximize button and the window
    menu on a right-click or Alt+Space are back through
    [ADR 0022](0022-title-bar-snap-layouts-and-window-menu.md). The
    multi-webview API is behind Tauri's `unstable` feature, so Tauri is
    pinned exactly. The reviewed versions, in `Cargo.lock`, are
    tauri 2.12.7, tauri-runtime 2.12.4, tauri-runtime-wry 2.12.5 and
    wry 0.57.3; a test fails on any other version until the bar has been
    tried on it and this list is updated.

## Why

A separate small program keeps the engine untouched and running without a
login, while the lab manager gets a window and a state at a glance. Showing
the engine's own pages keeps one set of pages, one sign-in and one place for
every rule. A coarse public state is enough for a tray icon, and the sign-in
page already says the engine runs, so it reveals nothing new.

## Consequences

- Tauri brings a large dependency tree on Windows; `cargo deny` checks it.
  Four unmaintained crates used only by Tauri's Linux desktop code are
  recorded in `deny.toml` as never built.
- The app's window needs the WebView2 Runtime. Windows 11 has it; on a
  Windows 10 PC without it, setup says so and opens the pages in the browser,
  and so does the app when it cannot make its window.
- Any program on the PC can read the coarse state from `/status`, as it could
  already see the sign-in page.
- WebView2 sends a link's request before the app's navigation check can
  cancel it, and cookies ignore the port, so a link to another port on
  127.0.0.1 would reach it with the admin session. The pages' webview runs
  one app script at the start of each page that turns a click on a link to
  another origin into a new-window request, which the app answers before
  anything is sent (app test round A). A browser sends that cookie to every
  127.0.0.1 port, typed addresses included.
- The app trusts whatever answers at 127.0.0.1 on the admin port as the
  engine, as a browser did; when the engine is stopped another local program
  could take the port.
- The title bar is a second webview with a capability of its own; the
  pages' webview still has none, and a test built from the real
  capabilities proves both. Tauri's `unstable` multi-webview API may change
  in any release, so upgrading Tauri means trying the bar again. The test
  programs link the Common Controls 6 manifest as the app does (`build.rs`),
  which Tauri's mock runtime needs to start on Windows.
- The bar's page is another origin, so WebView2 runs it in one more
  renderer process. Measured on 2026-11-20 on a 4K screen at 200 %, as the
  private working set of the app and its WebView2 processes: the sign-in
  page went from 83 MB (7 processes) to 102 MB (8), the Overview with live
  updates from about 95 to 100 MB to about 108 to 113 MB, and the tray
  alone stays at about 3 MB (the app only).
- Closing the window ends every WebView2 process of the app; measured on
  2026-11-20, the app and its WebView2 processes went from 461 MB working set
  (7 processes) with the Overview open to 40 MB (the app alone) in the tray.
