# 0017. Unsafe code for Windows access lists

Status: accepted, 2026-11-18 (the owner approved `windows-sys` for this check).

## Context

Clinical safety rule 10 keeps patient data only in the protected store. On
Unix the program creates the data directory with mode 0700 and refuses to
start if group or other can reach it. On Windows it created the folder with
`create_dir_all` and checked nothing. A folder under `C:\ProgramData` that was
made by hand, or changed after installation, normally inherits read access for
BUILTIN\Users, so every user of the PC could read raw messages. The admin
token file had the same gap: only Unix checked its mode.

Windows access lists (owner and DACL) can only be read and written through
Win32 security calls. The standard library exposes no such API, and the
approved dependencies offer no safe wrapper for it. That needs `unsafe` FFI,
which AGENTS.md forbids without an ADR.

## Decision

### The rule

A protected path (the data directory, any directory the program creates in
it, and the admin token file) passes only when its owner and every allow entry,
explicit **or inherited**, name one of:

- SYSTEM (`S-1-5-18`);
- BUILTIN\Administrators (`S-1-5-32-544`);
- the account the process runs as;
- `NT SERVICE\Analyte`, when that service is installed, so an administrator's
  commands work on the installed service's data.

Inherited entries are judged like explicit ones: what matters is who can read
the data, not where the grant came from. Children of the installer's protected
root inherit only allowed principals and pass; a folder inheriting Users from
`C:\ProgramData` fails. Inherit-only entries are judged too, since they govern
files created later. Deny entries only remove access and are ignored. A
missing access list (everyone has access) and any other entry type (object or
conditional entries) are refused rather than interpreted. A refusal names the
offending SID, never a payload.

When the program creates the final directory itself, it creates it with the
installer's shape: protected (no inheritance), SYSTEM and Administrators Full
Control, the process account Modify. A fresh foreground run is then private by
default. An existing directory is only checked, never rewritten.

### The unsafe exception

- All `unsafe` lives in `crates/analyte/src/runtime/service/acl.rs`, compiled
  only on Windows. Its safe API is `create_private(path)`, `check(path)` and
  `process_account()`; callers never see a raw pointer.
- The workspace lint `unsafe_code` is `deny` instead of `forbid`, because a
  forbid set by Cargo cannot be lifted locally. Every crate root still says
  `#![forbid(unsafe_code)]`, except the `analyte` library, which says
  `#![deny(unsafe_code)]`; only the `acl` module declaration carries
  `#[expect(unsafe_code, reason = ...)]`. Clippy's `undocumented_unsafe_blocks`
  requires a `SAFETY:` comment on every block.
- Safety preconditions, each stated at its block:
  - strings passed to Windows are NUL-terminated wide strings we built, with
    interior NULs refused;
  - every out pointer is a live local or a buffer with the length passed;
  - memory the API allocates (security descriptor, SID text) is freed once
    with `LocalFree` by an owning guard, and pointers into a descriptor are
    used only while that guard is alive;
  - the token handle is closed once by its guard;
  - ACE indexes stay below the count the API reports, and ACE headers are
    read unaligned;
  - SID text is read up to its terminator with a fixed bound.
- Dependency: `windows-sys` 0.61 (already in `Cargo.lock`) as a Windows-only
  direct dependency with only the features used.

## Consequences

- Windows gets the same guarantee as Unix: the program refuses to start, or to
  load the admin token, when other users can reach the protected data.
- Existing installations made by `install.ps1` pass unchanged. A directory set
  up by hand must be fixed by the operator; the error names the SID to remove.
- Miri cannot run Windows FFI, so this module is not Miri-tested. It is covered
  by Windows tests that change access lists with `icacls` and run in CI.
- Any further unsafe code needs its own ADR.
