# 0026. A listening machine accepts only its own device

Status: accepted, 2026-11-21 (the owner pre-approved the recommended option).
Extends the closed list of [ADR 0023](0023-status-reasons-and-notifications.md)
by two kinds. Numbered 0026 because open pull requests #309 and #310 both use
0025.

## Context

When a machine connects to Analyte (`tcp_server`), Analyte accepted any
device that reached the port. Since #308 the Windows installer opens the
firewall to the local subnet for `analyte.exe`, so any laptop on the lab
network could connect to a machine's port and send ASTM, HL7 or POCT data
that Analyte stored and delivered as that machine's results: fake or
misattributed results in patient reports. Analyzer protocols carry no
authentication, so the practical controls are the peer's address and
keeping analyzers on their own network.

## Decision

1. **`allowed_peers` in the site file.** A listening machine's site file may
   list the devices it accepts: `allowed_peers = ["192.168.1.50"]`, 1 to 16
   plain IPv4 or IPv6 addresses, each once. The list is parsed once at the
   configuration boundary into `analyte_config::AllowedPeers`. A hostname, a
   port, a range, `0.0.0.0`, `::`, a multicast or broadcast address, an empty
   or overlong list are refused. The key is refused for serial, folder and
   outgoing (`tcp_client`) machines. An IPv4 address and its IPv6-mapped form
   (`::ffff:192.168.1.50`, as a dual-stack listener reports it) are the same
   device. A connection from any other address is closed as soon as it is
   accepted: nothing is read from it or written to it (no ACK, no NAK),
   nothing of it is stored as the machine's data, and no dialog starts. Both
   listening paths (the session loop and independent LAW) go through the one
   accept function that applies the rule. The key stays out of the canonical
   snapshot when it is absent, so existing settings keep their digest.
2. **Without the key, keep accepting, and say so.** An existing site file
   without `allowed_peers` keeps working as before. Each such machine shows a
   warning on its page and on the Machines list ("Accepts connections from
   any device") and a line on the Overview, which `GET /status` names
   `machine_open_to_any_device` (it needs a look; it does not stop data). The
   setup steps fill the key in: step 3's connection test keeps the address of
   the device that connected, step 4 offers "Only accept this device:
   192.168.1.50", ticked, and step 2 lets the lab manager type or change the
   list. Unticking saves without the key, and the warning stays.
3. **Refusals are visible.** Each machine keeps, in memory, the last five
   refused devices with the newest time and a count. Its page lists them
   ("Refused 3 connections from 192.168.1.77, the last at 14:32"), the
   Machines list names the newest, and the Overview has a line for 24 hours
   after the newest refusal, which `GET /status` names
   `machine_refused_connection`. A refusal is logged with the machine and the
   address, at most once a minute per device. The record starts empty when
   Analyte restarts.
4. **The address is shown, not hidden.** Safety rule 10 keeps patient data
   out of ordinary logs. A refused device's IP address is not patient data:
   it names a device on the lab network, which the operator must find. The
   link status keeps no other address (an accepted machine's address is not
   kept), the pages that show it need sign-in, and `GET /status` names only
   the kind, never the machine or the address.

## Why

- A list of addresses is the only control the protocols allow, and it costs
  one key per machine. Without it, any device on the network can put results
  into patient reports under a machine's name.
- **Default kept open with a warning, not closed.** Refusing every machine
  without the key after an upgrade would stop results from labs that work
  today until someone edits each site file. Results stopping is itself a
  clinical risk, and nobody would be told which device to allow. The warning
  names each open machine, and the setup step makes the fix one tick. Making
  the key required can follow once installed sites have it.
- **No ranges.** An analyzer has one fixed address. A range lets other
  devices on the subnet in, which is the attack. Adding ranges later would
  be a small change in one type if a real site needs them.
- **The test's device is offered, not trusted.** Another device could connect
  during the test, so step 4 says to check the address against the
  machine's own network settings, and nothing is saved until Save.
- **Refusals stay in memory.** They are a signal to look, not evidence. The
  log keeps the record, and the store holds only data from machines.

## Consequences

- An analyzer whose address changes (DHCP) is refused until its new address
  is added; the Overview line and the machine page say what to check. Labs
  should give each analyzer a fixed address.
- Another device can still pretend to be the machine by taking its IP
  address. Network separation (analyzers on their own cable or VLAN) is the
  second control, described in the operations guide.
- When Analyte connects out to a machine (`tcp_client`), the analyzer's own
  "allowed host" setting and network separation are the controls; a lost
  connection already shows as "Not connected (retrying)" with a
  notification.
- The command line does the same through the same setup code:
  `analyte machines set <name> --allowed-peer <ip>` (repeatable) and
  `--any-device` to clear the list; at a terminal, `machines set` asks for
  the devices of a listening machine that has none, and `machines test`
  names the device that connected and offers to accept only it (a script is
  told the command instead). `machines list` shows the list or the warning
  and the newest refusal, and `analyte doctor` warns for a listening machine
  without `allowed_peers`.
