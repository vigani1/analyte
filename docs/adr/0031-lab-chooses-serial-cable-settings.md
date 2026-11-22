# 0031. The lab chooses its serial cable settings in Setup

Status: accepted, 2026-11-22 (the owner decided that serial cable settings are
standard values the lab chooses, with the manual's values as a suggestion, and
asked for the recommended route without waiting for approval). Refines
[ADR 0002](0002-config-first.md) (which layer owns what) and follows
[ADR 0025](0025-setup-from-the-command-line.md) and
[ADR 0027](0027-command-line-asks-the-engine.md) (one code path for the
pages and the command line).

## Context

A serial analyzer talks only when the PC uses the same baud rate, data bits,
parity, stop bits and flow control as the analyzer's own LIS (host) settings
screen. These are standard values, not machine code. Until now they came only
from the machine definition's `[transport.serial]`, so a lab whose analyzer
was set differently had to hand-edit the local overrides file, which replaces
the whole `[transport]` section. Setup could not show or change them, and the
connection test could not try other values. Some manuals do not state every
value (the Biolyte 2000's flow control is still unknown), so a definition
cannot always give all five.

## Decision

1. **The definition suggests, the lab chooses.** The definition's
   `[transport.serial]` holds the manual's usual values as a suggestion; any
   of the five may be left out. Setup step 2 shows each setting from its
   standard list, filled in with the saved choice or else the suggestion, with
   the hint "Match the analyzer's LIS/host settings screen", and marks values
   that differ from the suggestion. The connection test (step 3) opens the
   port with the chosen values.
2. **The choice lives in the site file's `[serial]`.** It is the lab's own
   file on the lab PC, the one Setup already writes (port and lab app) through
   its one validated write path (plan, checks, backups, safe write, reload
   with roll back). Each value there replaces the definition's value of the
   same key. The effective snapshot holds the resolved values under
   `transport.serial`, so the digest changes when a value changes and equal
   values give equal bytes from either layer. Save writes all five values, so
   a later definition update never silently changes a cable that works.
   The local overrides file was not chosen: it replaces the whole
   `[transport]` (kind and deadline too), Setup does not write it, and a
   second writer would break "one place per rule".
3. **Nothing is assumed.** A value that neither the definition nor the site
   file gives is refused by its key
   (`transport.serial.flow_control: no value`), with what to do.
4. **Standard lists only.** Baud rates are the ten that both Windows and the
   POSIX serial interface name, 300 to 115200
   (`analyte_transport::serial::BAUD_RATES`); data bits 5 to 8; parity none,
   odd or even; stop bits 1 or 2; flow control none, hardware (RTS/CTS) or
   software (XON/XOFF). The config loader checks every layer against these
   lists, and the site's `[serial]` accepts no other key, so it cannot change
   the link kind, deadlines, evidence or anything else.
5. **One place, one kind of link.** A site `[serial]` is refused for a machine
   that is not on a serial cable, and refused beside an overrides file that
   replaces `[transport]` ("keep them in one place"). An overrides file alone
   still replaces the whole section, as before.
6. **Same on the command line.** `analyte machines add` and `machines set`
   take `--baud`, `--data-bits`, `--parity`, `--stop-bits` and
   `--flow-control` and pass them through the same draft and checks as step 2;
   `add` keeps them in the starter site file for the set that follows.
   `machines list` (and `--json`), `analyte doctor` and the machine's page
   show the values in use and which differ from the suggestion. `analyte
   overview` stays the engine's verdict (ADR 0027): cable values are settings,
   not a verdict.

## Consequences

- A lab changes a cable setting on the Setup page or with one command; the
  definition stays as shipped.
- A machine whose local overrides file replaces `[transport]` cannot be saved
  in Setup until that section is removed there; the refusal says so.
- Definitions may now leave a serial value out; the machine cannot run until
  the lab chooses it.
- A baud rate outside the list (for example 14400) is refused even in a
  definition. No shipped or example definition used one.
