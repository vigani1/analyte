# Rust contributor and agent practices for Analyte

Research checked **2026-11-15**. Read Analyte's `AGENTS.md` first, then `docs/plan.md` (planning, nothing built).

## 1. Summary

- Keep the clinical rules: preserving results and refusing an unsafe acknowledgment matter more than style agreement.
- Serious projects repeatedly automate formatting, review dependencies, test public behavior and explain unsafe invariants.
- Add explicit cancellation contracts, exhaustive internal state matches, isolated tests and reviewed regression fixtures.
- Tighten the unsafe exception process; a safety comment alone is insufficient.
- Clarify panic handling; neither arbitrary bytes nor routine I/O failures should panic.
- Relax error-enum-per-module and blanket future-proofing rules where they create work without a caller need.
- Keep agent instructions concrete, linked to human documentation and enforced by configuration where possible.
- AI policies vary from human accountability to outright bans; an agent file is not permission to submit AI work.
- Start with the checks already planned; consider Miri, nextest and concurrency model checking only where justified.

### Method and evidence

**Verified (V)** means I read the upstream file or official page, sometimes a relevant section rather than the entire document. **Inferred (I)** means my recommendation or synthesis, not an upstream requirement. All proposed Analyte wording is I. Links below identify the supporting source for each reported rule. Repository files were read with `gh api repos/OWNER/REPO/contents/PATH`; recursive Git trees were checked for agent/config/workflow files. Website sources were read on their official sites.

This is a purposive sample of mature projects, not a measured ranking of size, quality or compliance. A checked-in rule proves a stated requirement; it does not prove universal enforcement. “No agent file found” below refers to the requested filenames in the returned Git tree, not proof that a project has no private or differently named instructions. No `.cursor/rules/` files appeared in the sampled trees; that is not an ecosystem-wide finding.

GitHub source links use the checked HEAD's abbreviated commit ID. Dates in the table are **HEAD commit dates**, not the last edit date of each file. These snapshots were current on their repository default HEAD when checked. Official web pages are rolling documents unless otherwise stated; I did not verify each page's revision history. HEAD versions are not necessarily released product versions.

## 2. Projects and rules found

All notable rules in this table are **V**. Config names without links were also located/read where discussed in section 5; this is a selected inventory, not every file in each repository.

| Project / checked HEAD date | Rule files found | Notable rules |
| --- | --- | --- |
| Official Rust API Guidelines | [Checklist][api], dependability, documentation, future-proofing | Newtypes distinguish meanings; types can enforce validation; document errors, panics and unsafe preconditions; protect representation with private fields. Library API advice, not a medical safety standard. |
| rust-lang/rust — Nov 13 | [AGENTS.md][rust-agent] (265 lines), CONTRIBUTING, rustfmt, dev guide, CI | Use `./x`, including formatting; compiler-specific pinned formatter. Agent changes require advance human review arrangements; restrict generated prose and soundness-sensitive implementation; reproduce bugs before fixes. |
| rust-lang/cargo — Nov 14 | [CONTRIBUTING][cargo-contrib], contributor book, clippy.toml, deny.toml, Cargo.toml, main/audit/release workflows; no requested agent filename found | [Integration tests][cargo-tests] run the actual binary in temporary projects, avoiding public network dependencies. [LLM policy][cargo-ai] follows Rust's current policy. Configuration redirects environment access through its owner. |
| Microsoft Pragmatic Rust Guidelines — Oct 26 | [CLAUDE.md][ms-agent] (62 lines), named/versioned guidelines | [Unsafe][ms-unsafe] needs justification and safety reasoning; benchmark performance exceptions; Miri for new abstractions/performance unsafe. [Lint exceptions][ms-expect] use `expect` with reasons. Agent must not invent the substance of ambiguous new guidelines. |
| Google / Android | [Google Rust Style Guide][google], [AOSP Rust module documentation][android], [Comprehensive Rust][course] | Readability, contextual API design, safe abstractions, justified unsafe, standard traits and meaningful newtypes. AOSP enables compiler/Clippy lint sets by default, distinguishing platform and vendor code. Comprehensive Rust is a course, not a contributor policy. |
| Google / Chromium | [Current first-party unsafe policy][chrome], Google guide | Unsafe is discouraged and opt-in per GN target; contracts and safety comments required. Current first-party policy explicitly does not require an extra specialist approval. Third-party code has a separate policy. |
| AWS Firecracker — Nov 13 | [CONTRIBUTING][firecracker], Cargo.toml, deny.toml, rustfmt/toolchain, devtool and CI | Two maintainer approvals; tests with new functionality; unsafe needs both technical justification and every relevant invariant explained. Avoid unwrap; DCO sign-off. These are Firecracker requirements, not automatically all AWS Rust requirements. |
| Cloudflare Pingora — Nov 9 | [.github/CONTRIBUTING][pingora-contrib], clippy/rustfmt, [build workflow][pingora-ci], audit workflow; no requested agent filename found | Discuss larger design decisions; maintainers batch accepted changes with internal changes. CI tests compiler versions, uses nextest plus doctests and seeded fuzz-soak replay. |
| Linux kernel Rust | [Coding guidelines][kernel] | Rustfmt; `SAFETY:` comments before every unsafe block; `# Safety` documents caller obligations. Prefer fallible APIs; document justified panics. Kernel build commands and no-std constraints differ from this application. |
| Mozilla / Firefox | [Rust update policy][mozilla] | Stable toolchain for official stable builds; separate build toolchain from MSRV; minimum increases only when needed and after Nightly exposure; preserve ESR compatibility. This is not a whole-browser Rust rewrite. |
| Servo — Nov 14 | CONTRIBUTING redirects to [book/style guide][servo-style]; [AI policy][servo-ai], deny/rustfmt/toolchain, CI | Avoid unsafe and unwrap; invariant-only expect with a reason; no reachable todo/unimplemented. **AI-generated contributions are prohibited**, including code and prose. Servo is now a separate project; do not attribute its current policy to all Mozilla work. |
| Astral uv — Nov 13 | [AGENTS.md][uv-agent] (49 lines), CONTRIBUTING, clippy/rustfmt/toolchain, `.cargo/deny.toml`, CI | Focused integration scenarios; exhaustive matches; avoid panic/unsafe; reasoned lint expectations; targeted lockfile updates. [Astral AI policy][astral-ai] requires human understanding and human communication; bars autonomous external contributions. |
| Astral Ruff / ty — Nov 14 | [AGENTS.md][ruff-agent] (240 lines), CLAUDE.md (one-line include), CONTRIBUTING, clippy/config, daily fuzz workflow | Review regenerated snapshots; readable small reproducers; crate-specific workflows and threat-model links. Same external AI policy as uv. Some documented commands force-update snapshots: review is essential. |
| Zed — Nov 14 | [AGENTS.md][zed-agent] (190 lines), CLAUDE.md with same guidance, clippy/rustfmt/toolchain | Correctness before speed; avoid panic/indexing traps; never silently discard errors; deterministic framework timers. Human-review README marker. High bar for new agent rules: non-obvious, recurring, actionable. |
| Deno — Oct 26 | [CLAUDE.md][deno-agent] (433 lines), [Copilot instructions][deno-copilot] (352), `.github/CONTRIBUTING`, per-crate Clippy, rustfmt/toolchain, generated CI | Real CLI spec tests through the Cargo harness; focused changes; no force pushes; wrapper commands for formatting/linting. Copilot reviews must verify claims and distinguish fixtures. |
| Biome — Nov 14 | [AGENTS.md][biome-agent] (88 lines), contributor guides, scoped skills, clippy/rustfmt/deny/toolchain | Evidence for repository claims; regression fails without fix; narrow tests first; generated artifacts regenerated; final review; user-facing changeset and AI disclosure. |
| Oxc — Nov 14 | [AGENTS.md][oxc-agent] (338 lines), many crate AGENTS, [Copilot review rules][oxc-copilot], configs, Miri/CI | Human understands/tests AI output and discloses use; conformance suites guide parser work; never invent compiler failures. Intentionally invalid fixture code is not a production-code defect. |
| ripgrep — Nov 9 | [CONTRIBUTING / AI_POLICY][ripgrep-ai], rustfmt, CI/release; no requested agent filename found | Human must understand changes and explain them personally; autonomous contributions forbidden; AI-written maintainer replies forbidden. |
| Vercel Turbopack — Nov 15 | [turbopack/AGENTS.md][turbo-agent] (28 lines), crates/AGENTS (5-line shared link), root instructions/configs/CI | Keep bundler independent from Next.js concepts; precise cache dependencies; account for eventual consistency across cells. Checked bounds alone do not prevent reading the wrong in-range value from mismatched executions. |
| Rolldown — Nov 14 | [AGENTS.md][roll-agent] (107 lines), CONTRIBUTING, clippy/rustfmt/toolchain, CI | Read/update rationale docs; generators own generated files; prefer `just`; draft PRs until user requests review. Windows CI is label-triggered. Rolldown is not a Vercel repository. |
| Helix — Nov 12 | [docs/CONTRIBUTING][helix], architecture docs, rustfmt/toolchain, CI/release; no requested agent filename found | Workspace unit/doctests plus editor integration tests; deliberately low MSRV following Firefox/distribution availability. |
| Bevy — Nov 13 | CONTRIBUTING links to website and [AI policy][bevy-ai]; Cargo workspace lints, clippy/rustfmt/deny, CI; no requested agent filename found | Human ownership/disclosure; no AI-generated public prose/media/communication or AI commit authors. Workspace denies unsafe by default and warns on undocumented unsafe; exemptions are possible. |
| Safety-Critical Rust Consortium — Nov 14 | [Guidelines repository][scr], contributing/review/release docs, guideline metadata and FLS audit workflows | Published site labels itself 0.1; [process][scr-style] distinguishes draft/approved/retired rules, requirement levels and compiler applicability. Do not treat every proposal as an approved rule. |
| Ferrocene / embedded Rust | [Safety Manual scope][ferrocene], [Embedded Rust Book panic chapter][embedded] | Qualification is specific to compiler, libraries, targets and usage constraints; application correctness is separate. Embedded panic response depends on the system; no universal halt/reboot policy. |

## 3. Rules to adopt

These are proposed text, **not edits to AGENTS.md**. “Why” and applicability are I; named upstream practices are V at the cited links. Prioritize 1–7 for the first implementation; later tooling remains a proposal under our stop rule.

### 1. State cancellation behavior at stateful boundaries

**What/why:** A timed-out future can already have changed a file, written bytes or committed storage. Cancellation must not turn an uncertain outcome into a false success or an unsafe retry. This makes the existing timeout/identity rules operational.
**Sources:** [Turbopack][turbo-agent] explicitly warns about cross-execution state consistency; [Pingora CI][pingora-ci] includes cancellation-related coverage. The exact Analyte cancellation contract is **I**, not a shared quoted upstream rule.
**Exact line:** “For each stateful async operation, document what cancellation can leave behind; a timeout never proves that nothing happened, and retries must keep the same identity.”

### 2. Match internal states exhaustively

**What/why:** Adding a protocol state should force a decision at every handler. A wildcard can silently absorb a new corrected-result or order state.
**Sources:** [uv][uv-agent] explicitly prefers exhaustive match arms; [Google][google] also asks for explicit variants where possible. **I:** limit the strongest rule to our internal state machines.
**Exact line:** “Match every variant of our protocol and lifecycle enums explicitly; use a wildcard only for an external non-exhaustive type, and explain its behavior.”

### 3. Make invariants enforceable through the public API

**What/why:** A checked constructor does not help if callers can mutate the validated fields afterward. This closes a gap in “validate once.”
**Sources:** [API Guidelines C-STRUCT-PRIVATE][api-future], [C-VALIDATE][api-depend], and [Google newtypes][google].
**Exact line:** “Keep fields private when callers could break an invariant; constructors and methods preserve the rule, including after deserialization.”

### 4. Separate the reason for unsafe from its proof

**What/why:** “Needed for serial I/O” explains motivation, not pointer ownership, aliasing or lifetime correctness. Preserve the current ADR requirement.
**Sources:** [Firecracker][firecracker], [Microsoft M-UNSAFE][ms-unsafe], [kernel][kernel], [Chromium][chrome].
**Exact line:** “An approved unsafe exception stays behind a small safe API; explain why safe Rust cannot do the job, document every safety precondition and why it holds, and test it with Miri where supported.”
**Limit:** Miri covers executed paths and supported operations; its success is not proof of soundness or clinical safety. No first-party unsafe is currently needed just to write this report.

### 5. Test through isolated, reproducible boundaries

**What/why:** CI must not depend on an analyzer, public API availability, real credentials, host clock scheduling or a user's files. Real-machine validation remains a separate evidence step.
**Sources:** [Cargo temporary-project/network rules][cargo-tests], [Zed deterministic timers][zed-agent], [Deno real CLI harness][deno-agent], [Helix integration tests][helix].
**Exact line:** “Automated tests use temporary stores, local test receivers and controlled time; they never contact real analyzers or lab apps, and never use live credentials or patient records.”

### 6. Preserve failures as readable regression cases

**What/why:** Fuzzing finds inputs; durable fixtures explain and guard the fix. A source reference and the original minimized bytes make parser behavior reviewable.
**Sources:** [Biome][biome-agent] requires a regression that fails without the fix; [Ruff][ruff-agent] calls for minimal readable reproducers and review of regenerated expectations; [Oxc][oxc-agent] uses external conformance suites.
**Exact line:** “Turn every parser or dialog bug into a small reproducible regression case; keep the failing bytes and source reference, and review changed expectations against the protocol before accepting them.”

### 7. Use narrow, justified lint exceptions

**What/why:** Some lints have false positives. Restructuring clear code solely to satisfy one is not always safer; silent broad suppression is worse.
**Sources:** [Microsoft][ms-expect], [uv][uv-agent], [Biome workspace lints][biome-cargo], [Bevy workspace lints][bevy-cargo].
**Exact line:** “Prefer fixing a lint; when an exception is justified, use the narrowest `#[expect(..., reason = "...")]`, never a blanket suppression to make checks pass.”
**Limit:** `forbid(unsafe_code)` cannot be bypassed by a local expectation. Keep ADR-controlled unsafe outside that ordinary exception path.

### 8. Make dependency changes deliberate and bounded

**What/why:** A dependency update is a behavior and supply-chain change, not incidental formatting. Asking before additions already exists; extend that discipline to updates.
**Sources:** [uv][uv-agent] prohibits whole-lockfile updates and requests precise updates; [uv deny config][uv-deny] rejects unknown registries/git sources and reviews build scripts; [Cargo deny config][cargo-deny] has explicit license/source policy.
**Exact line:** “Update only the dependencies needed for the task; review their lockfile changes, enabled features, build scripts and sources, and keep unrelated upgrades out.”

### 9. Treat generated files and fixtures according to their role

**What/why:** Regenerating a snapshot is not independent verification, and deliberately malformed bytes are valid test data.
**Sources:** [Ruff][ruff-agent], [Rolldown][roll-agent], [Oxc Copilot rules][oxc-copilot], [Deno Copilot rules][deno-copilot].
**Exact line:** “Regenerate generated files with their owning tool and inspect the diff; treat malformed fixtures as test inputs, and never accept changed snapshots just because the tests then pass.”

### 10. Require evidence and human accountability for AI work

**What/why:** Fast plausible answers are dangerous when they invent a protocol rule or claim a check passed. Our evidence-level system is already stronger than many general projects.
**Sources:** [Biome][biome-agent] requires evidence for claims; [Oxc][oxc-agent] requires human review, understanding and disclosure; [Bevy][bevy-ai] requires ownership and disclosure; [Rust][rust-agent] has stricter gates.
**Exact line:** “AI-assisted changes need a human who understands and reviews them before submission; disclose the assistance, cite evidence for technical claims, and never claim a check ran without its result.”
**Choice, I:** Adopt accountability, not Servo's complete ban or Rust's compiler-specific restrictions. Human-facing contribution policy should also describe the same requirement when that documentation is created.

## 4. Rules we have that others disagree with or do differently

All changes below are **I**. Keeping a stricter Analyte rule can be right; popularity does not settle a clinical risk.

| Current rule | Verified difference | Recommended change / exact replacement wording |
| --- | --- | --- |
| No unwrap/expect/panic on runtime paths, followed by an invariant-expect exception | [Microsoft][ms-panic] positively requires panic for unrecoverable programming bugs, while parse failures return errors; [Servo][servo-style] permits invariant expect/assertions. | Resolve the apparent contradiction: “External input and routine I/O failures return errors, never panic. A proven internal invariant may use `expect` with its reason; never catch a panic and continue a protocol session as if it succeeded.” Keep stronger protection for byte-handling paths. Decide actual process/task failure and restart behavior during implementation; do not silently choose `panic=abort`. |
| One typed error enum per module boundary; only binary top may use catch-all | [Microsoft application guidance][ms-app-error] permits one common application error type in app-only crates; its library guidance does not prescribe an enum per module. | “Use typed errors where callers must distinguish recovery actions; group them by a meaningful API boundary, preserve causes, and use one catch-all type only in application orchestration.” Keep typed parse/storage/delivery decisions. Do not change the current rule until that narrower scope is agreed. |
| Public types that may grow are non-exhaustive | [Google][google] emphasizes cross-team APIs and has exceptions; [uv][uv-agent] wants exhaustive handling internally. | “Use `#[non_exhaustive]` for intentionally extensible APIs consumed outside the workspace; keep internal state enums exhaustive.” Crate-level `pub` alone does not mean a third-party API commitment. |
| Selected pedantic lints, rather than allowing warnings case by case | [uv][uv-cargo], [Ruff][ruff-cargo] and [Oxc][oxc-cargo] enable broad groups with explicit allow lists; [Biome][biome-cargo] selects individual lints. | Keep selected lints, allow reasoned local `expect` exceptions as rule 7. There is no common “enable every lint” practice. [Clippy itself][clippy] warns about pedantic false positives and blanket restriction groups. |
| About ten files per folder, filenames never repeat folder | [Zed][zed-agent] discourages many tiny files; Chromium's [Google guide][google] uses flat crate layouts for its own build system; [Oxc][oxc-cargo] warns on self-named module files. | “Group files by a clear owner; split a module when it has a separate responsibility, not to meet a file-count target.” Keep descriptive names; drop a rigid numerical limit. |
| Every wait has a timeout | [Zed][zed-agent] emphasizes task/timer ownership in its executor, not a universal wall-clock wrapper. The sources do not establish a universal timeout rule. | “Every external operation has a deadline; long-lived service loops and idle waits must be cancellable, and shutdown must have a bounded completion time.” Applying timeout to a loop does not safely interrupt blocking serial/storage work; inspect those APIs explicitly. |
| Never hold a lock across await | [Google][google] discusses scoped accessors; the sample does not establish a universal ban on async mutex guards. | Keep the conservative default. Clarify if needed: “Never hold a blocking mutex guard across `.await`; any async-lock exception must name the protected invariant and why another ownership model is insufficient.” Do not add async locks speculatively. |
| Ask before adding any dependency | [Firecracker][firecracker] and [Cargo][cargo-contrib] use review processes; [uv][uv-deny] also encodes supply-chain policy. | Keep: it is proportionate during planning and complements the stop rule. Do not present it as a universal Rust convention. A documented dependency already authorized in the plan need not trigger repeated approval. |
| Full verification before handoff | [uv][uv-agent], [Biome][biome-agent] and [Oxc][oxc-agent] favor targeted iteration; Oxc reserves its broad checkpoint for integration. | Keep full final gates for this small safety-sensitive workspace. Add: “Run focused checks while iterating; before handoff run the required full checks and report every omission.” |
| Conventional Commits; no history rewrites without asking | [Zed][zed-agent] explicitly rejects conventional prefixes for PR titles; [Firecracker][firecracker] asks contributors to amend/force-push; [Deno][deno-agent] forbids force-push. | Keep our rules. These are collaboration choices, not correctness findings. DCO sign-off, used by Firecracker, is separate from signing commits cryptographically or adding AI co-author trailers; decide contributor legal policy before public contributions. |
| Clinical safety rules, proof levels, original bytes and data-free ordinary logs | None of the sampled general contributor guides replaces this domain contract. Rust memory safety does not establish result identity, units or durable acknowledgment semantics. | Keep unchanged. General logging/debug-print advice from Deno/Zed must not override patient-data restrictions. No reduction justified by this research. |

## 5. Tooling and configuration others use

All observations are **V**, checked 2026-11-15; recommendations are **I**. Values are from source, not installed or executed here.

### Toolchain and MSRV snapshots

| Source | Development/build pin observed | MSRV or support policy observed |
| --- | --- | --- |
| [uv toolchain][uv-toolchain] / [workspace][uv-cargo] | 1.99.0 | 1.97.0 |
| [Ruff toolchain][ruff-toolchain] / [workspace][ruff-cargo] | 1.99.0 | 1.97 |
| [Zed toolchain][zed-toolchain] | 1.98.1; rustfmt/clippy/analyzer/src components | Not established in this research |
| [Biome toolchain][biome-toolchain] | 1.100.0 | No workspace `rust-version` in the inspected workspace.package table; do not infer support from pin |
| [Oxc toolchain][oxc-toolchain] / [workspace][oxc-cargo] | 1.100.0 | 1.98.0 |
| [Firecracker toolchain][fc-toolchain] | 1.99.1; x86_64/aarch64 Linux musl targets | Pin is not an independently established MSRV |
| [Tokio CI][tokio-ci] / [policy][tokio-policy] | stable; lint 1.95; Miri nightly-2026-08-17 | 1.85 CI minimum; promises at least six-month-old compiler; increases on minor, not patch releases |
| [Pingora CI][pingora-ci] | nightly, 1.85.0, 1.97.1 matrix | 1.85 excludes pingora-foundations, whose higher requirement is checked elsewhere |
| [Helix toolchain][helix-toolchain] / [policy][helix] | 1.90.0 | Deliberately low, Firefox/distribution-informed; synchronize manifest, CI and toolchain |
| [Rolldown toolchain][roll-toolchain] / [Servo toolchain][servo-toolchain] | 1.100.0 / 1.99.1 | Separate MSRV not established here |
| [Firefox policy][mozilla] | Table lists Firefox 160 using 1.95.0 | Firefox 160 MSRV 1.90.0; future rows are schedules, not proof of releases |

**Recommendation:** Keep exact build-toolchain pins and an explicit, tested MSRV; distinguish the two. Review toolchain/lock changes together. Do not copy a HEAD pin just because it is newest. Edition 2024 is a project choice already made, not a reason to keep raising MSRV automatically.

### Checks and their limits

- **Workspace lints:** [Firecracker][fc-cargo] selects cast-truncation/wrap, unsafe documentation and other concrete lints; [Bevy][bevy-cargo] combines missing-docs, unsafe defaults and explicit exceptions; [Biome][biome-cargo] selects individual checks. **I:** enforce the clinical risks with targeted lints and behavior tests. Consider `unused_must_use`, checked casts, undocumented unsafe and lock-across-await checks; do not claim lints prove “no panic.” Member crates must actually opt into shared workspace lints.
- **Clippy configuration:** [Cargo][cargo-clippy] bans direct environment methods in favor of owned configuration; [Zed][zed-clippy] bans blocking process calls and nondeterministic timers in favor of its own APIs; [uv][uv-clippy] directs sensitive URLs through a redacted type. **I:** add disallowed-method/type rules only once Analyte has a real approved owner API. Clippy.toml parameters alone do not enable the associated lint.
- **Rustfmt:** the [official style guide][style] favors automatic common formatting; [rustc][rust-conventions] deliberately uses its own pinned formatter and `./x`. [Kernel][kernel] starts from defaults but has extra import conventions. **I:** use stable rustfmt with minimal configuration; do not copy compiler/nightly options or GN layout into Cargo.
- **Supply chain:** [Cargo deny][cargo-deny] has explicit SPDX choices, unknown-source policy, duplicate-version warnings; [uv deny][uv-deny] permits duplicates but denies unknown registries/git sources and checks build scripts; [Bevy deny][bevy-deny] records advisory exceptions with context. **I:** keep cargo-deny, review exceptions explicitly, and treat duplicate versions as a diagnostic unless a concrete conflicting version is forbidden. License allowlists are policy decisions, not automated legal opinions.
- **Nextest:** [Tokio CI][tokio-ci], [Pingora CI][pingora-ci], [uv CI][uv-ci] and [Ruff CONTRIBUTING][ruff-contrib] use/recommend it. Pingora pins **0.9.151**; other sampled invocations do not establish an exact installed nextest version. Tokio and Pingora run doctests separately. **I:** optional later speed/isolation improvement; `cargo test --workspace` is enough initially and must not disappear without replacing doctest coverage.
- **Miri:** [Oxc workflow][oxc-miri] pins **nightly-2026-10-04** and focuses relevant components; [Tokio][tokio-ci] uses its own pinned nightly. [Microsoft unsafe guidance][ms-unsafe] requires Miri in particular unsafe cases. **I:** use on pure parsers/model and approved unsafe code where supported, not as a promise that Windows serial drivers or an entire service can execute under Miri.
- **Fuzzing:** [Ruff daily workflow][ruff-fuzz] runs a budgeted Python-driven generator against the real Rust CLI; it is not evidence that Ruff uses cargo-fuzz there. [Pingora][pingora-ci] runs fixed-seed cases plus a million-case release soak, emitting replay seeds. [Android][android] provides `rust_fuzz` using libFuzzer. **I:** retain planned Linux byte-parser fuzz targets; cap input/resources, keep crash seeds and turn findings into deterministic regression tests. Choose a runner/version only when implemented.
- **Concurrency checking:** [Tokio Loom workflow][tokio-loom] explores schedules with explicit preemption/branch bounds, release mode and special cfg. **I:** useful if we implement a custom concurrency primitive; not a reason to build one. Use owned connection state, bounded channels, controlled-time dialog tests and cancellation tests first.
- **CI coverage:** [Tokio][tokio-ci] tests Windows/Linux/macOS, separate minimum Rust, feature configurations via cargo-hack, doctests and panic-abort cases. [Cargo CI][cargo-ci] covers multiple platforms/toolchains. [uv][uv-ci] has partitioned Windows tests; [Rolldown][roll-agent] requires a Windows label. **I:** Analyte must actually run tests on Windows and Linux, with supported feature configurations and MSRV; cross-compiling alone does not verify COM ports, filesystem replacement or service shutdown. An all-features check does not test the default/minimal feature set.
- **CI trust:** [Ruff fuzz workflow][ruff-fuzz] keeps untrusted builds separate from the issue-writing token, pins actions and disables persisted checkout credentials. [uv][uv-agent] forbids ad hoc privileged webhook workflows. **I:** keep PR jobs unprivileged and secrets out of untrusted contributor execution. Proposing this does not add a new workflow to Analyte.
- **Release process:** [Cargo][cargo-release] follows Rust's staged train; [Tokio][tokio-policy] defines patch scope and at least one-year LTS fix backports; [Firecracker][firecracker] requires review and buildable commits; [Firefox][mozilla] separates development upgrades from release branches. **I:** later document one release candidate commit, exact toolchain/lock/config versions, validation results and recovery procedure. Keep release machinery in the planned phase; no speculative signing/update subsystem now.

### Safety-critical and medical relevance

**V:** [Ferrocene's rolling safety manual][ferrocene] names rustc **1.101.0** in its current qualification scope; this is not evidence that stable Rust is that version or that a given distribution is covered. Scope distinguishes certified, qualified and uncertified libraries/targets; the end user has responsibilities for the usage environment. Its [IEC 62304 discussion][ferrocene-medical] explains its qualification approach, not qualification of Analyte.

**V:** The consortium's [published site][scr-site] is labeled **0.1**. Its [style/process specification][scr-style] defines rule status, categorization, rationales, compiler applicability and links to FLS paragraphs. Several inspected category index files were empty headings. Do not describe the entire catalog as a completed, approved MISRA-equivalent standard. Verify the status and version of any individual rule before adopting it.

**V:** The [Embedded Rust Book][embedded] describes several panic responses and explicitly says there is no one-size-fits-all behavior. Bare-metal rules such as no heap or a halt loop are not automatically suitable for a Windows/Linux bridge.

**I:** Borrow traceability and explicit deviations: requirement → protocol/source → implementation → regression/validation evidence, with reviewed exceptions. Keep the existing lab validation pack and evidence levels. A compiler qualification, safe Rust, fuzzing and unit tests cannot establish end-to-end clinical correctness or Analyte's regulatory classification. This report did not assess that classification or audit compliance with paid standards.

### Which tools were actually rewritten in Rust?

All statements here are **V** from project/owner descriptions, checked 2026-11-15. “Replacement implementation” matters: the original project's language may not have changed.

| Tool | Correct description / source |
| --- | --- |
| TypeScript compiler | Microsoft's native compiler/tooling port is **Go**, not Rust. [Microsoft's native-preview announcement][typescript] explicitly identifies Go. This research does not assert a particular release's rollout status. |
| GNU coreutils | [uutils/coreutils][uutils] is a Rust reimplementation; its README still warns of option/behavior differences. This does not mean GNU rewrote its own codebase in Rust. |
| sudo / su | [sudo-rs][sudo] is a separate Rust implementation, not proof that original sudo is now Rust; feature and platform differences remain. README lists audits of 0.2.0 (Aug 2023) and 0.2.8 (Aug 2025). |
| Python package tools | [uv][uv-readme] is a new Rust package/project manager replacing several workflows, not a port of pip or CPython. |
| Python linting/formatting | [Ruff][ruff-readme] is Rust software replacing lint/format workflows, not a rewrite of Python. |
| JavaScript bundling | [Rolldown][roll-agent] is a Rust/TypeScript bundler targeting Vite; [Turbopack][turbo-agent] is a Rust bundler in Next.js. Their existence does not make Rollup, webpack, TypeScript or Node wholly Rust. |
| Cloudflare proxying | [Cloudflare's Pingora account][pingora-story] describes replacing its previous NGINX-based infrastructure with its Rust proxy; NGINX itself was not rewritten. |

## 6. How large projects write agent instructions

**V, measured line counts at checked snapshots:** uv 49; Biome 88; Rolldown 107; Zed 190; Ruff 240; rust-lang/rust 265; Oxc 338; Deno CLAUDE 433 plus Copilot 352; Microsoft CLAUDE 62. Counts include blank lines and examples. Ruff's CLAUDE is a one-line include of AGENTS; Turbopack has 28 scoped lines and crates/AGENTS links back in five lines. These are descriptions, not evidence that a certain length produces better code.

- **V:** [uv][uv-agent] is mostly actionable imperatives with little introduction; it names recurring test and lockfile mistakes. **I:** best pattern for short shared rules.
- **V:** [Biome][biome-agent] routes detailed procedures to scoped skills; [Oxc][oxc-agent] routes work to crate instructions and names the right conformance harness. **I:** useful once Analyte actually has repeated machine/driver procedures; no empty skill catalog now.
- **V:** [Rolldown][roll-agent] maintains rationale/implementation docs; [Zed][zed-agent] instead warns that architecture maps in rules become stale, and admits new rules only if non-obvious, recurring and actionable. **I:** keep the plan/ADRs as architecture owners and let AGENTS link to them; record enduring traps, not a duplicate crate encyclopedia.
- **V:** [Rust AGENTS][rust-agent] operationalizes an existing human policy, including its limits; [Microsoft CLAUDE][ms-agent] prevents agents from inventing new ambiguous policy. **I:** don't let an agent quietly invent protocol meaning, approved evidence or contributor obligations.
- **V:** [Deno][deno-agent] supplies a long navigation/build/test handbook; [Copilot instructions][deno-copilot] overlap it. **I:** useful detail, but duplication costs maintenance. Keep one canonical Analyte file; any assistant-specific entrypoint should link to it.
- **V:** [Zed][zed-agent] requires a human-only README review marker; [Rust][rust-agent] requires prior reviewer agreement; [Bevy][bevy-ai] bans generated public prose; [Astral][astral-ai] and [ripgrep][ripgrep-ai] ban autonomous external submissions; [Servo][servo-ai] rejects generated contributions entirely. **I:** do not copy mechanical markers, blanket bans or compiler-specific reviewer gates without choosing that policy deliberately. Require real review and source evidence instead.
- **V:** [Oxc Copilot][oxc-copilot] and [Biome][biome-agent] demand evidence rather than plausible review claims. **I:** a useful agent rule names how to check compliance and what to report when evidence is missing.

**Top recommendations (I):** preserve the clinical contract; clarify panic and cancellation behavior; make internal state changes exhaustive; protect validated fields; require small, reviewed regression fixtures and isolated tests. Adopt narrow reasoned lint exceptions and targeted dependency updates. Keep AGENTS focused and automate its checkable rules. Add sophisticated tools only when their specific risk and cost justify them.

[api]: https://rust-lang.github.io/api-guidelines/checklist.html
[api-depend]: https://rust-lang.github.io/api-guidelines/dependability.html
[api-future]: https://rust-lang.github.io/api-guidelines/future-proofing.html
[style]: https://doc.rust-lang.org/stable/style-guide/
[rust-agent]: https://github.com/rust-lang/rust/blob/5f4effd57950/AGENTS.md
[rust-conventions]: https://rustc-dev-guide.rust-lang.org/conventions.html
[cargo-contrib]: https://github.com/rust-lang/cargo/blob/930d19d31c30/CONTRIBUTING.md
[cargo-tests]: https://github.com/rust-lang/cargo/blob/930d19d31c30/doc/contrib/src/tests/writing.md
[cargo-ai]: https://github.com/rust-lang/cargo/blob/930d19d31c30/doc/contrib/src/process/llm-usage.md
[cargo-clippy]: https://github.com/rust-lang/cargo/blob/930d19d31c30/clippy.toml
[cargo-deny]: https://github.com/rust-lang/cargo/blob/930d19d31c30/deny.toml
[cargo-ci]: https://github.com/rust-lang/cargo/blob/930d19d31c30/.github/workflows/main.yml
[cargo-release]: https://github.com/rust-lang/cargo/blob/930d19d31c30/doc/contrib/src/process/release.md
[ms-agent]: https://github.com/microsoft/rust-guidelines/blob/9585e288d21d/CLAUDE.md
[ms-unsafe]: https://github.com/microsoft/rust-guidelines/blob/9585e288d21d/src/guidelines/correctness/M-UNSAFE.md
[ms-panic]: https://github.com/microsoft/rust-guidelines/blob/9585e288d21d/src/guidelines/correctness/M-PANIC-ON-BUG.md
[ms-app-error]: https://github.com/microsoft/rust-guidelines/blob/9585e288d21d/src/guidelines/apps/M-APP-ERROR.md
[ms-expect]: https://github.com/microsoft/rust-guidelines/blob/9585e288d21d/src/guidelines/universal/M-LINT-OVERRIDE-EXPECT.md
[google]: https://google.github.io/styleguide/rustguide.html
[android]: https://source.android.com/docs/setup/build/rust/building-rust-modules/android-rust-modules
[course]: https://google.github.io/comprehensive-rust/unsafe-rust.html
[chrome]: https://chromium.googlesource.com/chromium/src/+/HEAD/docs/rust/unsafe.md
[firecracker]: https://github.com/firecracker-microvm/firecracker/blob/cc88013eadcf/CONTRIBUTING.md
[fc-cargo]: https://github.com/firecracker-microvm/firecracker/blob/cc88013eadcf/Cargo.toml
[fc-toolchain]: https://github.com/firecracker-microvm/firecracker/blob/cc88013eadcf/rust-toolchain.toml
[pingora-contrib]: https://github.com/cloudflare/pingora/blob/3f87e92c982e/.github/CONTRIBUTING.md
[pingora-ci]: https://github.com/cloudflare/pingora/blob/3f87e92c982e/.github/workflows/build.yml
[kernel]: https://docs.kernel.org/rust/coding-guidelines.html
[mozilla]: https://firefox-source-docs.mozilla.org/writing-rust-code/update-policy.html
[servo-style]: https://book.servo.org/contributing/style-guide.html
[servo-ai]: https://book.servo.org/policy/ai-usage.html
[servo-toolchain]: https://github.com/servo/servo/blob/c32ccc1a4edc/rust-toolchain.toml
[uv-agent]: https://github.com/astral-sh/uv/blob/d5c3f1744174/AGENTS.md
[uv-cargo]: https://github.com/astral-sh/uv/blob/d5c3f1744174/Cargo.toml
[uv-clippy]: https://github.com/astral-sh/uv/blob/d5c3f1744174/clippy.toml
[uv-deny]: https://github.com/astral-sh/uv/blob/d5c3f1744174/.cargo/deny.toml
[uv-toolchain]: https://github.com/astral-sh/uv/blob/d5c3f1744174/rust-toolchain.toml
[uv-ci]: https://github.com/astral-sh/uv/blob/d5c3f1744174/.github/workflows/test.yml
[astral-ai]: https://github.com/astral-sh/.github/blob/main/AI_POLICY.md
[ruff-agent]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/AGENTS.md
[ruff-contrib]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/CONTRIBUTING.md
[ruff-cargo]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/Cargo.toml
[ruff-toolchain]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/rust-toolchain.toml
[ruff-fuzz]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/.github/workflows/daily_fuzz.yaml
[zed-agent]: https://github.com/zed-industries/zed/blob/703b5ac3f4ff/AGENTS.md
[zed-clippy]: https://github.com/zed-industries/zed/blob/703b5ac3f4ff/clippy.toml
[zed-toolchain]: https://github.com/zed-industries/zed/blob/703b5ac3f4ff/rust-toolchain.toml
[deno-agent]: https://github.com/denoland/deno/blob/e7b39ebd55c8/CLAUDE.md
[deno-copilot]: https://github.com/denoland/deno/blob/e7b39ebd55c8/.github/copilot-instructions.md
[biome-agent]: https://github.com/biomejs/biome/blob/4cea5c6ab72e/AGENTS.md
[biome-cargo]: https://github.com/biomejs/biome/blob/4cea5c6ab72e/Cargo.toml
[biome-toolchain]: https://github.com/biomejs/biome/blob/4cea5c6ab72e/rust-toolchain.toml
[oxc-agent]: https://github.com/oxc-project/oxc/blob/6e31b456fcd5/AGENTS.md
[oxc-copilot]: https://github.com/oxc-project/oxc/blob/6e31b456fcd5/.github/copilot-instructions.md
[oxc-cargo]: https://github.com/oxc-project/oxc/blob/6e31b456fcd5/Cargo.toml
[oxc-toolchain]: https://github.com/oxc-project/oxc/blob/6e31b456fcd5/rust-toolchain.toml
[oxc-miri]: https://github.com/oxc-project/oxc/blob/6e31b456fcd5/.github/workflows/miri.yml
[tokio-ci]: https://github.com/tokio-rs/tokio/blob/f3e855fa55d6/.github/workflows/ci.yml
[tokio-loom]: https://github.com/tokio-rs/tokio/blob/f3e855fa55d6/.github/workflows/loom.yml
[tokio-policy]: https://github.com/tokio-rs/tokio/blob/f3e855fa55d6/CONTRIBUTING.md
[ripgrep-ai]: https://github.com/BurntSushi/ripgrep/blob/178cc027e283/AI_POLICY.md
[turbo-agent]: https://github.com/vercel/next.js/blob/ce93327ec721/turbopack/AGENTS.md
[roll-agent]: https://github.com/rolldown/rolldown/blob/b932fa3c2dca/AGENTS.md
[roll-toolchain]: https://github.com/rolldown/rolldown/blob/b932fa3c2dca/rust-toolchain.toml
[helix]: https://github.com/helix-editor/helix/blob/a714a818a3a4/docs/CONTRIBUTING.md
[helix-toolchain]: https://github.com/helix-editor/helix/blob/a714a818a3a4/rust-toolchain.toml
[bevy-ai]: https://bevy.org/learn/contribute/policies/ai/
[bevy-cargo]: https://github.com/bevyengine/bevy/blob/7ea4827e16d5/Cargo.toml
[bevy-deny]: https://github.com/bevyengine/bevy/blob/7ea4827e16d5/deny.toml
[clippy]: https://github.com/rust-lang/rust-clippy/blob/master/README.md
[scr]: https://github.com/Safety-Critical-Rust-Consortium/safety-critical-rust-coding-guidelines/tree/dc0c3e4f7489
[scr-site]: https://coding-guidelines.arewesafetycriticalyet.org/
[scr-style]: https://coding-guidelines.arewesafetycriticalyet.org/process/style-guideline.html
[ferrocene]: https://public-docs.ferrocene.dev/main/safety-manual/scope.html
[ferrocene-medical]: https://public-docs.ferrocene.dev/main/qualification/evaluation-plan/link-with-iec-62304-requirements.html
[embedded]: https://doc.rust-lang.org/embedded-book/start/panicking.html
[typescript]: https://devblogs.microsoft.com/typescript/announcing-typescript-native-previews/
[uutils]: https://github.com/uutils/coreutils/blob/main/README.md
[sudo]: https://github.com/trifectatechfoundation/sudo-rs/blob/main/README.md
[uv-readme]: https://github.com/astral-sh/uv/blob/d5c3f1744174/README.md
[ruff-readme]: https://github.com/astral-sh/ruff/blob/6bf61927fd32/README.md
[pingora-story]: https://blog.cloudflare.com/how-we-built-pingora-the-proxy-that-connects-cloudflare-to-the-internet/
