# Analyte

Connect any lab analyzer to any lab app.

Analyte is a small program that runs on the lab PC next to the analyzers. It
talks to each machine in its own protocol (ASTM, HL7, IHE LAW, POCT1-A or a
vendor format), stores every message safely, and passes results and orders
to a lab app over HTTPS in one common format.

**Status:** the receive, delivery, order and operator runtimes are built at the documented evidence level; machine qualification and the lab pilot remain. See [docs/plan.md](docs/plan.md).

- Machines are added with a config file, not new code, whenever they speak a
  standard protocol.
- Every driver and machine config states how far it has been proven: on a real
  machine, with an official simulator, or from documents only.
- Written in Rust. One program for Windows and Linux.

See [Adding a machine](docs/adding-a-machine.md) for configuration and qualification.

[Testing](docs/testing/quality.md): coverage, mutation testing and repeatable commands.

[Repository map](crates/README.md): what each part does and where to look.

Licensed under the [Apache License 2.0](LICENSE).
