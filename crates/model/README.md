# model

**What it does:** Defines shared results, orders, values and identities across protocols.

**Where it sits:** Receives validated evidence from mapping and supplies shared representations to storage and app integration.

**What it must never do:** Access connections, make clinical decisions or invent identities; keep original values and distinguish corrections from reruns.

**Main parts:**

- [identity.rs](src/identity.rs): Validated identities.
- [result.rs](src/result.rs): Results and their revisions.
- [lifecycle.rs](src/lifecycle.rs): Distinct order states.
- [text.rs](src/text.rs): Text with size limits.
- [value.rs](src/value.rs): Exact values and explicit unit mappings.

[Technical details and safety guidance](DETAILS.md).
