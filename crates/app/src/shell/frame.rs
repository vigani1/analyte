//! Windows-only: the frame features the webview title bar cannot give itself (ADR 0022): snap layouts on the maximize button, the window menu on Alt+Space, and that menu's items matching the window.
//!
//! The bar's page is a Windows caption through `WebView2`'s `app-region`, so
//! Windows already drags the window, maximizes it on a double-click and shows
//! the window menu on a right-click. Three things stay out of a page's reach:
//!
//! - **Snap layouts** open only over a window that answers Windows' hit test
//!   with "maximize button". A small native window, never painted, lies over
//!   the bar's maximize button and answers just that; it hands every mouse
//!   report to the shell, which shows the button's looks and clicks it.
//! - **Alt+Space** reaches `WebView2`, never the window, so a handler on each
//!   webview's `AcceleratorKeyPressed` takes it and asks Windows for the
//!   window menu, as the key does in a framed window.
//! - **The menu's items:** `WebView2` shows the window menu on a right-click
//!   without Windows first greying what the window cannot do, so the app
//!   greys them itself whenever the window is maximized or restored.
//!
//! This is the app's only `unsafe` code. Every call is behind the safe
//! functions below and every block says why it holds.
use crate::title_bar::{Area, MenuItem, Mouse, Sizing};
use std::cell::Cell;
use std::io;
use std::rc::Rc;
use webview2_com::AcceleratorKeyPressedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    COREWEBVIEW2_PHYSICAL_KEY_STATUS, ICoreWebView2Controller,
};
use windows_sys::Win32::{
    Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, SetLastError, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_SPACE,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, EnableMenuItem, FindWindowExW,
            GWLP_USERDATA, GetSystemMenu, GetWindowLongPtrW, GetWindowThreadProcessId, HTMAXBUTTON,
            HWND_TOP, MF_BYCOMMAND, MF_ENABLED, MF_GRAYED, PostMessageW, RegisterClassExW,
            SC_CLOSE, SC_KEYMENU, SC_MAXIMIZE, SC_MINIMIZE, SC_MOVE, SC_RESTORE, SC_SIZE,
            SWP_NOACTIVATE, SetWindowLongPtrW, SetWindowPos, WM_NCDESTROY, WM_NCHITTEST,
            WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SYSCOMMAND,
            WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
};

/// The native maximize button's window class, registered once per process.
const CLASS: &str = "AnalyteMaximizeButton";

/// The class of the strip Tauri lays along the window's top edge for
/// resizing (tauri-runtime-wry's `undecorated_resizing`). The button lies
/// just below it, so the edge above the button still resizes the window.
const RESIZE_STRIP: &str = "TAURI_DRAG_RESIZE_BORDERS";

/// Why a frame feature could not be added; each says which step failed.
#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    /// Tauri could not give the window's handle or reach its webview.
    #[error("Tauri could not reach the window: {0}")]
    Tauri(#[source] tauri::Error),
    /// The native button was asked for off the window's own thread, where
    /// its window procedure could not run.
    #[error("the maximize button's window must be made on the window's own thread")]
    Thread,
    /// The window has no native maximize button to place.
    #[error("the window has no maximize button window")]
    Missing,
    /// The window has no window menu to match.
    #[error("the window has no window menu")]
    NoMenu,
    /// A Win32 call failed.
    #[error("{call} failed: {source}")]
    Os {
        /// The failed call.
        call: &'static str,
        /// What Windows said.
        source: io::Error,
    },
    /// A `WebView2` call failed.
    #[error("WebView2's {call} failed with HRESULT {code:#010x}")]
    WebView2 {
        /// The failed call.
        call: &'static str,
        /// The HRESULT, as Windows writes it.
        code: i32,
    },
}

/// What the native button's window keeps: whom to tell about the mouse, and
/// whether Windows already watches for the mouse leaving.
struct State {
    notify: Rc<dyn Fn(Mouse)>,
    tracking: Cell<bool>,
}

/// Lays the native maximize button over the bar's own in `window`, unless
/// it has one, and tells `notify` of every mouse report on it. Place it with
/// [`place_maximize_button`]. The window procedure runs on the thread that
/// makes the button, so this must run on the window's own thread.
///
/// # Errors
///
/// When the window's handle cannot be read, this is not its thread, or
/// Windows refuses the class or the window.
pub(super) fn add_maximize_button(
    window: &tauri::Window,
    notify: impl Fn(Mouse) + 'static,
) -> Result<(), Error> {
    let parent = handle(window)?;
    // SAFETY: GetWindowThreadProcessId accepts any handle and a null process
    // out pointer; GetCurrentThreadId has no preconditions.
    if unsafe { GetWindowThreadProcessId(parent, std::ptr::null_mut()) != GetCurrentThreadId() } {
        return Err(Error::Thread);
    }
    let class = wide(CLASS);
    if find(parent, &class).is_some() {
        return Ok(());
    }
    // SAFETY: a null name asks for this program's own module, which lives as
    // long as the process.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let info = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        style: 0,
        lpfnWndProc: Some(procedure),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: module,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    // SAFETY: `info` is fully set and its class name is a NUL-terminated
    // string that outlives the call; Windows copies both.
    if unsafe { RegisterClassExW(&raw const info) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != i32::try_from(ERROR_CLASS_ALREADY_EXISTS).ok() {
            return Err(os("RegisterClassExW", error));
        }
    }
    // SAFETY: the class was registered above with this module; the parent is
    // the live window whose thread this is; there is no name, menu or
    // creation data. At 0 by 0 it covers nothing until it is placed.
    let button = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            parent,
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        )
    };
    if button.is_null() {
        return Err(os("CreateWindowExW", io::Error::last_os_error()));
    }
    let state = Box::into_raw(Box::new(State {
        notify: Rc::new(notify),
        tracking: Cell::new(false),
    }));
    // SAFETY: `button` was just made on this thread. SetWindowLongPtrW
    // returns the old value, 0 here, so the last error tells failure apart.
    let failed = unsafe {
        SetLastError(0);
        SetWindowLongPtrW(button, GWLP_USERDATA, state as isize) == 0
            && io::Error::last_os_error().raw_os_error() != Some(0)
    };
    if failed {
        let error = io::Error::last_os_error();
        // SAFETY: the state was never handed to the window, so it is still
        // only ours; the window is ours to destroy, on its own thread.
        unsafe {
            drop(Box::from_raw(state));
            DestroyWindow(button);
        }
        return Err(os("SetWindowLongPtrW", error));
    }
    Ok(())
}

/// Moves the native maximize button over `area` of `window`, just below
/// Tauri's resize strip.
///
/// # Errors
///
/// When the window's handle cannot be read, it has no native button, or
/// Windows refuses the move.
pub(super) fn place_maximize_button(window: &tauri::Window, area: Area) -> Result<(), Error> {
    let parent = handle(window)?;
    let button = find(parent, &wide(CLASS)).ok_or(Error::Missing)?;
    let after = find(parent, &wide(RESIZE_STRIP)).unwrap_or(HWND_TOP);
    let pixels = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    // SAFETY: both handles are children of the live window, or HWND_TOP;
    // SetWindowPos sends to the button's thread when called from another.
    let moved = unsafe {
        SetWindowPos(
            button,
            after,
            pixels(area.x),
            pixels(area.y),
            pixels(area.width),
            pixels(area.height),
            SWP_NOACTIVATE,
        )
    };
    if moved == 0 {
        return Err(os("SetWindowPos", io::Error::last_os_error()));
    }
    Ok(())
}

/// Greys the window menu's items that `window`, at `sizing`, cannot do and
/// offers the rest, as Windows does for a framed window before showing it.
///
/// # Errors
///
/// When the window's handle cannot be read or it has no window menu.
pub(super) fn match_window_menu(window: &tauri::Window, sizing: Sizing) -> Result<(), Error> {
    let parent = handle(window)?;
    // SAFETY: FALSE asks for the window's own menu, which Windows owns and
    // keeps for the window's life; a stale handle gives null.
    let menu = unsafe { GetSystemMenu(parent, 0) };
    if menu.is_null() {
        return Err(Error::NoMenu);
    }
    for item in MenuItem::ALL {
        let command = match item {
            MenuItem::Restore => SC_RESTORE,
            MenuItem::Move => SC_MOVE,
            MenuItem::Size => SC_SIZE,
            MenuItem::Minimize => SC_MINIMIZE,
            MenuItem::Maximize => SC_MAXIMIZE,
            MenuItem::Close => SC_CLOSE,
        };
        let state = if sizing.offers(item) {
            MF_ENABLED
        } else {
            MF_GRAYED
        };
        // SAFETY: the menu is the window's own, just read; an item the menu
        // lacks only makes the call return -1, which changes nothing.
        unsafe { EnableMenuItem(menu, command, MF_BYCOMMAND | state) };
    }
    Ok(())
}

/// Makes Alt+Space in `webview` open `window`'s window menu, as in a framed
/// window. The handler lives as long as the webview.
///
/// # Errors
///
/// When Tauri cannot reach the window or the webview. `WebView2` refusing the
/// handler happens later, on the window's thread, and is logged.
pub(super) fn menu_on_alt_space(
    webview: &tauri::Webview,
    window: &tauri::Window,
) -> Result<(), Error> {
    // A number, not a pointer, so the handler can cross to the window's thread.
    let parent = handle(window)? as isize;
    webview
        .with_webview(move |platform| {
            if let Err(error) = listen_for_alt_space(&platform.controller(), parent) {
                tracing::warn!(%error, "Alt+Space will not open the window menu");
            }
        })
        .map_err(Error::Tauri)
}

/// Adds the Alt+Space handler to one webview's controller.
fn listen_for_alt_space(controller: &ICoreWebView2Controller, parent: isize) -> Result<(), Error> {
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
        let mut key = 0_u32;
        let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
        // SAFETY: WebView2 calls this handler with live arguments; each out
        // pointer is a local that outlives its call.
        unsafe {
            args.KeyEventKind(&raw mut kind)?;
            args.VirtualKey(&raw mut key)?;
            args.PhysicalKeyStatus(&raw mut status)?;
        }
        let alt_space = kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            && key == u32::from(VK_SPACE)
            && !status.WasKeyDown.as_bool();
        if !alt_space {
            return Ok(());
        }
        // SAFETY: the arguments are live for the handler's call.
        unsafe { args.SetHandled(true)? };
        // Posted, so the menu's own message loop runs after this handler
        // returns rather than inside WebView2's call.
        // SAFETY: PostMessageW checks the handle and fails on a window that
        // is gone; the message carries no pointers.
        let posted = unsafe {
            PostMessageW(
                parent as HWND,
                WM_SYSCOMMAND,
                SC_KEYMENU as WPARAM,
                LPARAM::from(b' '),
            )
        };
        if posted == 0 {
            let error = io::Error::last_os_error();
            tracing::warn!(%error, "the window menu could not be asked for");
        }
        Ok(())
    }));
    let mut token = 0_i64;
    // SAFETY: the controller is live (Tauri hands it to us on its thread) and
    // the token is a local; WebView2 holds the handler until it closes.
    unsafe { controller.add_AcceleratorKeyPressed(&handler, &raw mut token) }.map_err(|error| {
        Error::WebView2 {
            call: "add_AcceleratorKeyPressed",
            code: error.code().0,
        }
    })
}

/// The native button's window procedure: it answers Windows' hit test as the
/// maximize button and reports the mouse; everything else is Windows'.
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Windows calls this procedure with its own window; the value is
    // 0 or the State that `add_maximize_button` stored.
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const State;
    if state.is_null() {
        // SAFETY: the arguments are Windows' own, passed on unchanged.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_NCDESTROY {
        // SAFETY: WM_NCDESTROY is the window's last message; the State is
        // taken out of it first, so it is freed exactly once.
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(state.cast_mut()));
            return DefWindowProcW(window, message, wparam, lparam);
        }
    }
    // SAFETY: the State lives until WM_NCDESTROY above; it is only read
    // here, and the shell is told after the last read, through its own
    // reference, so nothing it does can leave this one dangling.
    let (notify, tracking) = unsafe { (Rc::clone(&(*state).notify), &(*state).tracking) };
    let mouse = match message {
        WM_NCHITTEST => return LRESULT::try_from(HTMAXBUTTON).unwrap_or(0),
        WM_NCMOUSEMOVE => {
            if !tracking.get() {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: u32::try_from(size_of::<TRACKMOUSEEVENT>()).unwrap_or(u32::MAX),
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                // SAFETY: `track` is fully set for this window and outlives the call.
                tracking.set(unsafe { TrackMouseEvent(&raw mut track) } != 0);
            }
            Mouse::Move
        }
        // Not passed on: Windows' own handling of a press on a maximize
        // button acts on the window it reached, which is this child.
        WM_NCLBUTTONDOWN => Mouse::Down,
        WM_NCLBUTTONUP => Mouse::Up,
        WM_NCMOUSELEAVE => {
            tracking.set(false);
            Mouse::Leave
        }
        // SAFETY: the arguments are Windows' own, passed on unchanged.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    };
    notify(mouse);
    0
}

/// `window`'s handle.
fn handle(window: &tauri::Window) -> Result<HWND, Error> {
    window.hwnd().map(|hwnd| hwnd.0).map_err(Error::Tauri)
}

/// The first child of `parent` of the NUL-terminated `class`.
fn find(parent: HWND, class: &[u16]) -> Option<HWND> {
    // SAFETY: `class` is NUL-terminated and outlives the call; a stale parent
    // only makes the search fail.
    let child = unsafe {
        FindWindowExW(
            parent,
            std::ptr::null_mut(),
            class.as_ptr(),
            std::ptr::null(),
        )
    };
    (!child.is_null()).then_some(child)
}

/// `text` as a NUL-terminated wide string.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn os(call: &'static str, source: io::Error) -> Error {
    Error::Os { call, source }
}
