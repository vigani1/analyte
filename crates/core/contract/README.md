# contract

**Kind: core.** Groups files by the data and operations they handle.

**What it does:** Defines the versioned documents and signatures exchanged with lab apps.

**Where it sits:** Receives model evidence for app documents and checks incoming documents; sink and receiver use this shared agreement.

**What it must never do:** Interpret clinical results or substitute internal model data directly for the published document format.

**Main parts:**

- `lib`: Versioned documents and input limits.
- `document`: Independent version-1 app-facing document types.
- `response`: Lab-app response shapes.
- `signing`: Signature rules without network access.

## Frozen validation and duplication

The six version-1 modules `document/{value,text,identity,list,time,sample}.rs`
retain independent wire types and acceptance rules. The per-module reasons live
once in [ADR 0008](../../../docs/adr/0008-app-contract.md#frozen-wire-types).
No shared model validation helper is used: changes to model acceptance must not
silently change version 1. Jiff already supplies civil offset validation.
`tests/agreement.rs` detects drift and requires an explicit contract decision.
Only these six paths are exempted from production clone detection in
`scripts/duplication-exclusions.txt`; conversion code and other contract modules
remain measured.
