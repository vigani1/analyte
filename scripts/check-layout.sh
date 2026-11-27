#!/usr/bin/env bash
# Runs the repository checks that rustfmt, Clippy, cargo-deny and the tests
# cannot do, using only Bash and the Python standard library.
set -euo pipefail
python3 scripts/test-code-page.py
python3 scripts/check-code-page.py
python3 scripts/test-layout-names.py
python3 scripts/check-layout-names.py
python3 scripts/test-protocol-tests.py
python3 scripts/check-protocol-tests.py
python3 scripts/check-fuzz-targets.py
python3 scripts/check-locales.py
python3 scripts/test-catalog-sinks.py
python3 scripts/check-catalog-sinks.py
python3 scripts/test-links.py
python3 scripts/check-links.py
failed=0
while IFS= read -r -d '' file; do
  if [[ "$file" == crates/* || "$file" == fuzz/* ]] && ! head -n 1 "$file" | grep -q '^//!'; then
    echo "$file: missing opening //! header"; failed=1
  fi
  if [ -d "${file%.rs}" ]; then
    echo "$file: exists beside ${file%.rs}/"; failed=1
  fi
done < <(git ls-files -z -- '*.rs')
while IFS= read -r -d '' manifest; do
  directory=${manifest%/Cargo.toml}
  case "$directory" in
    crates/analyte|crates/app|crates/steps/*|crates/protocols/*|crates/core/*|crates/testing/*)
      case "$directory" in crates/*/*/*) echo "$directory: nested crate"; failed=1 ;; esac ;;
    *) echo "$directory: crate outside approved kind folders"; failed=1 ;;
  esac
done < <(find crates -name target -prune -o -name Cargo.toml -print0)
if [ "$failed" -ne 0 ]; then exit 1; fi
echo 'Layout passed'
