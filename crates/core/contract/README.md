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
