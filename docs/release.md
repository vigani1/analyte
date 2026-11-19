# Releasing Analyte

Only the owner cuts and publishes releases. The release workflow runs only on
pushed tags matching `v*.*.*`; branch pushes and pull requests do not release.
Do not create a tag to test this workflow.

1. In a reviewed PR, bump `workspace.package.version` in `Cargo.toml`, update
   the versions of workspace path dependencies together, regenerate `Cargo.lock`
   with Cargo, and move the relevant Unreleased changelog entries under the new
   version and date. Run the normal CI checks and merge to main.
2. From a clean checkout, fetch main, check out its intended release commit,
   and confirm its version and changelog. Create an annotated tag, for example
   `git tag -a v0.1.0 -m "Analyte 0.1.0"`, then explicitly push that tag with
   `git push origin v0.1.0`. Never move an existing release tag.
3. Wait for the Release workflow on that tag. It verifies tag/version equality,
   formatting, Clippy, tests, docs, minimum Rust and cargo-deny using CI's
   commands. Both native builds use the pinned Rust toolchain and locked
   dependencies, without all features; a feature check refuses `test-hooks`.
4. Review the draft release and download its assets into an empty directory:
   `gh release download v0.1.0 --repo vigani1/analyte --dir release-check`.
   Authenticate as the owner to read the draft. In that directory on Linux,
   run `sha256sum --check SHA256SUMS`. On Windows, compare
   `Get-FileHash -Algorithm SHA256 <asset>` with each entry in SHA256SUMS.
5. Verify provenance for **each** archive, installer, package,
   `dependencies.json` and SHA256SUMS:
   `gh attestation verify <asset> --repo vigani1/analyte --signer-workflow vigani1/analyte/.github/workflows/release.yml`.
   Inspect the verified statement and confirm its source commit and tag match
   the intended release, rather than merely accepting a successful signature.
6. Inspect archive contents and test installation on clean Windows and Linux
   hosts with synthetic data: run the Windows installer, install the `.deb` and
   the `.rpm`, open the setup page and uninstall (see
   [Install](operations/install.md)). Review the changelog and replace the draft's short
   placeholder notes with the version's release notes. Publish the draft in
   GitHub only after these checks, or explicitly use
   `gh release edit v0.1.0 --repo vigani1/analyte --draft=false`.

Each platform's assets come from one script, `packaging/build.ps1`, which the
pull-request Packaging check also runs and then installs and removes on the
runners:

| Asset | Contents |
| --- | --- |
| `analyte-<tag>-x86_64-pc-windows-msvc.zip` | analyte.exe, analyte.ico, machines, packaging/windows, LICENSE, README.md |
| `analyte-<tag>-x86_64-windows-setup.exe` | Click-through installer built with Inno Setup 6.7.4 from the same files |
| `analyte-<tag>-x86_64-unknown-linux-gnu.tar.gz` | analyte, machines, packaging/linux, LICENSE, README.md |
| `analyte_<version>-1_amd64.deb`, `analyte-<version>-1.x86_64.rpm` | Packages built with nfpm 2.48.0 from the same files |

The archives have a versioned top-level directory. Follow the packaged service
instructions with the extracted program. The build tools are downloaded at a
pinned version and refused unless their SHA-256 matches; they are not Rust
dependencies and are not shipped. The root README still describes the original
planning status; use CHANGELOG.md and the program and packaging guides for
current functionality.

**The Windows installer is unsigned for now: Windows SmartScreen will warn;
choose More info → Run anyway** after checking its checksum and provenance as
above. The draft release notes say the same.

`dependencies.json` is Cargo's full `cargo metadata --locked --format-version 1`
output, including the resolved graph and development dependencies. It is a
workspace inventory, not a target-specific SBOM or a claim that every listed
package is linked. This fallback avoids adding a new third-party SBOM tool.
SHA256SUMS covers the archives, installer, packages and inventory; it cannot
hash itself. GitHub provenance covers every asset. Actions are pinned to full commit
SHAs with release comments; no credentials beyond GITHUB_TOKEN are needed.

If a workflow fails, inspect the failed job before rerunning. A failure before
draft creation creates no release; failure during creation may leave a draft.
Inspect and retain or remove that draft deliberately before retrying; the
workflow does not replace an existing release or publish automatically.

Code signing (Windows Authenticode for the program and the installer, and
package signing for the .deb and .rpm), automatic updates and reproducible-build
guarantees are not implemented. GitHub provenance
identifies the build workflow; it does not qualify an analyzer or replace lab
validation. Evidence remains documented unless separately supported by recorded
simulator or real-machine runs.

References: [GitHub provenance action](https://github.com/actions/attest-build-provenance),
[Cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html),
[Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).
