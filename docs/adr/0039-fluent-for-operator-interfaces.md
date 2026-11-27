# 0039. Fluent for operator interfaces

Status: accepted, 2026-11-27.

## Decision

Use Mozilla Fluent for the admin pages, Setup, the command line and the
desktop app. English is the source language and the fallback for every
message. Ship English first. Add Albanian, then German, French and Spanish
in later pull requests.

Keep catalogs in `locales/<tag>/` by area. Build them into the programs so an
installation needs no translation files. All labs use the same catalogs.
Preserve the English output and the existing message ownership. Give Fluent
whole sentences and named variables so a translation can reorder variables
and use the language's plural rules.

Admin Settings saves the user's preference in their browser profile. If
there is no supported saved choice, use the browser's weighted
`Accept-Language` preferences, then English. Show the preference control
when a second language ships. The CLI uses `--lang` when provided, then
`LC_ALL` or `LANG`, and otherwise English. It passes that tag to the local
engine when the engine formats a reply. Scope the language to the request
or command.

Machine and lab-app data travels verbatim. Names, test codes, sample IDs,
values, units and other data received from a machine or lab app can fill a
Fluent variable, with the usual output escaping. Wire values and audit
identities remain stable across locales.

## Verification

The repository check compares production message uses with the English
catalog and reports coverage for every language. Embedded catalogs are
parsed by Fluent. [Adding a language](../develop/adding-a-language.md)
describes the review status and how to register an additional catalog.
