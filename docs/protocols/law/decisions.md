# LAW decisions

These entries explain selected roles, options and host policy. Mandatory field
rules and outstanding actor qualification stay in [checklist.md](checklist.md).
Evidence remains documented; implementation restrictions below do not amend LAW.

## 1. Implement the Analyzer Manager role

- **what**: Receive analyzer results and distribute app-authorized work as
  Analyzer Manager; do not implement an Analyzer in the production path.
- **why**: The bridge sits between instruments and the app that owns orders.
  This role provides the required work/result exchanges without moving assay
  execution or clinical decisions into the bridge.
- **source**: PaLM TF-1 Rev11 §5.1.1.1–5.1.1.2, p.34;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), product boundary.
- **test**: `law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options`.

## 2. Configure peer options explicitly

- **what**: Intersect supplied enhanced work content with configured peer
  capabilities; do not discover capabilities by trial messages.
- **why**: Successfully parsing an optional field does not prove the peer
  supports the associated workflow. Explicit selection prevents accidentally
  granting repeat/reflex permission or sending unusable enhanced content.
- **source**: PaLM TF-1 Rev11 §5.1.1.2, p.34;
  [ADR 0002](../../adr/0002-config-first.md), configuration boundary.
- **test**: `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`.

## 3. Refuse queries whose identities the app cannot supply

- **what**: Support the configured container/sample and all-pending forms;
  return a query error for rack, tray and isolate/parent forms outside that scope.
- **why**: Those forms need identities beyond the current app contract.
  Converting a location or parent reference into a sample ID would risk
  selecting another specimen's work. This remains an actor-coverage gap.
- **source**: PaLM TF-1 Rev11 §§5.2.1–5.2.4, pp.36–37;
  PaLM TF-2b Rev11 §3.27.4.1.3, pp.43–44.
- **test**: `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`.

## 4. Leave related-observation and pooling workflows unclaimed

- **what**: Keep full related-observation sending and pooling workflows outside
  qualified support, despite parsing some associated fields.
- **why**: These options require coordinated work content and returned results,
  not just valid field shapes. Claiming them from parser tests could authorize
  a specimen combination or dependency the host cannot track end to end.
- **source**: PaLM TF-1 Rev11 §§5.2.18–5.2.20, pp.40–41;
  [ADR 0007](../../adr/0007-standards-first.md), explicit scope.
- **test**: none yet.

## 5. Do not fetch external result pointers in the parser

- **what**: Retain pointer and persistence metadata without fetching its URL;
  leave complete external-object custody and recovery unimplemented.
- **why**: A pointer is not custody of its target. Temporary objects can expire,
  so this limitation must remain visible rather than being presented as a
  complete implementation of external results.
- **source**: PaLM TF-2b Rev11 §B.3.12, p.77;
  [ADR 0001](../../adr/0001-rust-and-layered-architecture.md), I/O ownership.
- **test**: `law_tf2b_c7_10_tf2x_a4_rp_application_hd_profile`.

## 6. Use a short-lived outbound link in independent mode

- **what**: Own and close one outbound LAB-28 connection per transaction,
  independently of inbound LAB-27/LAB-29 session lifetimes.
- **why**: LAW permits short-lived connections. This choice gives each
  reserved work exchange one clear owner and prevents an inbound disconnect
  from orphaning already-admitted outbound work.
- **source**: PaLM TF-2b Rev11 §B.6, pp.79–80;
  PaLM TF-2x Rev11 §2.2.4, pp.10–11.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`.

## 7. Keep the shared persistent link an explicit restricted mode

- **what**: Allow the implemented single-link persistent configuration only
  as a selected topology; do not equate it with B.6's two directional links.
- **why**: The program retains this configured exchange mode, but the profile
  requires two connections for bidirectional trigger events. Naming the
  restriction avoids claiming full connection conformance from local tests.
- **source**: PaLM TF-2b Rev11 §B.6, pp.79–80;
  [ADR 0002](../../adr/0002-config-first.md), explicit machine settings.
- **test**: `law_tf2b_b6_declared_topologies_require_explicit_direction_and_outbound_endpoint`.

## 8. Gate result AA on durable interpretation

- **what**: Retain LAB-29 raw bytes and their interpretation before a positive
  result acknowledgment; keep uninterpretable input for an operator.
- **why**: The bridge is accepting application responsibility, not merely
  forwarding bytes. A crash after AA must leave enough durable evidence to
  deliver or diagnose the result without asking the analyzer to resend it.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), LAW row;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), custody policy.
- **test**: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`.

## 9. Restrict profile recovery to avoid repeating machine work

- **what**: A missing ORL or disconnected outbound order becomes Uncertain
  and is not resent automatically.
- **why**: PaLM's reconnect-and-resend rule can repeat a command that the
  analyzer already acted on. Analyte deliberately requires reconciliation
  instead; this is a safety restriction against that recovery rule.
- **source**: PaLM TF-2x Rev11 §2.2.4, p.10;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle rule.
- **test**: `law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain`.

## 10. Preflight and reserve work before a positive query response

- **what**: Validate and reserve each AWOS through the shared order path
  before emitting a positive RSP and its work follow-up.
- **why**: A positive response followed by a rendering or reservation failure
  would promise work the bridge cannot safely send. Durable per-item ownership
  also lets restart recovery account for every possibly transmitted AWOS.
- **source**: [ADR 0003](../../adr/0003-clinical-safety-rules.md), order lifecycle
  and durable identities.
- **test**: `law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work`.

## 11. Reserve negative work replies for trusted empty lookups

- **what**: Cache failure, stale data and restore pause produce query errors,
  rather than the negative LAB-28 used for a genuinely empty lookup.
- **why**: The profile's no-work exchange has an operational meaning to the
  analyzer. Using it for unavailable order information would turn uncertainty
  into an incorrect instruction that no tests are required.
- **source**: PaLM TF-2b Rev11 §3.27.4.1.3, p.44;
  [ADR 0003](../../adr/0003-clinical-safety-rules.md), Lookup failure is not no orders.
- **test**: `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work`.

## 12. Preserve unverified results without upstream delivery

- **what**: Apply the syntax-owned status lifecycle in the common store;
  retain unverified revisions visibly while withholding result delivery.
- **why**: An unverified observation may legitimately advance to final later.
  Keeping its history supports that progression without publishing a revision
  that is not reportable or treating the final result as a conflict.
- **source**: [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `law_adr0011_c7_all_status_moves_and_unverified_delivery`.

## Complete observation identities

TF-2b C.7 Table C.7-4 requires distinct observations under one OBR. Results
validation rejects a repeated primary code/system and complete OG before
returning any observations, locating OBX-4 at its whole-message occurrence.
Test: `law_tf2b_c7_repeated_observation_requires_unique_complete_og`.

## Retained work matching

Received OBR-2 uses only EI.1 to find retained work for the configured machine.
Other OBR-2 components are X under TF-2b 2.2.1/2.2.2 and C.6-2: they remain
in protected original bytes but do not authorize or reject a match. Specimen,
container and test comparisons still reject conflicting returned work as a whole.
Test: `law_tf2b_c6_2_x_awos_components_do_not_change_retained_work_match`.
The combined receive/structure qualification row remains open.
