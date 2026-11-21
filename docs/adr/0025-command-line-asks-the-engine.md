# 0025. The command line asks the running engine for the verdict

Status: accepted, 2026-11-21 (the owner asked that everything the admin pages
and the app show or do is also possible from PowerShell, with the recommended
route pre-approved). Builds on [ADR 0023](0023-status-reasons-and-notifications.md).

## Context

IT staff need the admin pages' views and actions as commands. Most pages read
only the store, which every operator command already opens beside the running
engine (the store's lock is shared). The Overview's verdict is different: a
machine whose service in Analyte stopped, or whose link is off, is known only
inside the running engine. A command that rebuilt the verdict from the store
alone would leave those lines out and could print "All good" while a machine
is down, which safety rule 9's spirit forbids: not knowing must be said as not
knowing.

## Decision

1. **Store pages are read from the store.** `activity`, `alerts list`,
   `data-keeping` and `orders cancelled` open the store like `parked list`
   and call their page's own read and wording functions; nothing is copied.
   Reads that show sample IDs (Activity, Cancelled by machine) are recorded in
   the audit with the `--actor`, as `orders list` is.
2. **The verdict comes from the engine.** The engine serves
   `GET /overview/verdict`: the Overview's lines as JSON (state, each line's
   words, kind, the machine or lab app it is about, the page and the command
   that fix it), built by the same rule as the page and `GET /status`. It is
   protected like every page: a signed-in session or the admin key as a bearer
   header. Its lines hold names and counts only, never values or sample IDs
   (safety rule 10). A state it cannot read answers 503 with what it could not
   read, never an empty list.
3. **The command line reads the admin key from the data directory** with the
   engine's own checks, sends it only to the loopback admin address, never
   prints it, uses no proxy, follows no redirect and stops after 5 seconds and
   256 KiB. An engine that does not answer, or an admin page that is off, is an
   unknown state: `analyte overview` exits 3 and says what to do.
4. **Exit codes for scripts:** `overview` 0 all good, 1 needs a look,
   2 a problem stops data, 3 unknown; `doctor` 0 OK, 1 warning, 2 failure.
5. **`doctor` changes nothing.** It never opens a serial port or sends a byte
   to a machine; a machine Analyte connects to gets one connection, closed at
   once, and only while the engine is stopped; a lab app gets one unsigned GET
   through the sink's own client settings (`analyte_sink::answers`); it never
   creates a store. Opening an existing store may upgrade an older layout, as
   every store command does.
6. **Windows itself owns the service and the users group.** Analyte wraps
   neither: `Start-Service Analyte` and `Add-LocalGroupMember -Group
   "Analyte users"` are documented in [commands](../operations/commands.md).
   `doctor` only reads whether the group exists, through the existing safe
   account lookup (ADR 0021), and whether the installer's firewall rule exists,
   with `netsh`.

## Consequences

- A page and its command cannot disagree: they share one read and one rule.
- `overview` needs the `[admin]` table and an administrator PowerShell (the
  admin key file is readable only by SYSTEM, Administrators and the service).
- One new small dependency, `clap_complete` (MIT or Apache-2.0, from the clap
  maintainers, no default features, depends only on `clap`), prints shell
  completion scripts.
