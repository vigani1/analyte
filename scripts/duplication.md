# Duplication checks

CI runs `python3 scripts/test-duplication.py` and
`python3 scripts/check-duplication.py`, using pinned jscpd 5.4.2 with eight-line
and 70-token minimum clones. Python uses only its standard library.

The script stages separate temporary production and test trees. Test files,
inline `cfg(test)` modules, individual test functions and test-only imports
belong to the test tree. It follows test-only out-of-line modules. Comments,
character literals and raw/escaped strings cannot end a module early. Code
following a test module stays in production. Feature-enabled hooks stay in
production; `cfg(all(test, feature = "test-hooks"))` remains test-only.
Staged files are measurement inputs and never replace repository sources.
Removed code contributes no padding lines to either denominator.

Production has a 0.2% ceiling, just above the 0.16% of lines and tokens
measured on 2026-11-18 (113 of 72,004 lines). Only the six frozen contract v1
paths in `duplication-exclusions.txt` are exempt, for the reasons in ADR 0008.
A drop caused by those exclusions is not removal of the underlying deliberate
copies. Tests have a separate 2.95% ceiling; on 2026-11-18 they measured 2.85%
of lines and 2.81% of tokens, too close to lower it. Never raise a ceiling to
make a change pass.
