# 0017. Unsafe code for Windows access lists

Status: accepted, 2026-11-18 (the owner approved `windows-sys` for this check).
Refined by [ADR 0021](0021-windows-sign-in-pipe.md): a second module,
`caller.rs`, may now use `unsafe`, and the helpers both share moved there.

## Context

Safety rule 10 keeps patient data only in the protected store. On Unix the data
directory is created with mode 0700 and checked at startup; on Windows nothing
was checked, so a hand-made folder under `C:\ProgramData` let every user of the
PC read raw messages, and the admin token file had the same gap. Windows access
lists can only be read and written through Win32 calls, which need `unsafe`.

## Decision

- **The rule:** the data directory, directories the program creates in it and
  the admin token file pass only when their owner and every allow entry,
  explicit, inherited or inherit-only, name SYSTEM (`S-1-5-18`),
  Administrators (`S-1-5-32-544`), the process account or, when installed,
  `NT SERVICE\Analyte`. Deny entries are ignored; a missing access list or any
  other entry type is refused. A refusal names the offending SID, never data.
- A directory the program creates gets the installer's shape: no inheritance,
  SYSTEM and Administrators full control, the process account modify. An
  existing directory is only checked, never rewritten.
- The locked credentials folder (`<data>/credentials`) uses a stricter shape of
  the same code: it grants only SYSTEM and the process account, and its check
  refuses any other grant, Administrators included (2026-11-18).
- **The exception:** all `unsafe` lives in
  `crates/analyte/src/runtime/service/acl.rs`, Windows only, behind the safe
  `create_private`, `check` and `process_account`. The workspace lint is
  `deny` and the `analyte` library root says `#![deny(unsafe_code)]`; every
  other crate keeps `#![forbid(unsafe_code)]`, and only the `acl`
  module carries an `#[expect(unsafe_code, reason = ...)]`. Every block has a
  `SAFETY:` comment covering its preconditions: NUL-terminated wide strings we
  built, live out pointers with their lengths, API memory freed once by an
  owning guard, the token handle closed once, ACE indexes below the reported
  count with unaligned header reads, and SID text read with a fixed bound.
- Dependency: `windows-sys` 0.61, Windows only, with only the features used.

## Why

Who can read the data matters, not where a grant came from, and the standard
library has no safe API for Windows access lists.

## Consequences

- Windows refuses to start, or to load the admin token, when other users can
  reach protected data; installs made by `install.ps1` pass unchanged.
- Miri cannot run Windows FFI; Windows CI tests change access lists with
  `icacls` instead.
- Any further unsafe code needs its own ADR.
