# contract

**What it does:** Defines the versioned documents and signatures exchanged with lab apps.

**Where it sits:** Receives model evidence for app documents and checks incoming documents; sink and receiver use this shared agreement.

**What it must never do:** Interpret clinical results or substitute internal model data directly for the published document format.

**Main parts:**

- [lib.rs](src/lib.rs): Versioned documents and input limits.
- [evidence.rs](src/evidence.rs): Independent app-facing evidence fields.
- [response.rs](src/response.rs): Lab-app response shapes.
- [signing.rs](src/signing.rs): Signature rules without network access.
