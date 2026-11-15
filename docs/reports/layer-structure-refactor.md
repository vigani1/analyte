# Layer structure refactor: partial implementation report

Status: incomplete. The naming and role-folder work compiles, but the requested
size/complexity cleanup and boolean-group changes remain unfinished. PR #91 must
remain draft until these are completed and the strict per-crate checks pass.
No new lint exceptions or dependencies were added. No boolean groups were changed.
Configuration spelling and wire construction were not changed.

## Moves and renames

Paths below are relative to the repository root.

| Old path | New path | Reason |
| --- | --- | --- |
| `crates/mapping/src/map.rs` | `crates/mapping/src/inbound/grouping.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/identity.rs` | `crates/mapping/src/inbound/identity.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/result.rs` | `crates/mapping/src/inbound/result.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/result_evidence.rs` | `crates/mapping/src/inbound/evidence.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/source.rs` | `crates/mapping/src/inbound/source.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/time.rs` | `crates/mapping/src/inbound/time.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/value.rs` | `crates/mapping/src/inbound/value.rs` | Group source-to-model translation steps by inbound responsibility. |
| `crates/mapping/src/outbound_codes.rs` | `crates/mapping/src/orders/codes.rs` | Put outgoing translation tables beside order mapping. |
| `crates/poct/src/build.rs` | `crates/poct/src/encode.rs` | Name outbound encoding by its operation. |
| `crates/hl7/src/order/wire.rs` | `crates/hl7/src/order/encode.rs` | Name outbound encoding by its operation. |

Mapping now has nine top-level source files. Its six protocol files remain side
by side. All other in-scope crates had ten or fewer top-level files.
The new `inbound.rs` declares the inbound role modules; it adds no forwarding API.
HL7 order encoding remains separate from `encoder.rs`: the former validates order
identities and assembles order messages, while the latter bounds common segment
output. Order encoding continues to use the shared segment encoder.
`transport::recording::recording_with_header` became `with_header`.

## Function diagnostics before and after

Counts below use the requested Clippy warnings across all targets, deduplicated
by source path, line and lint. They include tests and examples; a source included
under two distinct paths is counted twice. These are lint diagnostics, not a
physical line count. The extraction of folder waits did not cross a lint threshold.

| Crate | Long before | Long after | Complex before | Complex after |
| --- | ---: | ---: | ---: | ---: |
| transport | 0 | 0 | 0 | 0 |
| framing | 0 | 0 | 0 | 0 |
| dialog | 5 | 5 | 1 | 1 |
| astm | 2 | 2 | 1 | 1 |
| hl7 | 19 | 19 | 1 | 1 |
| poct | 2 | 2 | 1 | 1 |
| vidas | 0 | 0 | 0 | 0 |
| autolyser | 0 | 0 | 0 | 0 |
| mapping | 6 | 6 | 0 | 0 |
| model | 1 | 1 | 0 | 0 |
| config | 8 | 8 | 1 | 1 |
| contract | 0 | 0 | 0 | 0 |
| sink | 2 | 2 | 0 | 0 |
| receiver | 1 | 1 | 0 | 0 |
| sim | 0 | 0 | 0 | 0 |

## Folder waits

The run loop now names `handoff` and `await_disposition` separately. It still
finishes a claim only after a received host disposition; a disconnected or late
reply leaves it claimed. The polling intervals and deadlines are unchanged.

- The 5 ms send retry preserves bounded cancellation/deadline observation: the
  caller supplies an atomic cancellation flag, without a wakeable signal.
- The 10 ms receive timeout is a blocking channel wait between cancellation checks.
- The 100 ms directory scan interval is necessary because producer renames supply
  no notification channel; it also bounds cancellation observation.

Replacing these with indefinitely blocking waits would change shutdown behavior.
No notification subsystem or dependency was introduced.

## Changed test call paths

Only `crates/poct/tests/builders.rs` changed existing test calls:
`analyte_poct::build` became `analyte_poct::encode` in:

- `poct01_b_6_reviewer_builders_round_trip_and_correlate`
- `poct01_project_builder_limits_and_explicit_offset`
- `poct01_b_5_7_builder_honors_exact_device_receive_size`

Test names, assertions and expected bytes are preserved. The transport support
file gained only its module header.

## Implementation commits

- `94766d1b5ff682608a0c96f1f3010faded77770c` refactor: name recording constructor within its module
- `1fd5efbb555eb41841ae85f8cc5a31df0fba7f8b` docs: state layer ownership and exclusions in every crate
- `00578a048f224c2231da09de7cd921aa95146c0b` refactor: name folder handoff and disposition waits
- `d9cd06aae118ccee7b493d04ff7ab0a234854c6d` refactor: group mapping roles and name outbound encoders
- `a48b70e7b67f914bb6cf92d7ecf7378ce19ffa13` docs: explain source file ownership at layer boundaries

## Verification

`git fetch origin` and `git merge --signoff origin/main` reported already up to
date before final checks. No merge commit was needed.

The strict command ran for each of the 15 requested crates, with all seven
requested lint flags and `-D warnings`. Exit codes:

| Crate | Exit code |
| --- | ---: |
| transport | 0 |
| framing | 101 |
| dialog | 101 |
| astm | 101 |
| hl7 | 101 |
| poct | 101 |
| vidas | 0 |
| autolyser | 101 |
| mapping | 101 |
| model | 101 |
| config | 101 |
| contract | 101 |
| sink | 101 |
| receiver | 101 |
| sim | 0 |

Some crate checks fail while compiling another in-scope crate they depend on,
so a failure does not necessarily locate a diagnostic in that crate itself.
The before/after warning-only workspace runs provide the complete diagnostic
inventory, rather than stopping at the first failing dependency.

Outstanding: the requested function decomposition and boolean-group conversions
in LAW, VIDAS framing and config; remaining item-name warnings; and production
file-size justifications. No `#[expect]` was added by this refactor.

Final workspace checks:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed, exit 0; no output |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed, exit 0 |
| `cargo test --workspace` | Passed, exit 0; 1023 passed, 0 failed, 2 existing ignored |
| `cargo deny check` | Passed, exit 0; advisories, bans, licenses and sources OK |
| `cargo check --manifest-path fuzz/Cargo.toml --bins` | Passed, exit 0 |

An earlier test build raced a constructor rename and failed to resolve the new
symbol. The complete final workspace test run above was rerun after edits and
passed. The final normal Clippy and format checks were also rerun after edits.

Full local check output (including each strict crate diagnostic) is retained at
`C:/Users/super/AppData/Local/Temp/analyte-layer-structure-checks-91`.
