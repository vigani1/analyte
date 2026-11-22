# Changelog

## Unreleased

## 0.1.0 - 2026-11-22

First draft release. **Status:** every protocol is built and tested from its
official documents and synthetic recordings (evidence level documented). No
machine has been qualified against a real analyzer or in a lab pilot yet. The
Windows installer and programs are not code-signed, so Windows SmartScreen
warns before running the installer.

### Program

- Rust workspace with separate transport, framing, dialog, syntax, mapping and
  shared result layers. Protocol work covers ASTM, HL7 v2 over MLLP, IHE LAW,
  POCT01-A2, VIDAS Literal and Autolyser within the documented protocol scopes.
- Receive program with validated configuration, durable raw evidence before
  positive acknowledgment, preserved result values and revisions, parked items
  with reasons, and protected local operator commands.
- Durable HTTPS delivery to lab apps with stable retry IDs, signed requests,
  order-cache freshness checks and explicit uncertain machine-order states.
  Includes a reference receiver, synthetic conversation tests and a contract
  checker that runs the published contract vectors against a lab app.
- HL7 ORM/OML, VIDAS and Autolyser order sending with durable reservations,
  explicit uncertain outcomes and restart without replay.
- LAW program receiving, query-triggered work, send-ahead and cancellation on
  one persistent link; independent links and simulator qualification remain
  open.
- Recorded Gazelle message validation: all eight Analyzer Manager messages
  pass; validation found and led to a fix for the SAC-3/SAC-10 conflict.
- A serial machine can be named by its USB adapter's serial number instead of
  a port name; Analyte finds the adapter again on start and reconnect.
- Three harmless protocol differences are accepted with a visible note
  instead of being refused.
- Set each lab app once and let machines pick it; a removed machine's results
  are still delivered to its lab app.
- An alert when results cannot be delivered, with the reason and the fix.
- One Needs review item per cause, instead of two or three for one problem
  (ADR 0024).
- Activity folds a delivery's failed tries in a row into one line (store
  schema 18).
- Admin page reads stop at their deadline, so a slow page never holds up
  machine data (ADR 0020). HL7 messages are answered when storage is slow
  instead of the socket being closed.
- Verified backups and staged restore that keeps the previous database and
  pauses machine orders until the operator reconciles and resumes them.

### Admin pages

- Admin pages on this PC only, signed in with the admin key or Windows, with
  named-responder alerts, audited reprocessing of stored bytes and
  exact-payload resend.
- A new look: an Overview that says whether everything is OK, a first-run
  welcome, guided Needs review items, Machines, Lab apps, Setup and Activity
  pages, a connection light and last data time per machine, and live updates.
- Step-by-step machine setup with a plain connection-test verdict; add and
  remove machines from the setup page.
- Network analyzers: setup lists this PC's addresses and what to type into
  the analyzer, and the network test says what it saw.

### Desktop app (Windows)

- The Analyte app (`analyte-app.exe`): the admin pages in their own window, a
  tray icon with the engine's state, an Engine off screen with Start engine,
  and start at sign-in (ADR 0019). The engine's public `GET /status` says only
  the overall state and the kinds of thing that need someone.
- Sign in with Windows through a local pipe; the audit names the Windows
  account, and setup creates the "Analyte users" group (ADR 0021).
- Title bar snap layouts and the window menu (ADR 0022).
- Notifications that name the problem and open the Overview (ADR 0023).

### Command line

- Set up machines and lab apps from PowerShell or a shell:
  `analyte machines ...` and `analyte lab-apps ...`, running the setup pages'
  own rules (ADR 0025). Lab app keys go through the running engine
  (ADR 0028).
- Every admin page view as a command: `overview`, `activity`, `alerts list`,
  `data-keeping`, `orders cancelled`, `review`, plus `doctor`, `logs` and
  `completions` (ADR 0027). See [commands](docs/operations/commands.md).
- Commands find the installed program file without `--config`
  (`ANALYTE_CONFIG`, the current folder, then the installed location).

### Installers

- Tag-only release workflow with Windows and Linux archives, checksums, a
  locked dependency inventory, GitHub provenance and an owner-published draft
  release.
- An unsigned click-through Windows installer (Inno Setup) and Linux `.deb`
  and `.rpm` packages (nfpm). They install the service, protect the data
  folder, let the service write the config folder for the setup page, create
  the admin token and write a starter program whose one machine waits for the
  setup page. Uninstalling keeps data and configuration.
- The Windows installer offers "Engine and app" or "Engine only" (set up from
  PowerShell or a browser), and a task that adds Analyte to the system PATH.
- The Windows installer adds one firewall rule so analyzers on the local
  network can reach Analyte; uninstall removes it.
- Windows service and Linux systemd packaging with dedicated service
  accounts, protected data folders, and bounded shutdown with crash recovery.

### Security

- A listening machine accepts data only from its own device when
  `allowed_peers` lists its addresses; the Overview warns about a machine open
  to any device (ADR 0026).

### Docs

- README rewritten for newcomers, with a demo and an example of a lab app's
  orders and results flow; merged protocol notes and a link check.
- Contributor files: contributing guide, code of conduct and security policy.
