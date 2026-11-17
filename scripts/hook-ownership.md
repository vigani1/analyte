# Synthetic hook ownership

`test-hooks` enables synthetic controls without changing protocol behavior.
Each production caller uses its owner's facade; feature selection and direct
synthetic state access stay inside the hook modules. Multiple named pause
points remain in a session because moving them would change custody, response
or cancellation ordering. They all use the same machine hook facade.

| Owner | Hook module | Responsibilities |
| --- | --- | --- |
| Installation | `crates/analyte/src/installation/config/hooks/` | Feature-only Program setup APIs, loopback restrictions and program override state |
| Machine and its order operations | `crates/analyte/src/machine/hooks/` | Named session pauses, LAW lookup/reservation/cancellation gates, store-page gate and queue expiry override |
| Runtime | `crates/analyte/src/runtime/hooks/` | Destination assembly, explicit loopback service, stuck-shutdown override and store-probe publication |
| Sink | `crates/steps/sink/src/hooks.rs` | Feature-only literal-loopback destination and webhook constructors |

`runtime/test_hooks/` owns the existing bounded gate primitives. Their control
API is feature-gated; the error type remains available to the inert facades.
Without the feature, machine and program hook state is zero-sized and the
facades neither wait nor permit URL overrides. HTTPS construction and normal
shutdown deadlines remain the production implementations.

CI runs `python3 scripts/check-hook-layout.py`. After separating inline test
items with the duplication splitter, it rejects multiple feature-selection
entries in any production caller and direct access to synthetic hook fields
outside these exact owners. Feature-disabled and all-features Clippy and test
passes verify both implementations. The six test-hook integration suites
exercise the same named pause locations, including stop/drop, restart custody,
queue expiry and loopback rejection.
