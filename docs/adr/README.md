# Architecture decision records

This folder is for **decisions and why they were made**, one file each. Open
one when a link points to it or before changing what it decided.

One file per hard-to-reverse decision, named `NNNN-short-title.md`, about half
a page: context, decision, why, consequences and status. Protocol detail lives
in [docs/protocols/](../protocols/README.md). A superseded record stays and
links to its replacement; none is superseded today. A record whose part a
later one changes says "Changed by" in its status line and beside the part.

| ADR                                             | Decision                                           |
| ----------------------------------------------- | -------------------------------------------------- |
| [0001](0001-rust-and-layered-architecture.md)   | Rust and a layered architecture                    |
| [0002](0002-config-first.md)                    | Config first, code only where config cannot do it  |
| [0003](0003-clinical-safety-rules.md)           | Clinical safety rules                              |
| [0004](0004-outbound-only-polled-orders.md)     | Outbound connections only, orders polled           |
| [0005](0005-evidence-levels.md)                 | Evidence levels for drivers and machine configs    |
| [0006](0006-apache-2-license.md)                | Apache License 2.0                                 |
| [0007](0007-standards-first.md)                 | Standards first, machines after                    |
| [0008](0008-app-contract.md)                    | Contract with lab apps (version 1)                 |
| [0009](0009-recording-format.md)                | Recording format for fixtures and replay           |
| [0010](0010-acknowledgment-boundaries.md)       | Acknowledgment boundaries for every protocol       |
| [0011](0011-result-status-progression.md)       | Result status progression (refines ADR 0003)       |
| [0012](0012-uniform-layout-and-naming.md)       | One layout and naming scheme for protocols and machines |
| [0013](0013-retention-of-delivered-data.md)     | Retention of delivered data (refines ADR 0003)     |
| [0014](0014-external-alert-delivery.md)         | External alert delivery                            |
| [0015](0015-named-machine-departures.md)        | Typed, evidenced departures for individual machines |
| [0016](0016-awos-status-notifications.md)       | LAW status notifications beside order state        |
| [0017](0017-windows-access-list-unsafe.md)      | Unsafe code for Windows access lists               |
| [0018](0018-server-rendered-pages-and-one-stylesheet.md) | Server-rendered admin pages with one token stylesheet |
| [0019](0019-desktop-app.md)                     | The Analyte desktop app: a window and a tray beside the engine |
| [0020](0020-admin-reads-stop-at-their-deadline.md) | Admin reads stop at their deadline; writes never do |
| [0021](0021-windows-sign-in-pipe.md)            | Windows sign-in for the Analyte app through a local pipe |
| [0022](0022-title-bar-snap-layouts-and-window-menu.md) | Snap layouts and the window menu for the app's title bar |
| [0023](0023-status-reasons-and-notifications.md) | Notifications that say what happened |
| [0024](0024-one-review-item-per-cause.md)       | One Needs review item per cause (refines ADR 0003) |
| [0025](0025-setup-from-the-command-line.md) | Setting up machines and lab apps from the command line |
| [0026](0026-accept-only-the-machine.md)         | A listening machine accepts only its own device    |
| [0027](0027-command-line-asks-the-engine.md)    | The command line asks the running engine for the verdict |
| [0028](0028-lab-app-keys-through-the-engine.md) | Lab app keys from the command line through the running engine (refines ADR 0025) |
| [0029](0029-lab-app-owns-test-names.md)         | The lab app owns test names (changes ADRs 0002, 0003, 0008, 0024) |
| [0030](0030-lab-app-owns-specimen-codes.md)     | The lab app owns specimen codes (changes ADRs 0002, 0008, 0029) |
| [0031](0031-lab-chooses-serial-cable-settings.md) | The lab chooses its serial cable settings in Setup (refines ADR 0002) |
| [0032](0032-machine-model-and-published-schema.md) | Deliveries name the machine model; the contract is published as a schema (changes ADR 0008) |
