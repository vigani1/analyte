#!/usr/bin/env bash
# Enforce ADR 0012 without installing dependencies.
set -euo pipefail
failed=0
while IFS= read -r -d '' file; do
  if [[ "$file" == crates/* || "$file" == fuzz/* ]] && ! head -n 1 "$file" | grep -q '^//!'; then
    echo "$file: missing opening //! header"; failed=1
  fi
  lines=$(awk 'END { print NR }' "$file")
  if [ "$lines" -gt 800 ]; then
    echo "$file: $lines lines exceeds 800"; failed=1
  elif [ "$lines" -gt 600 ] && ! awk 'substr($0,1,3)=="//!" { if ($0 ~ /Kept whole:.*[[:alnum:]]/) found=1; next } /^$/ { next } { exit } END { exit !found }' "$file"; then
    echo "$file: requires a //! Kept whole: header reason"; failed=1
  fi
  if [ -d "${file%.rs}" ]; then
    echo "$file: exists beside ${file%.rs}/"; failed=1
  fi
done < <(git ls-files -z -- '*.rs')
while IFS= read -r -d '' manifest; do
  directory=${manifest%/Cargo.toml}
  case "$directory" in
    crates/analyte|crates/steps/*|crates/protocols/*|crates/core/*|crates/testing/*)
      case "$directory" in crates/*/*/*) echo "$directory: nested crate"; failed=1 ;; esac ;;
    *) echo "$directory: crate outside approved kind folders"; failed=1 ;;
  esac
done < <(find crates -name target -prune -o -name Cargo.toml -print0)
if [ "$failed" -ne 0 ]; then exit 1; fi
echo 'Layout passed'
