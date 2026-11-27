# Adding a language

1. Copy `locales/en/` to `locales/<tag>/` using the language's BCP 47 tag.
   Keep the file names, message IDs and variable names. In the header of
   each file, record the review status, for example
   `### Machine-translated, needs native review.`
2. Translate whole messages. Use Fluent select expressions for plurals.
   Keep commands and literal machine or lab-app terms as written. Preserve
   any formatting the message needs and keep markup in the templates.
   A message ending in `-in-line` is the label above it written inside a
   sentence; give it the capitals your language uses there. Code never
   changes the case of a translated label.
3. Register the new files and tag in
   [the embedded catalog list](../../crates/core/i18n/src/lib.rs).
   A missing translation uses the English message.
4. Run `bash scripts/check-layout.sh` to see coverage and check message IDs.
   Run the tests and review the language in the admin pages, desktop app
   and with `analyte --lang <tag> overview`. Mark native review once a
   fluent speaker has checked the messages in context.

## What never goes in a catalog

A catalog holds wording for people, and a translator may change any of it.
Never put paths, folder or file names, commands or their arguments,
environment variable names, HTTP header names or values, protocol tokens or
any other machine-readable word in a catalog; write them as literals in code.
The Albanian catalog once translated `System32`, and the desktop app could
not start the engine in Albanian.

`scripts/check-catalog-sinks.py` (run by `check-layout.sh`) fails when a
catalog lookup, or a `let` variable holding one in the same function, is
passed to `Path::new`, a path's `join` or `push`, `Command::new`, `.arg`,
`.args`, an environment variable call, an HTTP header, a file name setter or
a `std::fs` call. It reads source text, not types, so a value passed through a
struct field or a function return is not followed; review those by hand.

Code also never passes an English word into a sentence as a variable. When
the wording depends on a state, pass a fixed identifier and choose the words
in the catalog with a Fluent selector (`{ $state -> [added] … *[saved] … }`),
or use separate messages. Never change a message's case or spelling in code;
give each form its own message.

See [ADR 0039](../adr/0039-fluent-for-operator-interfaces.md) for the
language selection rules.
