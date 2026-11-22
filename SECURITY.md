# Security policy

Analyte moves clinical lab results. A security bug can change a result in a
patient report, so we treat it as a safety problem, not only a technical one.

## Report a vulnerability privately

Use GitHub's private reporting: open the repository's **Security** tab and
choose **Report a vulnerability**
([direct link](https://github.com/vigani1/analyte/security/advisories/new)).
Only the maintainer sees the report.

Please do not open a public issue, pull request or discussion for a
vulnerability.

**Never include patient data or real captures in a report.** Use synthetic
messages, made-up sample IDs and test credentials. If a real capture is the
only way to show the problem, describe it in the report and we will agree a
safe way to share a de-identified version.

## What counts

Anything that could:

- change, lose, duplicate or misattribute a result, or attach it to the wrong
  sample or patient;
- make Analyte tell a machine "got it" for data it has not stored;
- send or resend an order the lab did not ask for;
- leak patient data or raw messages outside the protected store, for example
  into ordinary logs;
- leak or weaken credentials, signing keys or HTTPS settings;
- bypass sign-in or reach the admin pages without permission;
- let a device or another program on the network inject or alter data.

Not sure? Report it anyway. Ordinary bugs with no safety or security effect
belong in a normal [issue](https://github.com/vigani1/analyte/issues/new/choose).

## What to expect

- We confirm the report within 7 days.
- We tell you what we found and how we plan to fix it, and keep you updated.
- We fix it in private, publish a release and then a GitHub security advisory
  that credits you, unless you prefer not to be named.
- Please give us a fair chance to release a fix before you talk about the
  problem in public.

## CI and GitHub Actions safety

Workflows get a read-only token by default; only the release job may write,
and only on a release tag. No workflow uses `pull_request_target`, so code
from a fork never runs with the repository's secrets or write access. Every
action is pinned to a full commit SHA, which a repository setting enforces,
and checkouts do not keep credentials. Workflow runs from outside
contributors wait for the maintainer's approval. Release tags (`v*`) are
protected by a ruleset, so only the maintainer can create, move or delete
them. Each release stays a draft, with checksums and build provenance, until
the maintainer has checked it and published it ([releasing](docs/develop/release.md)).

## Supported versions

Analyte has no release yet. Once it does, fixes go to `main` and the latest
release only. Older releases do not receive fixes; please upgrade.
