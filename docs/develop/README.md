# Developer guides

This folder is for **writing code and settings for Analyte**: how to add a
machine, how to add a machine's own language, how to test and how to release.
Not here: installing and running Analyte (see [operations](../operations/README.md)),
connecting a lab app (see [contract](../contract/README.md)) and background
research (see [research](../research/README.md)).

| Guide | Use it to |
| --- | --- |
| [Project layout](project-layout.md) | See the whole repository on one page: code, machine settings, docs, installers, and how settings meet the code |
| [Adding a machine](adding-a-machine.md) | Add an analyzer that speaks a protocol Analyte has: a settings folder in [`machines/`](../../machines/README.md), no code |
| [Adding a vendor protocol](adding-a-vendor-protocol.md) | Add a machine's own language: where each piece of code goes |
| [Test quality](testing/quality.md) | See and repeat the coverage and mutation measurements |
| [Test timing](testing/timing.md) | Write tests that wait on timers or the store |
| [Release](release.md) | Tag, build, check and publish a release |

The rules that apply to every change are in [AGENTS.md](../../AGENTS.md) and
[CONTRIBUTING.md](../../CONTRIBUTING.md).
