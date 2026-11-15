# Store mutation triage

Baseline: `bdb6b6f7af375d5679db462af0fe070f3297bc1f` (origin/main).

This round is in progress. Production code must remain unchanged.
Tool: cargo-mutants 27.1.0, all features, scratch copies, 60-second tests,
180-second builds, four jobs. Baseline output: `target/store-mutation-before/`.

```powershell
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 4 -o target/store-mutation-before
```

| Baseline missed mutant (unchanged production location) | Decision and evidence |
| --- | --- |

No counts or equivalence conclusions are claimed before results are inspected.
