# 0028. Lab app keys from the command line through the running engine

Status: accepted, 2026-11-21 (the owner asked for an engine-only install set up
entirely from PowerShell, and for the recommended route without waiting for
approval). Refines point 4 of ADR 0025.

## Context

`analyte lab-apps add` and `lab-apps keys` store a lab app's two keys in the
locked keys file, which only the account the service runs as may write
(ADR 0025, point 4). On an installed Windows PC that account is the virtual
`NT SERVICE\Analyte`, which no person can run a command as, so the commands
refused there and the keys could be set only on the Lab apps page. An
engine-only installation (no desktop app) should not need a browser for this.

Every admin POST so far needs a signed-in browser session and its CSRF value;
the admin key in a `Bearer` header alone can read pages but not change
anything.

## Decision

1. **The engine stores the keys.** When the engine runs, `lab-apps add` and
   `lab-apps keys` send the lab app and its keys to
   `POST /command/lab-apps` on the loopback admin address. The engine runs
   the Lab apps page's own `add_blocking` or `save_blocking`: the same checks,
   dated backups, safe write and locked-file write, then the same audit row,
   naming the command's `--actor` and "command line". It answers what was
   stored (key names, never values) or the page's refusal.
2. **Only to the proven engine.** The command first asks `GET /proof` as in
   ADR 0025, point 5, and sends nothing, neither the admin key nor the lab
   app's keys, unless the listener proves it holds the admin key. An unproven
   listener is reported as such; the command does not fall back to writing
   the file itself.
3. **A bearer-only POST.** `/command/` routes are the one exception to "a POST
   needs a session and its CSRF value". They need the admin key in the
   `Authorization: Bearer` header; a session cookie does not admit them, so a
   page in a browser cannot post there with the browser's sign-in. CSRF
   protects against a browser sending its cookie on a cross-site request; a
   header that a cross-site page cannot set needs no such value. The usual
   Host and Origin checks still apply (any `Origin` header must be the admin
   page's own), and so do the connection slots, the setup write budget and
   `no-store`.
4. **Bounded, never logged.** The body is JSON of at most 16 KiB, read once;
   unknown fields are refused. The keys move into the credentials type at
   once; the body, the keys and the admin key are never logged or echoed.
5. **Engine stopped.** When nothing listens on the admin address the command
   stores the keys itself, as before, if this account may write the locked
   file (`Locked::check_writer`); otherwise it says to run
   `Start-Service Analyte` and the command again. Keys are still read only from
   a hidden prompt or standard input, and only once the command knows where
   they will go.

## Consequences

- The Lab apps page, the direct command and the engine route share one write
  path and one audit wording; only the caller differs.
- `lab-apps add` on an installed PC writes `lab-apps.toml` through the engine
  too, so the settings and keys of one lab app are written together, as on
  the page.
- The admin key in a header now allows one kind of change. It already gave
  every read and, through the login form, a full session, so this adds no
  power to whoever holds it.
