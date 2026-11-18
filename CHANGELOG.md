# Changelog

## Unreleased

- Local bearer-token admin page, named-responder alerts, and audited stored-byte
  reprocessing and exact-payload resend.
- LAW program receiving, query-triggered work, send-ahead and cancellation on one
  persistent link; independent links and simulator qualification remain open.
- HL7 ORM/OML, VIDAS and Autolyser order sending with durable reservations,
  explicit uncertain outcomes and restart without replay.
- Recorded Gazelle message validation: all eight Analyzer Manager messages pass;
  validation found and led to a fix for the SAC-3/SAC-10 conflict.

- Rust workspace with separate transport, framing, dialog, syntax, mapping and
  shared result layers. Protocol work covers ASTM, HL7 v2 over MLLP, IHE LAW,
  POCT01-A2, VIDAS Literal and Autolyser within the documented protocol scopes.
  This is document-based implementation and synthetic testing, not qualification
  against real analyzers or a completed lab pilot.
- Receive program with validated configuration, durable raw evidence before
  positive acknowledgment, preserved result values and revisions, parked items
  with reasons, and protected local operator commands.
- Durable HTTPS delivery to lab apps with stable retry IDs, signed requests,
  order-cache freshness checks and explicit uncertain machine-order states.
  Includes a reference receiver and synthetic conversation tests.
- Windows service and Linux systemd packaging with dedicated service accounts,
  protected data directories and bounded shutdown with crash recovery.
- Verified backups and staged restore that retains the previous database and
  pauses machine orders until the operator reconciles and resumes them.
- Tag-only release workflow with Windows and Linux archives, checksums, locked
  dependency inventory, GitHub provenance and an owner-published draft release.
  Code signing remains future work.
- Installers built by the release workflow: an unsigned click-through Windows
  installer (Inno Setup) and Linux `.deb` and `.rpm` packages (nfpm). They
  install the service, protect the data folder, let the service write the
  config folder for the setup page, create the admin token, and write a starter
  program whose one machine waits for the setup page. Uninstalling keeps data
  and configuration.
