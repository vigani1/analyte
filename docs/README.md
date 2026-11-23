# Documentation map

Nobody reads all of these. Find your role, read its first page, and follow
links only when a task needs them. Every folder opens with a line saying what
it is for.

| Folder | For |
| --- | --- |
| [operations/](operations/README.md) | Using Analyte: install, connect, run, commands |
| [contract/](contract/README.md) | Connecting a lab app |
| [develop/](develop/README.md) | Writing code and settings: adding a machine or a language, tests, releases |
| [protocols/](protocols/README.md), [adr/](adr/README.md), [design/](design/README.md), [conformance/](conformance/gazelle/law-run/README.md) | Looking things up |
| [research/](research/README.md) | Background only |

## Start by role

| You are | Read first | Then, when you need it |
| --- | --- | --- |
| **Lab IT or lab manager** installing and running Analyte | [Operations guide](operations/README.md) | [Commands](operations/commands.md), [Recovery](operations/recovery.md), [Pilot](operations/pilot.md) |
| **Lab app developer** connecting an app | [Contract v1](contract/v1.md) | [Orders cursor](contract/orders-cursor.md), [contract checker](contract/README.md#check-your-app), [reference receiver](../crates/testing/receiver/README.md) |
| **Adding a machine** on a protocol Analyte has | [Adding a machine](develop/adding-a-machine.md) | [Machine folders](../machines/README.md), [config reference](../crates/core/config/README.md), [protocol scope](protocols/README.md) |
| **Adding a machine's own language** | [Adding a vendor protocol](develop/adding-a-vendor-protocol.md) | [Layout and naming (ADR 0012)](adr/0012-uniform-layout-and-naming.md), [machine research](research/machines/README.md) |
| **Working on Analyte's code** | [AGENTS.md](../AGENTS.md) (the rules), then [project layout](develop/project-layout.md), [context](context.md) and [plan](plan.md) | [The crates](../crates/README.md), [program guide](../crates/analyte/DETAILS.md), [orders](../crates/analyte/ORDERS.md), [glossary](glossary.md), [developer guides](develop/README.md) |
| **Releasing** | [Release guide](develop/release.md) | [Windows packaging](../packaging/windows/README.md), [Linux packaging](../packaging/linux/README.md) |

New contributors also read [CONTRIBUTING.md](../CONTRIBUTING.md), which lists
the [ways to help](../CONTRIBUTING.md#ways-to-help).

## Reference: look things up, do not read through

| Folder | What it holds | When to open it |
| --- | --- | --- |
| [adr/](adr/README.md) | One file per decision and why it was made | When a link points to one, or before changing what it decided |
| [protocols/](protocols/README.md) | Our notes and test checklists per protocol | When working on that protocol |
| [design/](design/README.md) | The admin pages' look and rules | When changing a page |
| [conformance/](conformance/gazelle/law-run/README.md) | Runs against official test tools | When checking a protocol's evidence |
| [research/machines/](research/machines/README.md) | What was learned about each analyzer | Before working on that machine |
| [research/other/](research/other/README.md) | Other software, lab middleware and practices | Rarely: history, not current rules |

## Which one wins

If two documents disagree, the order is: the owner's request, the
[plan](plan.md), ADRs that are not superseded, then the code. Research and
older notes never override them.
