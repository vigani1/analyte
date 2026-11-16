# contract

**Kind: core.** Parts are grouped by role; small parts currently stay in single files and grow into role folders when needed.

**What it does:** Defines the versioned documents and signatures exchanged with lab apps.

**Where it sits:** Receives model evidence for app documents and checks incoming documents; sink and receiver use this shared agreement.

**What it must never do:** Interpret clinical results or substitute internal model data directly for the published document format.

**Main parts:**

- `lib`: Versioned documents and input limits.
- `evidence`: Independent app-facing evidence fields.
- `response`: Lab-app response shapes.
- `signing`: Signature rules without network access.
