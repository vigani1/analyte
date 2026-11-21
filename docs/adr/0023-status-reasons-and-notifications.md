# 0023. Notifications that say what happened

Status: accepted, 2026-11-21 (the owner asked for notifications that make
sense in the Windows notification centre, and for the recommended route
without waiting for approval). Refines
[ADR 0019](0019-desktop-app.md) decisions 5, 7 and 11.

## Context

The app's notifications said only the state ("A problem needs you") and
"Click to open Analyte and see what." In the Windows notification centre that
told nobody what had happened, and opening the app later did not say what the
notification had been about. Three causes:

- `GET /status` answered one word, so the app could say no more.
- A click on a notification is reported to the app only while the toast is on
  screen (tauri-plugin-notification 2.8.1, `desktop.rs`). A click from the
  notification centre starts the app through its Start menu entry, which
  carries the app's identifier as its AppUserModelID; the single-instance
  plugin hands that second start to the running app, which brought the window
  forward on whatever page it showed.
- Nothing kept the last notification, so a stale one in the centre could not
  be matched to anything in the window.

## Decision

1. **`/status` names kinds, never things.** The answer becomes
   `{"state": "...", "reasons": [...], "version": "..."}`. `reasons` holds a
   word from a closed list for each kind of line on the Overview's verdict,
   each once, most serious first: `disk_full`, `disk_low`, `lab_app_blocked`,
   `sending_failing`, `machine_stopped`, `machine_link_off`, `sending_paused`,
   `machine_silent`, `result_waiting`, `needs_review`, `orders_uncertain`,
   `orders_cancelled`, `orders_paused_after_restore`, `order_list_stale`.
   Every kind that stops data now comes before every kind that only needs a
   look. No names, counts, identifiers or values (safety rule 10); the route
   stays public. The kinds come from the same lines as the verdict
   (`overview::Summary`), so the tray, a notification and the Overview never
   disagree. `state` and `version` are unchanged; `reasons` is empty for
   `good` and `unknown`. A full disk is `disk_full`; the disk-reserve alert
   alone is `disk_low`, as the Overview already said "full" and "low".
2. **The app reads kinds within bounds.** It reads at most 32 entries of an
   answer of at most 1024 bytes, keeps a set of the 14 kinds it knows and
   leaves out words a newer engine may add. An answer without `reasons` (an
   engine from before this ADR) reads as no kinds; `good` or `unknown` with
   kinds is not the engine's answer.
3. **When a notification is sent.** As before, when the state gets worse.
   Also when a kind the app has not told of appears while the state is not
   good, once per kind: a kind that goes and comes back is not told again
   until the state has been good. Never for the first answer, never when
   things get better; the engine counting as off still needs two missed
   answers.
4. **What it says.** The title names the most serious new kind (or, with
   none new, the most serious listed) as a short sentence, such as "A
   machine’s connection is off" or "The disk is full: Analyte is refusing new
   data". The body says what to do ("Open Analyte to see which machine and
   what to check.") and, when more kinds are listed, "1 more thing needs a
   look too." With no kind the app knows, the title is the state as before.
   The words are in `engine::status::Reason`.
5. **A click opens the Overview.** A click on the toast, and a second start
   of the app (which is what a click in the notification centre does), open
   the window on the Overview, the page that lists each thing needing
   someone, even when the window was open on another page. "Open Analyte" in
   the tray keeps the page the window was on.
6. **The title bar keeps the last notification.** After the state dot, a
   short muted line: "Notified 14:32 · A machine’s connection is off", and
   once the app sees that kind gone, "· Fixed since 14:40". It goes back to
   unfixed if the kind returns, a newer notification replaces it, the engine
   being off does not count as fixed, and the line goes a day after the fix.
   It lives only in the app's memory, so it starts empty after the app
   restarts.

## Why

The kinds are what the owner needed to read in the notification centre, and
a closed list keeps the unauthenticated route free of patient or site data.
Deriving them from the verdict's own lines keeps one rule for "what needs
someone". The title bar is the app's own page, always visible above the
engine's pages, so the line needs no change to the engine's pages and
matches the notification's own words. The Overview then shows the details.

## Consequences

- Another program on the PC can now read which kinds of thing need someone,
  not only the coarse state; it still learns no names or counts.
- Adding a kind means adding a word to the engine's `Reason` and the app's,
  with its title and advice; an older app ignores a new word.
- Whether Windows starts the installed app for a click in the notification
  centre is checked by hand on the installed build: a build run from
  `target\` has no AppUserModelID, so its toasts show under another app's
  name and a click from the centre does not reach it.
