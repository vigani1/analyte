# 0021. Windows sign-in for the Analyte app through a local pipe

Status: accepted, 2026-11-21 (approved by the owner, with the unsafe exception
in decision 8). Refines [ADR 0017](0017-windows-access-list-unsafe.md), whose
shared Windows helpers now live in the module this record adds.

## Context

People reach the admin pages in the Analyte app (ADR 0019) by pasting the
shared admin key. The key names nobody, so every change is recorded as
"admin session", and everyone who needs the pages must know the one secret.
The app runs as the signed-in Windows user and must not read the key file
(ADR 0017). The engine runs as `NT SERVICE\Analyte` and serves HTTP only on
127.0.0.1. HTTP cannot tell the engine which Windows user is asking; a local
named pipe can, because Windows gives the pipe's server the client's token.
Reading that token needs Win32 calls, which need `unsafe`, and no single
maintained crate wraps them safely: tokio has the pipe but not impersonation;
`interprocess` impersonates only on blocking streams and ignores a failed
revert in release builds; `winsafe` reads tokens but cannot impersonate.

## Decision

1. **The pipe.**
   - The engine on Windows creates `\\.\pipe\analyte-sign-in-<admin port>`
     with tokio: first instance, one instance at most, remote clients
     rejected. It holds that one instance for its whole life, disconnecting
     and connecting it again per caller, so the name is never free.
   - Access list `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)`: SYSTEM and
     Administrators full control, interactive logons read and write.
     Generic write includes the right to add a pipe instance (it is the same
     bit as appending data), so the list cannot withhold it from a client
     that writes; holding the only instance is what keeps a second one out.
   - If the name is taken at start, Windows sign-in is off with that reason,
     which the engine keeps and later shows on the Overview and Settings
     pages; the engine and the admin key keep working.
   - The same holds when the engine runs from a console (`analyte.exe run`)
     as a signed-in person: the pipe belongs to that person's process and
     answers the same way.
2. **Who is asking.**
   - The app connects at identification level (tokio's default) and writes
     one request line of at most 1 KiB within 2 s:
     `{"v":1,"next":"/parked/12"}`. Unknown fields, other versions and
     anything but one JSON object are refused (`bad-request`).
   - On a short-lived thread of its own, never a runtime or pool thread, the
     engine impersonates the client on a duplicate of the pipe handle, opens
     the thread token as itself, reads the user and groups and reverts. Only
     after the revert does it look up `DOMAIN\name`; without a name in time
     the caller is shown by SID, never by a guess.
   - The client's word is never trusted for identity.
   - The answer is one line within 1 s:
     `{"code":"…","account":"LAB-PC\\maria"}` or
     `{"refused":"not-allowed","account":"LAB-PC\\bob"}`; the other refusal
     words are `bad-request`, `unidentified` and `unavailable`. A caller that
     sends nothing is dropped after 2 s; each connection ends within about
     3 s.
3. **Who may sign in** (the owner's choices, 2026-11-21).
   - Members of the local group `<COMPUTERNAME>\Analyte users`, enabled in
     the token. The name is qualified with the computer and must be a local
     group, so a same-named domain group or account never matches. Without
     the group only Administrators may sign in, and the engine says why.
   - `BUILTIN\Administrators`, **enabled or deny-only**: an administrator's
     normal, unelevated token carries the group as deny-only, and UAC is not
     a security boundary.
   - **Remote Desktop users are allowed**: an RDP logon is interactive, and
     the access list has no deny entry for REMOTE INTERACTIVE (`S-1-5-14`).
   - Setup creates the group and adds the installing user; uninstall leaves
     it. Membership counts from the person's next Windows sign-in.
4. **The code.**
   - An allowed client gets a code: 256 random bits, single use, valid 20 s,
     at most 8 pending, in memory only. It carries the account and a local
     return path, checked when the code is kept.
   - `GET /login/windows?code=` exchanges it for a normal session and
     redirects to the path. Everything else about sessions is unchanged:
     1 h, 128 at most, CSRF on every form, the same Host and Origin checks.
5. **Sessions say who.**
   - A session is pending, signed in with the key, or signed in as a Windows
     account. The sidebar shows "Signed in as DOMAIN\name".
   - The audit actor of a Windows session is `windows:<SID>:<DOMAIN>\<name>`
     (or `windows:<SID>` when the name could not be found). Names cannot hold
     `:`, so the spelling reads back; a value over 256 bytes is refused, never
     cut. The command line refuses actors beginning `windows:` or
     `admin-session-`. The stored schema does not change.
   - **A shared Windows account is recorded as that one account**: Windows
     sign-in cannot tell the people who share it apart.
6. **The app signs in by itself.**
   - Its pages webview carries the user-agent mark `AnalyteApp`. The engine
     adds a refresh to `/login/windows` on the 401 challenge for such
     requests, never on `/login`, so sign-out stays signed out.
   - The app takes that address, asks the pipe and opens the code. It tries
     automatically at most once per 30 s. A refusal shows the key form with
     a fixed sentence.
   - Only the engine's refresh counts as automatic. A click on "Sign in with
     Windows" always asks (once per click, so it cannot loop) and starts the
     30 s again; opening the window clears them, so it signs in at once.
     So does the engine answering again after it did not: a restarted
     engine has no session, so the window signs in at once there too.
     No pipe, no answer within 3 s for a busy pipe plus 2 s for the answer,
     or an answer that is not a valid line all show the plain key form; a
     missing pipe is not a refusal.
7. **The key stays.** The admin key keeps working in browsers, with
   `Authorization: Bearer` and for recovery.
8. **The exception.**
   - All new `unsafe` lives in `crates/analyte/src/runtime/service/caller.rs`,
     Windows only, behind the safe `create_pipe`, `identify`, `account_sid`
     and `process_account`. ADR 0017's shared helpers (wide strings, SID
     text, the token and `LocalFree` guards, account lookup by name) moved
     there from `acl.rs`, so both modules use one copy and ADR 0017's checks
     behave as before.
   - The `analyte` library root says `#![deny(unsafe_code)]`; only `acl` and
     `caller` carry an `#[expect(unsafe_code, reason = ...)]`. Every other
     crate keeps `#![forbid(unsafe_code)]`.
   - Its calls: building the pipe's security descriptor and creating the pipe
     with it; `ImpersonateNamedPipeClient`, `OpenThreadToken`,
     `RevertToSelf`; `GetTokenInformation` (user, groups);
     `ConvertSidToStringSidW`, `LookupAccountSidW`, `LookupAccountNameW`.
     Every block has a `SAFETY:` comment with its precondition.
   - Dependency: `windows-sys` 0.61 gains only `Win32_System_Pipes`.
9. **Linux** keeps the admin key only.

## Why

The pipe is the one local channel where Windows itself vouches for the
caller, refuses the network and lets the service see the user without being
able to act as them. A one-time code keeps every session born in the
engine's one HTTP path and leaves the cookie, CSRF and origin rules
untouched. HTTP Negotiate would need more unsafe code, sign in every browser
silently and invite local relay attacks; a group-readable key file would
still be one shared, nameless secret.

## Consequences

- Anything running as an allowed, signed-in person can open the pages as that
  person without typing. This is the price of silent sign-in; the audit names
  the account and sessions last at most 1 h.
- While the engine is stopped, an allowed local user could take the pipe's
  name and sign the app in as themselves; the sidebar shows their name and
  the engine refuses Windows sign-in at its next start.
- One local user can make silent sign-in slow by holding the pipe; each hold
  ends within 3 s and the key still works.
- The ordinary log names a refused caller by SID only, and nothing on success.
- Miri cannot run Windows FFI; Windows CI proves the pipe with real local
  users made for the test. Remote Desktop cannot be tested in CI.
- Any further unsafe code needs its own ADR.
