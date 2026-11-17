# mapping

**Kind: step.** One file per protocol where it has a role here (`astm.rs`, `hl7.rs` and so on).

**What it does:** Translates machine fields and codes into the common model using explicit settings, and maps outgoing work.

**Where it sits:** Receives structured protocol messages and configuration; hands model evidence and findings to the host.

**What it must never do:** Access connections, make clinical decisions or guess missing information; the host retains receipts and parks findings.

**Main parts:**

- `astm`, `hl7`, `law`, `poct`, `vidas`, `autolyser`: Translations for each protocol.
- `read`: One receipt reader per protocol, calling the framing and syntax owners
  before the existing mapper. Live intake and reprocessing share these readers;
  the program supplies durable POCT Hello context and commits the output.
- `inbound`: Incoming translation steps that preserve evidence.
- `orders`: Outgoing mappings, separate from incoming code tables.

[Technical details and safety guidance](DETAILS.md).

Receipt reading is composed by the program (`analyte::reading`), which depends
on framing, config, protocol syntax and mapping. Mapping depends on config,
model and the five protocol grammars for their parsed input and outgoing
structures; contract supplies app order types. It does not depend on framing.

Every inbound mapper has one entry path and argument order:
`analyte_mapping::<protocol>::map(&settings, &parsed, input)`.
ASTM and HL7 use their module's `Settings` to pair validated syntax with
incoming mappings (and HL7 departure declarations); LAW, POCT, VIDAS and
Autolyser take their validated protocol settings from config. Parsed input
always comes from the owning protocol grammar. `Input` carries the retained
receipt, receive time, revision and optional durable POCT Hello device identity.
Mapping performs no lookup or I/O to fill missing context. Root-level
`map_<protocol>` aliases are deliberately absent.
