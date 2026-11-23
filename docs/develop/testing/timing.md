# Test timing

How tests wait on timers and the store, and the measurements behind those
limits.

## Wall-clock budgets

`analyte_fixtures` defines harness and store-request budgets with a fixed CI
scale factor of three: a 180-second hang guard and a 30-second store wait.
These allow scheduler delays and durable writes on loaded Windows runners;
expiry still fails the test. Synthetic loopback service fixtures explicitly
apply the store allowance to delivery workers and order pollers. Tests that
restart during delivery or finish a wire capture can also use it for shutdown
I/O. Keep blocking operator checks outside an active synthetic conversation
so the peer can respond to each poll promptly. Production
entry points reject these overrides, and production deadlines stay unchanged.
Do not use these allowances for a protocol timer whose expiry is under test.
Keep those configured deadlines and quiet-window assertions exact. Name waits
with the integration `step` helper so an expiry identifies its operation; print
only synthetic case labels when looping over scenarios.

## Store tests that flush the disk often

CI runs the tests with cargo-nextest. A few store tests commit to a store
hundreds of times, each commit a full disk flush, and on Windows runners they
slowed the disk enough for other tests' deadlines to expire. They share the
`store-flush-heavy` group in [`.config/nextest.toml`](../../../.config/nextest.toml),
which runs them one at a time; the tests themselves are unchanged. Add a new
test to that group only when it does the same kind of repeated flushing.

## Service test concurrency and startup benchmark

To repeat the manual capacity measurement (creates and
removes a synthetic store of at least 2 GiB in a temporary directory), run:

```text
cargo test -p analyte-store --release startup_timing_two_gib_synthetic_store -- --ignored --nocapture
```

Windows measurement, 2026-11-15: a 2,150,469,632-byte synthetic store took
1.713..1.775 seconds for integrity checking and 1.604..1.630 seconds for
restarts. These are local disk/cache measurements.

The global test-thread override was removed after the startup fix. Three concurrent
full analyte integration binaries with default threads passed in 35.19, 35.64 and
35.49 seconds (127 passed, one external-network test ignored in each). Concurrent
comparison runs with explicit four and two threads took 66.85 and 126.31 seconds.
These runs also overlapped compilation; the numbers describe that load rather than
an isolated performance benchmark. No fixture semaphore was needed.
