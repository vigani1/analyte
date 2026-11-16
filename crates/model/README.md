# model

**Kind: core.** Parts are grouped by role; small parts currently stay in single files and grow into role folders when needed.

**What it does:** Defines shared results, orders, values and identities across protocols.

**Where it sits:** Receives validated evidence from mapping and supplies shared representations to storage and app integration.

**What it must never do:** Access connections, make clinical decisions or invent identities; keep original values and distinguish corrections from reruns.

**Main parts:**

- `identity`: Validated identities.
- `result`: Results and their revisions.
- `lifecycle`: Distinct order states.
- `text`: Text with size limits.
- `value`: Exact values and explicit unit mappings.

[Technical details and safety guidance](DETAILS.md).
