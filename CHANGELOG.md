# Changelog

## Unreleased

- **ASTM result uploads:** documented Roche layouts retain qualitative codes,
  cutoff indices, missing results, alarms and dilution evidence. Generated
  and ambiguous QC labels are held for review. Biolyte mapping keeps
  measurement method, specimen descriptor and preliminary report status,
  and holds results from sender-aborted uploads.

- **Recording at shutdown:** slow disks have more time to finish recordings
  within the configured service stop allowance. If a write stays blocked,
  Analyte logs the affected recording, tries to mark it as failed and exits
  with code 3 at the overall deadline.

- **Measurement review** (ADR 0035): compare held and earlier measurements,
  record a resend or queue a confirmed rerun, and review suggestions when a
  later receipt includes a missing tube ID. The browser and CLI record each
  decision with the operator and their reason.

- **Result identity recipes** (ADR 0035): documented ASTM, HL7 and VIDAS
  mappings can use source run evidence to distinguish new measurements from
  resends. ASTM and HL7 mappings can declare separate rerun and correction
  codes. Missing evidence and conflicting results are held for review.
- Model definitions can ask for documented encoding and Autolyser settings,
  require the lab's polling interval, and activate evidenced departures by
  mode. Normal ASTM definitions can opt into the Biolyte empty processing ID
  and one lost establishment acknowledgment retry.
- Analyte warns when a machine reports software outside the versions its
  settings were checked with. The warning keeps the first report time and can
  be cleared by an audited confirmation. Results continue to flow (ADR 0036).

## 0.1.0 - 2026-11-23

First draft release. **Status:** every protocol is built and tested from its
official documents and synthetic recordings (evidence level documented). No
machine has been qualified against a real analyzer or in a lab pilot yet. The
Windows installer and programs are not code-signed, so Windows SmartScreen
warns before running the installer.

### Program

- **Record only** (ADR 0034): any model, including the first lab's six that
  are not ready yet, can be connected only to record its conversation for a
  first visit. Analyte keeps the analyzer talking with its protocol's shipped
  capture profile (ASTM LIS01-A2, HL7 over MLLP, VIDAS Literal, the
  Autolyser's next-report poll), stores every received chunk before any reply,
  records every connection, and reads, delivers and orders nothing. A query is
  answered with the protocol's own error (ASTM code Q, HL7 AE), never "no
  tests"; the Autolyser is asked only for its next report. Setup step 1 offers
  "Record only", `analyte machines set` takes `--record-only` (and
  `--connect-normally` to leave it); the Overview, Machines, the machine's
  page, `machines list` and `doctor` say "Recording only — nothing is sent to
  a lab app" with the counts since start. The six first-lab models gained a
  `record.toml`. Recorded messages are never delivered later and
  reprocessing refuses them. The cobas e 411 asks its protocol type (Elecsys
  or cobas; Setup step 2 or `--analyzer-mode`): in the cobas type a frame
  holding several records is acknowledged under the ADR 0015 departure its
  `record.toml` declares, instead of being refused every time; a bad
  checksum is still refused.
- `analyte captures list` and `analyte captures export` list and copy the
  recorded conversations, audited first, with a reminder that they contain
  patient data and must be de-identified before sharing.
- A machine definition can declare the choices a lab makes (ADR 0032): modes
  the analyzer offers on its own screen, and its software version, whose band
  decides version-specific facts such as the VIDAS sample ID limit. Each value
  selects existing settings only (the mappings, whether orders apply, VIDAS
  Literal or Alternate, the VIDAS sample ID limit); every combination is
  checked when the definition loads. Setup step 2 asks exactly the selected
  model's choices under **Analyzer settings**, starting at the manual's
  suggestion, and Save keeps every value in the site file's `[choices]`. A
  choice without a value and without a suggestion is refused by name, never
  assumed. `analyte machines set` and `test` take `--choice NAME=VALUE`;
  `machines models` lists each model's choices; `machines list` (and
  `--json`), `doctor` and the machine's page show the values in use and which
  differ from the manual. A choice cannot change evidence or departures.
- The synthetic example `vidas_choices.toml` shows a mode choice and a
  software version band.
- A lab chooses a serial machine's cable settings (baud rate, data bits,
  parity, stop bits, flow control) on the Setup page's step 2, from the
  standard values, filled in with the manual's suggestion and marked where they
  differ; the connection test uses the chosen values and Save keeps them in the
  machine's site file (`[serial]`). `analyte machines add`, `set` and `test`
  take `--baud`, `--data-bits`, `--parity`, `--stop-bits` and
  `--flow-control` with the same checks; `machines list`, `doctor` and the
  machine's page show the values in use (ADR 0031).
- A machine definition may leave out a serial value its manual does not state;
  Analyte refuses to run it until the lab chooses one, and never assumes it.
  Baud rates are limited to the standard list, 300 to 115200.
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
- The lab app owns test names and test lists (ADR 0029). Results carry the
  machine's own test code exactly as sent, in the machine's code system; a
  code no table lists is delivered, not parked. Orders arrive in the
  machine's own codes, with an optional name from the lab app, and a test in
  another code system is rejected before sending. Machine definitions name
  their `test_system` instead of a test table.
- Order specimen types are the machine's own codes too (ADR 0030): machine
  definitions name one `[orders] specimen_system` instead of a specimen
  table, a specimen type in another code system or one the protocol cannot
  carry is rejected before sending, and nothing is defaulted. Priority, unit,
  QC and identity tables stay: they hold machine and protocol facts.
- Every result delivery and order event names the machine model in a required
  `source.model`, the definition's `vendor/model` folder key such as
  `roche/cobas-e411`, so a lab app keys its per-model test and specimen
  tables on it (ADR 0033). A machine definition outside a `<vendor>/<model>/`
  folder is refused at start, since no model could be named for it. Each
  machine order keeps the model it was sent with (store schema 19), so an
  interrupted order of a machine removed since then still becomes uncertain
  at restart and its event names that model; an order kept from before
  schema 19 has its events parked on Needs review instead of guessing one.
- Contract v1 is published as machine-readable files, generated from the Rust
  contract types: a JSON Schema of every request and answer body
  (`docs/contract/contract-v1.schema.json`) and an OpenAPI 3.1 document of
  the three calls (`docs/contract/openapi.json`). A machine catalog
  (`docs/contract/machines.json`, with its schema) lists each shipped model's
  id, name, vendor, protocol, evidence level, whether it is ready or blocked,
  and its code systems. `cargo run -p analyte-publish` writes them; CI checks
  they are current (ADR 0033).
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
  release. Releases also carry the contract schema, OpenAPI document and
  machine catalog, covered by the checksums and provenance (ADR 0033).
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
