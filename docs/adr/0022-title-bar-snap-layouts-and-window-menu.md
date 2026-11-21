# 0022. Snap layouts and the window menu for the app's title bar

Status: accepted, 2026-11-21 (the owner asked for both behaviours back and
for the recommended route without waiting for approval). Refines
[ADR 0019](0019-desktop-app.md) decision 11, and
[ADR 0021](0021-windows-sign-in-pipe.md) decision 8: the app crate now has
one module that may use `unsafe`.

## Context

The app's window has no Windows frame; its title bar is a webview of its own
(ADR 0019 decision 11). Two Windows 11 behaviours of a framed window were lost
with it: the snap layouts flyout when the mouse rests on the maximize button,
and the window menu (Restore, Move, Size, Minimize, Maximize, Close) on a
right-click of the bar or on Alt+Space. Win+Z and Win+arrows still worked,
as Windows handles them for any window.

What we found on 2026-11-21, against the pinned Tauri 2.12.7 and wry 0.57.3:

- **`app-region`.** wry turns on WebView2's `IsNonClientRegionSupportEnabled`
  for every webview it makes. With it, the CSS `app-region: drag` makes
  WebView2 treat that part of the page as the window's caption: Windows drags
  the window, maximizes or restores it on a double-click and shows the window
  menu on a right-click, through small windows WebView2 lays over the area.
  Tried live: all three work in the bar.
- **Caption buttons.** WebView2 also names region kinds for the minimize,
  maximize and close buttons, but reports them only to an app that hosts it
  through the composition controller (`GetNonClientRegionAtPoint`), and no
  CSS value maps an element to them. wry hosts WebView2 in a window of its
  own, and Tauri 2.12 has no setting for it (its title bar styles are for
  macOS). Windows offers snap layouts only over a window whose hit test
  answers "maximize button" (`HTMAXBUTTON`); the webview's own windows belong
  to WebView2, partly in its browser process, so the app cannot make them
  answer that.
- **Plugins.** `tauri-plugin-decorum` (MIT, last release 1.1.1 in
  September 2024, "mostly in maintenance mode") presses Win+Z for the user
  through a key-injection crate. `tauri-plugin-snap-layout` (MIT, 1.0.9) and
  `tauri-plugin-frame` (MIT, 1.1.8, a fork of decorum) lay a native window
  over the maximize button, as below, but each is a young crate with one
  maintainer that brings its own unsafe code, commands and page permissions,
  and its own copy of Tauri's resize handling.
- **Alt+Space.** The keyboard goes to WebView2, not the window: logging the
  window's messages showed no `WM_SYSKEYDOWN` or `WM_SYSCHAR` for Alt and a
  letter. WebView2 reports such keys to the app through its
  `AcceleratorKeyPressed` event.
- **The menu's items.** WebView2 shows the window menu on a right-click
  without Windows first greying what the window cannot do, so the items kept
  the state of the last time Windows showed the menu itself: Restore offered
  on a normal window, Maximize, Move and Size on a maximized one.

## Decision

1. **The bar is a caption.** `.title-bar` has `app-region: drag` and its
   buttons `no-drag`. Tauri's `data-tauri-drag-region` and the permissions it
   needed, `start-dragging` and `internal-toggle-maximize`, are gone: the bar
   keeps `minimize`, `toggle-maximize` and `close` only, and the pages still
   have none.
2. **A native maximize button for snap layouts.**
   - A child window of class `AnalyteMaximizeButton`, never painted, lies
     exactly over the bar's maximize button
     (`title_bar::maximize_button`, two 46 px buttons from the right, rounded
     to whole pixels), just below Tauri's top-edge resize strip, so the edge
     above the button still resizes. It is placed again on every size or
     scale change.
   - It answers Windows' hit test with `HTMAXBUTTON` and keeps the press to
     itself: Windows' own handling of a press would act on the child.
     It reports each move, press, release and leave to the shell.
   - The shell gives the page's button its hover and pressed looks through
     `data-pointer`, and maximizes or restores on a release over a press
     (`title_bar::Pointer`); leaving the button ends a press.
   - The keyboard still reaches the page's button (Tab, then Enter or Space),
     which runs `toggle-maximize` as before.
3. **Alt+Space.** A handler on each webview's `AcceleratorKeyPressed` takes
   the first Alt+Space (not its repeats), marks it handled and posts
   `WM_SYSCOMMAND` `SC_KEYMENU` to the window, which opens the window menu as
   the key does in a framed window.
4. **The menu's items.** When the window opens and whenever it is maximized
   or restored, the app greys what a framed window's menu greys
   (`title_bar::Sizing::offers`): Restore on a normal window; Move, Size and
   Maximize on a maximized one.
5. **The exception.**
   - All of the app's `unsafe` lives in `crates/app/src/shell/frame.rs`,
     Windows only, behind the safe `add_maximize_button`,
     `place_maximize_button`, `menu_on_alt_space` and `match_window_menu`.
     The `analyte-app` library root says `#![deny(unsafe_code)]`; only
     `frame` carries an `#[expect(unsafe_code, reason = ...)]`.
   - Its calls: `GetModuleHandleW`, `RegisterClassExW`, `CreateWindowExW`,
     `DestroyWindow`, `GetWindowLongPtrW`, `SetWindowLongPtrW`,
     `SetLastError`, `DefWindowProcW`, `FindWindowExW`, `SetWindowPos`,
     `TrackMouseEvent`, `GetWindowThreadProcessId`, `GetCurrentThreadId`,
     `PostMessageW`, `GetSystemMenu`, `EnableMenuItem`; and WebView2's
     `add_AcceleratorKeyPressed`, the event arguments' readers and
     `SetHandled`. Every block has a `SAFETY:` comment with its precondition.
   - The button's window is made on the window's own thread, which the code
     checks and otherwise refuses. Its state is stored after the window is
     made and freed exactly once, on `WM_NCDESTROY`. The procedure reaches the
     shell through its own reference to the shell's callback, so nothing the
     shell does can leave the procedure holding a freed one. A panic cannot
     unwind out of the procedure: Rust aborts instead.
6. **Dependencies.** `webview2-com` 0.39.1 (MIT), the WebView2 bindings wry
   already uses, so `Cargo.lock` gains no new package; and `windows-sys`
   0.61 in the app with only the `Win32_Foundation`,
   `Win32_System_LibraryLoader`, `Win32_System_Threading`,
   `Win32_UI_Input_KeyboardAndMouse` and `Win32_UI_WindowsAndMessaging`
   features.

## Why

The supported route goes first: `app-region` is WebView2's own, the pinned
wry already turns it on, and it brings back dragging, the double-click and
the right-click menu with fewer permissions than before. No supported API
gives a windowed WebView2 snap layouts or Alt+Space. The native window over
the button is the technique Tauri itself uses for its resize strip and the
plugins use for snap layouts, here in one module with no new package; a
plugin would add more code, commands and permissions from a younger source
than these few calls. Pressing Win+Z for the user would depend on focus and
timing and inject input the user did not give.

## Consequences

- Hovering the maximize button shows Windows' snap layouts, maximized or
  not; the window menu opens on a right-click of the bar and on Alt+Space
  with the right items offered; dragging, the double-click, Win+Z and
  Win+arrows work as before.
- The mouse over the maximize button reaches the native window, not the
  page: the button shows no "Maximize" tooltip (Windows shows the flyout, as
  on a framed window), and a right-click on it does nothing.
- A launcher that claims Alt+Space for itself (PowerToys Run, Raycast and
  the like) gets the key before any app, this one included.
- Memory is unchanged: the button is a plain window in the app's process,
  not another webview. Measured on 2026-11-21 on a 4K screen at 200 % with
  the Overview open, as the private working set of the app and its WebView2
  processes over four samples per start: before, 109 to 135 MB (most samples
  109 to 115 MB, 8 processes); after, 107 to 122 MB (most samples 107 to
  113 MB, 8 processes).
- The code relies on two things of the pinned versions: wry turning on
  WebView2's non-client regions, and the class name of Tauri's resize strip
  (without it the button lies on top and the edge above it stops resizing).
  The version test of ADR 0019 already makes any upgrade try the bar again;
  the hand checks are: hover the maximize button, right-click the bar,
  Alt+Space, double-click and drag the bar, and resize from the top edge
  above the buttons.
- The decisions that hold logic (the button's area, its looks and click, the
  menu's items) are tested on every platform. Miri cannot run Windows FFI.
- Any further unsafe code needs its own ADR.
