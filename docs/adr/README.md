# Architecture decision records

One file per hard-to-reverse decision, named `NNNN-short-title.md`, with
context, decision and consequences. A superseded record stays and links to its
replacement.

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
| [0014](0014-external-alert-delivery.md)         | External alert delivery                |
