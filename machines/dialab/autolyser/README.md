## Machine

Dialab Autolyser, reported REF 20010D (2020). Operator's Manual Rev18 covers
software 21.18/21.18M; applicability to the installed build needs confirmation.

No machine.toml is shipped: the current loader requires concrete settings, and
there is no placeholder value for the missing evidence. This is a blocked
machine definition, not an activatable configuration. Resolve the blockers,
then add TOML through the real loader before connecting the instrument.

## Connection

Vendor internal format over serial with host polling. Data bits are not
established by the inspected manual. Test-utility 9600/parity/stop/handshake
settings are not evidence of installed application settings. Required site
settings: endpoint and all serial parameters. TCP role/port/protocol equivalence
are also unknown, so TCP is not a justified substitute for missing serial data.

## Settings on the machine

Confirm Utility / Setup Analyzer / Serial settings and internal versus
variable protocol. Confirm installed software and controls layout. Keep app
destination, credentials, retention and overrides in site files.

Record whether staff manually release reports through Utility / RS232 or the
Serial setup option releases all results automatically. Release is required
before host polling can retrieve reports; automatic release does not establish
unsolicited transmission. Enable the Serial option for transmitting
non-calculable (NC) values: disabling it removes NC results from the outgoing
queue instead of delivering them (Rev18 serial setup p.119; §23.2 p.177).

During recovery, stop polling and preserve queued reports and protected evidence.
Never delete queued reports to clear a fault. Reconcile with the operator and
vendor before resolving an uncertain poll guard or resuming polling.

## Supported workflows

The host polls results (R), recovers the last response (L) and can insert a
work list. No workflows are enabled. A is not an automatic retry. Missing
immutable run identity, timestamps and units must not be invented. Work lists
require explicit patient namespace, clone, specimen medium and cup position.

## Departures

No guessed 8N1, variable-protocol script or TCP alternative added. Source
checksum/layout inconsistencies are documented in protocol notes; no copied
manual example is used as a fixture. Poll recovery must preserve uncertain
cursor state and must not silently advance or resend work.

## Evidence

Level: documented

Research only; no machine recording or official simulator run. No recordings shipped.

## Open questions

- Blocking: full serial parameters, installed protocol and software applicability.
- Required site settings: controls layout, exact four-byte analysis codes and
  app mappings, units evidence, clone/specimen/position rules and polling bounds.
- Blocking: record manual or automatic report release policy and verify that
  NC transmission is enabled. Before activation, capture synthetic NC results
  and manual-release and automatic-release polling conversations, demonstrating
  that unreleased reports are unavailable and released reports are retrievable.
  Keep all queued reports during qualification and recovery; never delete them.
- Confirm request/EOT timing, checksum layout and restart/cursor recovery with
  captures. A barcode retrofit and TCP supplement remain vendor questions.

## Sources

[Research](../../../docs/research/autolyser.md), §§2–5: Autolyser / Autolyser
ISE Operator's Manual Rev18 p.119 and chapter 23 pp.175–188; Service Manual
Rev01. [Autolyser protocol notes](../../../docs/protocols/autolyser/README.md)
and [versions](../../../docs/protocols/autolyser/versions.md).

