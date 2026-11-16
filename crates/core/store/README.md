# store

**Kind: core.** Parts are grouped by role; small parts currently stay in single files and grow into role folders when needed.

**What it does:** Keeps original evidence, result history, pending deliveries and order state durably.

**Where it sits:** Saves data before Analyte tells a machine it has received it, and keeps it until the lab app can receive it.

**What it must never do:** Discard evidence, overwrite originals, log payloads or send a reserved machine order again.

**Main parts:**

- `intake`: Saves original machine data first, so Analyte only says got it once the data is safe.
- `results`: Keeps result history, including corrections, without overwriting earlier results.
- `delivery`: Keeps deliveries for the lab app and retries them with the same identity.
- `orders`: Tracks orders and what happened to each one, so a retry cannot send it to a machine twice.
- `upkeep`: Opens, checks, backs up and restores the store, and refuses new data when space runs low.
- `operator`: Shows staff what needs attention and records their decisions.

[Technical details and safety guidance](DETAILS.md).
