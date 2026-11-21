# 0025. Setting up machines and lab apps from the command line

Status: accepted, 2026-11-21 (the owner asked for the Setup and Lab apps pages'
steps from PowerShell for scripted installs and remote support, and for the
recommended route without waiting for approval).

## Context

Machines and lab apps were set up on the admin Setup and Lab apps pages, or
by editing TOML by hand and running `check-config`. IT staff need the same
from a terminal, for scripted installs and for remote support, without a
second set of rules that could drift from the pages.

## Decision

1. **One code path.** `analyte machines list | models | ports | add | set |
   test | remove` and `analyte lab-apps list | add | set | keys | remove` call
   the same functions as the pages: the plan and its checks, the dated
   backups, the safe write, the reload with roll back, the Remove dialog's
   "still unfinished" counts and the Lab apps page's refusals. The setup state
   (`admin::setup::Setup`) no longer depends on the web server, so both use it.
   Refusals keep the pages' words, printed as "Fix this: …".
2. **Audited as the person who ran it.** Every change writes the same audit
   row as the page, naming `--actor` and starting "command line". A change
   without `--actor` is refused, or asked for at a terminal.
3. **Ask only a person.** A missing choice is asked for, step by step, only
   when standard input and output are both a terminal; then a summary and
   "Save? [y/N]". Scripts and piped input are never prompted: a missing
   option is the page's own refusal. `--dry-run` makes every check and writes
   nothing.
4. **Keys never in arguments.** Keys are read only from a prompt that shows
   nothing as it is typed (the `rpassword` crate, approved for this) or from
   standard input, one per line. They go to the locked keys file exactly as
   the page stores them. Before asking, the command checks that this account
   may write that file (`Locked::check_writer`): an existing file must pass
   its usual checks and be readable here, and a missing folder may be created
   only by the account the service runs as. Otherwise it says to run as that
   account or use the Lab apps page. The file's protection is never weakened.
   On an installed Windows service, whose account is the virtual
   `NT SERVICE\Analyte`, that means keys are set on the Lab apps page.
5. **Live state from the running engine.** Link states live only in the
   engine, so `machines list` reads them from `GET /machines.json`: the
   Machines list's rows in the page's words, behind the same admin key or
   signed-in session as every page (not public, no payloads). The command
   reads the admin key from the protected data directory and never prints it.
   When nothing listens on the admin address it says "Engine not running: live
   state unknown"; any other failure says why. A state is never guessed.
   `machines test` uses the same answer to know which ports the engine holds;
   when it cannot tell, every machine in the settings counts as running.
   **The engine proves itself before the key is sent.** Threat: while the
   service is stopped its loopback admin port is free, and any local account
   can listen there and catch the admin key, which gives full admin control,
   the moment someone runs the command. So the command first sends a fresh
   256-bit random nonce to `GET /proof?nonce=…`. The engine answers
   HMAC-SHA256 under the admin key of the context `analyte-cli-proof-v1` and
   the nonce. The command checks it in constant time with the key it read
   from the protected data directory, and sends the key only on a match. A
   wrong answer, an error or no answer means "something else answers on the
   admin address" (never "not running"), and the key is not sent. `/proof`
   is public like `/status`, with the same Host and Origin checks, connection
   slots, request budget and `no-store`; it takes only 32 to 128 lowercase hex
   digits and answers 32 bytes. A proof for a nonce someone else chose is of
   no use, since every challenge is new. A challenge was chosen over checking
   which process owns the port: that needs OS-specific calls (new unsafe code
   on Windows) and still trusts a PID that can change between check and send,
   while the challenge proves possession of the key itself in safe Rust.
6. **The test's bytes, only when asked.** `machines test` is step 3's
   listen-only test with its limits (60 s, 64 KiB, nothing sent, nothing
   stored). Its bytes are dropped as on the page, except with `--show-bytes`,
   which first warns that they may hold patient data and then prints the kept
   bytes once to that person's console, never to a file or log.
7. **No restart from the command line.** The page's restart signals the
   running engine from inside it; the command cannot reuse that safely. Each
   change ends with how to apply it (`Restart-Service Analyte`).
8. **Exit codes.** 0 done (for a test: the model's format was seen), 1 refused
   or failed with nothing changed, 2 wrong usage (clap), 3 the change was
   written but its audit row or its keys were not, 4 the test ran but did not
   see the model's format.

## Consequences

- A rule changed for the page changes for the command line too; there is one
  place per rule.
- The prompt for a machine that connects to Analyte does not list this PC's
  addresses yet. PR #308 adds them to the wizard (`admin/setup/addresses.rs`);
  once it is merged, the prompt offers them from that same module.
- A broken program file cannot be fixed from the command line, because its
  changes need the store for the audit row; `check-config` shows what is
  wrong, and the page or an editor fixes it.
