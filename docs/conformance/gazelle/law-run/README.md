# Gazelle LAB-28 run, 2026-11-15

Outcome: **recorded interoperability failure; cancellation not reached**. Evidence
levels remain **documented**. This is a real simulator conversation, not a claim
of a simulator pass or complete LAW actor conformance.

## What ran

Analyte was the LAW Analyzer Manager, making one outbound persistent MLLP TCP
connection to `151.80.220.161:10112`. The peer was IHE Gazelle Order Manager's
Laboratory Analyzer simulator. MSH-5/MSH-6 were `OM_LAB_ANALYZER` / `IHE`.
The machine configuration selects basic content, no enhanced peer options,
send-ahead, a 10-second response budget and synthetic test `SYN^Synthetic^99LAB`.
Both orders have one item. There are no patient identifiers or patient segments.
Sample IDs are `SYN-0001` and `SYN-0002`; the configured authority is
`ANALYTE-TEST`.

The run used the owner's approved alternative: the ignored
`gazelle_cases::gazelle_lab28_live` integration test in
`crates/analyte/tests/service/gazelle.rs`. It calls the real
`service::serve_loopback_test`, with the same program configuration loader,
machine owner, transport, LAW stack, durable store, order poller, delivery worker
and live capture as the foreground program. Only the lab-app destination uses
the existing `Destination::for_loopback_test` seam, to the reference receiver on
`127.0.0.1:59273` over HTTP. No production HTTPS verification option was added.
The site file's reserved HTTPS URL is replaced before service startup and is
never contacted. This is not evidence of the foreground binary's process/signal
path or a production HTTPS app connection.

Platform: Windows; Rust `1.100.0 (0ef1bc386 2026-11-09)`. Feature source commit:
`a6a4c89c415fcf046ef863bea6fb0d6df811b7b6`; the run additionally used the ignored
harness and configs included with this artifact. Selected local standards source:
PaLM TF Rev 11. Gazelle tool build and deployed validation profile revision were
not supplied over MLLP and were not independently retrieved. No account was used.
The linked simulator status page was not contacted.

## Commands and guardrails

From the repository root in PowerShell, with Cargo on PATH:

```powershell
cargo run -p analyte -- check-config docs/conformance/gazelle/law-run/analyte.toml
$env:ANALYTE_GAZELLE_ENDPOINT = '151.80.220.161:10112'
cargo test -p analyte --test delivery gazelle_cases::gazelle_lab28_live -- --ignored --exact --nocapture
cargo run -p analyte -- --config docs/conformance/gazelle/law-run/analyte.toml --actor gazelle-operator orders list
```

The test is ignored in normal CI and refuses a missing or different endpoint
variable. It refuses an existing `analyte.db`, so invoking it again cannot reuse
or replay these machine orders. The receiver is started in-process using its
normal router/storage implementation, seeded from `seed-orders.json`. The
planned second seed, `seed-cancel.json`, is applied only after both originals
have Accepted item outcomes; it was **not applied** in this run. The harness
waits at most 35 seconds for exchanges and bounds service shutdown to 30 seconds.
Transport/capture/session failures wake its stop path. There were zero failed
connection attempts, one successful connection and no manual retry.

The initial draft program file failed local storage-limit validation before any
network connection; its database budget was corrected to exceed the reserve.
The configuration committed here passed, digest
`f2be40ec7bff873d5f6d92fbf0ee9c743437a3016cacfcd6ca8716360c0eec07`.
A temporary loopback TLS setup was abandoned before the run at the owner's
request: the Windows trust prompt was dismissed, no certificate remained in the
certificate store, and the temporary private key/certificate files were removed.

## Wire and durable outcomes

Times below are relative to the recording's first event at
2026-11-15 approximately 12:34:36 UTC.

| Time | Exchange | Observed outcome |
| --- | --- | --- |
| 0 ms | TCP client link opens | One persistent link to the authorized peer |
| 79 ms | OML NW for SYN-ORDER-0001, AWOS `1-1` | Complete MLLP block sent |
| 1074 ms | ORL O34 / ORL_O42 | MSA AA correlates to the request; ORC OK/SC names AWOS `1-1`, but control validation fails before item acceptance |
| 1252 ms | OML NW for SYN-ORDER-0002, AWOS `2-1` | Sent before the harness observed the first item's Uncertain outcome |
| 1279 ms | Host cancellation of connection | Clean shutdown; no second ORL was observed |

Gazelle's exact reply contains `MSH-7 = 20261115133437`, with no UTC offset.
Analyte reports `LAW component rejected at MSH-7.1`; the existing validator
requires seconds precision and an explicit offset. The relevant local source is
`crates/hl7/src/law/control.rs` (`validate_control`, C.4), and the existing test is
`law_tf2b_c4_msh_7_m_ts_precision_calendar_timezone_and_singleton` in
`crates/hl7/tests/law_control.rs`. PaLM TF-2b Rev 11 Appendix C.4,
Table C.4-6 requires MSH-7 to carry the time zone; it is the one timestamp that
must include it, and all other times in the message are relative to it.
The simulator's ORL omits that required element and is non-conformant to Rev 11.
Analyte's refusal is the conformant behaviour. No tolerance was added: no
timestamp was repaired, no timezone was guessed, and no parser or dialog was
changed to make this run pass.

The reply's MSA-2 is `01a1df33ccc873c18d414f31167d93b5`, matching the first
outgoing MSH-10. Its per-item control is `ORC|OK|1-1|||SC`. Those wire values were
not promoted to clinical acceptance after failed message validation. Exact bytes,
including MLLP framing and the original read/write chunks, are in
[lab28.rec.jsonl](lab28.rec.jsonl).

The protected store retains the exact incoming MLLP reply and parks the failed
interpretation. `orders.txt` records the operator CLI output after shutdown:

- SYN-ORDER-0001 / item 1: Uncertain, `LAW-invalid-ORL`, with the located error.
- SYN-ORDER-0002 / item 1: Uncertain, `LAW-disconnect`.

`receiver-events.json` exports the reference receiver's durable order-event
records: one event for the first order, state `uncertain`, source status
`item-outcomes: 1=Uncertain:LAW-invalid-ORL`. The second order's disconnect event
remains **pending** in Analyte's outbox; it was not delivered before shutdown.
It is retained, not lost or replayed. `parked.json` contains the parking reason;
`outcome.txt` records the manual test failure and successful service shutdown
(`Ok(Ok(Ok(())))`). The live test exited 101 because its scenario did not complete;
service shutdown itself succeeded, within the 30-second bound.

## Evidence scope

This adds narrow live evidence to these `docs/protocols/law/README.md` rows, without
changing their checkboxes or raising any evidence level:

- Directional MLLP / persistent connection: the declared TCP client link carried
  outbound LAB-28 and an inbound ORL. It does not evidence independent links.
- Send work in OML and process ORL for every AWOS: two NW requests were sent;
  only the first received a reply, which was retained and rejected before item
  processing. Successful acceptance and cancellation remain unqualified.
- NE/AL and correlation: outgoing trigger headers selected NE/AL and the observed
  ORL reversed applications/facilities and echoed MSA-2. This is a single sample.
- Malformed/control-content and invalid-ORL handling: the raw reply was preserved,
  its specific control error parked, no ACK loop occurred, and acceptance was not
  fabricated. This is partial evidence for the corresponding error-handling row.

It does **not** cover simulator-initiated LAB-27 or LAB-29 (which need inbound
reachability and an account), the planned cancellation, valid ORL acceptance,
results, independent/directional multi-link operation, corrected/rerun results,
restart/replay, a real machine, or complete actor conformance. The profile conflict
must be resolved before repeating the scenario; neither order may be silently
replayed.

What remains: a run against a simulator build that sends MSH-7 with an offset,
or a separately decided, explicitly configured named deviation (never a default)
if real analyzers turn out to omit it.

## Artifacts and privacy

`analyte.toml`, `machine.toml`, `site.toml`, `seed-orders.json` and
`seed-cancel.json` are the run inputs. `capture = true` is a program setting;
the machine definition remains documented. Runtime databases and the original
machine-labelled capture remain under the ignored `data/gazelle-law-run/`.
No databases, keys or credentials are committed.

The committed recording is a copy. All decoded events were inspected: no patient
identifiers occur. Only its header was changed to `simulator` evidence,
`synthetic data only`, the named Gazelle source and capture date. Event lines and
bytes are unchanged, including the local socket address in link metadata. The
protected original retains its `machine` / `NOT de-identified` header.

## Verification

After adding the harness and artifacts, all four offline checks passed on Windows:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

The normal workspace run reports the live Gazelle test as ignored. Its separate,
explicit live invocation failed as documented above; it was not retried or made
to pass by changing expectations. Cargo deny reported the existing duplicate
version warnings, with advisories, bans, licenses and sources all passing. Linux
and minimum-version runs remain CI work, not checks claimed for this session.
