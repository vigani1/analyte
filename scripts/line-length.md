# Production line lengths

The Layout job rejects production Rust lines longer than 140 characters.
It scans crate Rust files, excluding integration tests, testing crates, inline
test-only items and comment lines. It reuses the duplication check's Rust
literal and test-item scanner so braces inside wire fixtures do not end items.

The JSON allowlist retains exact existing lines containing string literals,
including SQL, wire text and operator messages. Entries are scoped to a path
and counted: copying a grandfathered line creates a new violation. Editing
one also requires shortening it or explicitly reviewing an allowlist change.
It is a compatibility baseline, not a general exemption for new strings.
Shorten surrounding code before adding any exception. The gate never rewrites
literal bytes. Run its three regression tests with
`python scripts/test-line-length.py`, then run
`python scripts/check-line-length.py`.
