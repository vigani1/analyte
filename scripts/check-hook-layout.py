#!/usr/bin/env python3
"""Keep feature selection and synthetic hook state in the owning hook modules."""
import importlib.util
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('duplication', ROOT / 'scripts/check-duplication.py')
duplication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(duplication)


def main():
    failures = []
    callers = 0
    for owner in [ROOT / 'crates/analyte/src', ROOT / 'crates/steps/sink/src']:
        for path in owner.rglob('*.rs'):
            # Owners contain all feature branches; callers use one facade/import.
            relative = path.relative_to(ROOT)
            hook_owners = ('crates/analyte/src/installation/config/hooks/',
                           'crates/analyte/src/machine/hooks/',
                           'crates/analyte/src/runtime/hooks/',
                           'crates/analyte/src/runtime/test_hooks/')
            if relative.as_posix().startswith(hook_owners) or relative.as_posix() == 'crates/steps/sink/src/hooks.rs':
                continue
            production, _ = duplication.split(path.read_text(encoding='utf-8'))
            sites = re.findall(r'#\[cfg\([^\]]*"test-hooks"[^\]]*\)\]', production)
            relative = path.relative_to(ROOT)
            if len(sites) > 1:
                failures.append(f'{relative}: {len(sites)} feature hook entries (maximum one)')
            if re.search(r'\.hooks\.(?:law_gates|autolyser_quiescent|input_processed|law_precommit|law_committed|astm_committed|connection_closing|loopback_destinations|stall_store_on_shutdown)\b', production):
                failures.append(f'{relative}: synthetic state accessed outside its hook owner')
            if sites:
                callers += 1
    if failures:
        raise SystemExit('\n'.join(failures))
    print(f'Hook ownership passed: maximum one feature entry per production caller ({callers} entries).')


if __name__ == '__main__':
    main()
