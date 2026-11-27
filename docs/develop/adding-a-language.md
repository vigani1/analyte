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

See [ADR 0039](../adr/0039-fluent-for-operator-interfaces.md) for the
language selection rules.
