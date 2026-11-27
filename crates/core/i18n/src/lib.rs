//! Selects embedded Fluent catalogs and formats operator-facing messages.
#![forbid(unsafe_code)]

use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use fluent_syntax::ast::Entry;
use std::{collections::BTreeMap, future::Future, sync::LazyLock};
use unic_langid::LanguageIdentifier;

/// Formats external text and numbers as Fluent variables.
pub use fluent_bundle::FluentValue;

const ENGLISH: &[&str] = &[
    include_str!("../../../../locales/en/admin.ftl"),
    include_str!("../../../../locales/en/setup.ftl"),
    include_str!("../../../../locales/en/cli.ftl"),
    include_str!("../../../../locales/en/desktop.ftl"),
    include_str!("../../../../locales/en/doctor-overview.ftl"),
    include_str!("../../../../locales/en/dates.ftl"),
];
const ALBANIAN: &[&str] = &[
    include_str!("../../../../locales/sq/admin.ftl"),
    include_str!("../../../../locales/sq/setup.ftl"),
    include_str!("../../../../locales/sq/cli.ftl"),
    include_str!("../../../../locales/sq/desktop.ftl"),
    include_str!("../../../../locales/sq/doctor-overview.ftl"),
    include_str!("../../../../locales/sq/dates.ftl"),
];
const FRENCH: &[&str] = &[
    include_str!("../../../../locales/fr/admin.ftl"),
    include_str!("../../../../locales/fr/setup.ftl"),
    include_str!("../../../../locales/fr/cli.ftl"),
    include_str!("../../../../locales/fr/desktop.ftl"),
    include_str!("../../../../locales/fr/doctor-overview.ftl"),
    include_str!("../../../../locales/fr/dates.ftl"),
];

struct Catalog {
    tag: &'static str,
    name: &'static str,
    bundle: FluentBundle<FluentResource>,
    labels: BTreeMap<String, String>,
}

impl Catalog {
    #[expect(
        clippy::expect_used,
        reason = "the embedded catalog and tags are verified by tests"
    )]
    fn new(tag: &'static str, name: &'static str, sources: &[&str]) -> Self {
        let language = tag.parse().expect("registered language tags must be valid");
        let mut bundle = FluentBundle::new_concurrent(vec![language]);
        // English output already has established byte-for-byte representations.
        bundle.set_use_isolating(false);
        let mut ids = Vec::new();
        for source in sources {
            let resource = FluentResource::try_new((*source).to_owned())
                .expect("embedded Fluent catalogs must parse");
            ids.extend(resource.entries().filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_owned()),
                Entry::Term(_)
                | Entry::Comment(_)
                | Entry::GroupComment(_)
                | Entry::ResourceComment(_)
                | Entry::Junk { .. } => None,
            }));
            bundle
                .add_resource(resource)
                .expect("embedded message IDs must be unique");
        }
        let labels = ids
            .into_iter()
            .filter_map(|id| {
                let pattern = bundle.get_message(&id)?.value()?;
                let mut errors = Vec::new();
                let value = bundle.format_pattern(pattern, None, &mut errors);
                errors.is_empty().then(|| (id, value.into_owned()))
            })
            .collect();
        Self {
            tag,
            name,
            bundle,
            labels,
        }
    }

    fn format(&self, id: &str, args: &FluentArgs<'_>) -> Option<String> {
        let pattern = self.bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        let value = self.bundle.format_pattern(pattern, Some(args), &mut errors);
        errors.is_empty().then(|| value.into_owned())
    }

    fn label(&self, id: &'static str) -> &str {
        self.labels
            .get(id)
            .or_else(|| SOURCE.labels.get(id))
            .map_or(id, String::as_str)
    }

    fn message(&self, id: &str, args: &FluentArgs<'_>) -> String {
        self.format(id, args)
            .or_else(|| SOURCE.format(id, args))
            .unwrap_or_else(|| id.to_owned())
    }
}

static SOURCE: LazyLock<Catalog> = LazyLock::new(|| Catalog::new("en", "English", ENGLISH));
static TRANSLATIONS: LazyLock<Vec<Catalog>> = LazyLock::new(|| {
    let embedded: &[(&str, &str, &[&str])] =
        &[("sq", "Shqip", ALBANIAN), ("fr", "Français", FRENCH)];
    embedded
        .iter()
        .map(|(tag, name, sources)| Catalog::new(tag, name, sources))
        .collect()
});

tokio::task_local! {
    static CURRENT: Language;
}

/// An available language selected once per request or command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Language(Option<usize>);

impl Language {
    /// Negotiates ordered BCP 47 preferences against the installed catalogs.
    #[must_use]
    pub fn select<'a>(preferences: impl IntoIterator<Item = &'a str>) -> Self {
        Self::negotiate(preferences).unwrap_or_default()
    }

    /// Finds an installed catalog for the ordered preferences, if any match.
    #[must_use]
    pub fn negotiate<'a>(preferences: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let requested: Vec<LanguageIdentifier> = preferences
            .into_iter()
            .filter_map(|tag| tag.parse().ok())
            .collect();
        let available: Vec<&LanguageIdentifier> = std::iter::once(&*SOURCE)
            .chain(TRANSLATIONS.iter())
            .flat_map(|catalog| catalog.bundle.locales.first())
            .collect();
        let ordered =
            negotiate_languages(&requested, &available, None, NegotiationStrategy::Lookup);
        ordered
            .first()
            .and_then(|found| available.iter().position(|tag| tag == *found))
            .map(|index| Self(index.checked_sub(1)))
    }

    /// Returns the languages embedded in both programs, with English first.
    pub fn installed() -> impl Iterator<Item = Self> {
        std::iter::once(Self::default())
            .chain((0..TRANSLATIONS.len()).map(|index| Self(Some(index))))
    }

    /// Selects an explicit CLI tag or the first nonempty POSIX locale variable.
    #[must_use]
    pub fn command_line(explicit: Option<&str>) -> Self {
        let from_environment = ["LC_ALL", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()));
        let tag = explicit.or(from_environment.as_deref()).unwrap_or_default();
        let tag = tag
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .replace('_', "-");
        Self::select([tag.as_str()])
    }

    /// Returns the canonical BCP 47 tag for the selected catalog.
    #[must_use]
    pub fn tag(self) -> &'static str {
        self.catalog().tag
    }

    /// Returns the language's name as written by its speakers.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.catalog().name
    }

    fn catalog(self) -> &'static Catalog {
        self.0
            .and_then(|index| TRANSLATIONS.get(index))
            .unwrap_or(&SOURCE)
    }

    /// Runs a future with the language scoped to its polls, including cancellation.
    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        CURRENT.scope(self, future).await
    }

    /// Runs synchronous rendering with the language scoped to the call.
    pub fn sync_scope<T>(self, render: impl FnOnce() -> T) -> T {
        CURRENT.sync_scope(self, render)
    }

    /// Reads the request's or command's language, defaulting to English.
    #[must_use]
    pub fn current() -> Self {
        CURRENT.try_with(|language| *language).unwrap_or_default()
    }
}

/// Returns an embedded label, or the message ID if its English source is broken.
#[must_use]
pub fn text(id: &'static str) -> &'static str {
    Language::current().catalog().label(id)
}

/// Formats an embedded message with named arguments, showing the message ID if
/// its English source cannot be formatted.
pub fn message<'a>(
    id: &str,
    values: impl IntoIterator<Item = (&'a str, FluentValue<'a>)>,
) -> String {
    let args = values.into_iter().collect();
    Language::current().catalog().message(id, &args)
}

/// Formats a plural message with a numeric selector and the exact count.
/// Fluent numbers are floating-point. The separately rendered number keeps
/// every digit even when a count exceeds the exact integer range.
pub fn count(id: &str, count: impl Into<u64>) -> String {
    let count = count.into();
    message(
        id,
        [
            ("count", count.into()),
            ("number", count.to_string().into()),
        ],
    )
}

/// The full weekday name in the current language; Monday is 1, as in ISO 8601.
#[must_use]
pub fn weekday(day: i8) -> String {
    message("date-weekday", [("day", day.into())])
}

/// The full month name in the current language; January is 1.
#[must_use]
pub fn month(month: i8) -> String {
    message("date-month", [("month", month.into())])
}

/// The short month name in the current language, such as "Nov"; January is 1.
#[must_use]
pub fn month_short(month: i8) -> String {
    message("date-month-short", [("month", month.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_parse_and_english_is_the_default() {
        assert_eq!(Language::select(["it-IT", "en-GB"]).tag(), "en");
        assert_eq!(Language::select(["und"]).tag(), "en");
        assert_eq!(text("admin-language"), "Language");
    }

    #[test]
    fn calendar_names_follow_iso_numbering() {
        assert_eq!(weekday(1), "Monday");
        assert_eq!(weekday(7), "Sunday");
        assert_eq!(month(1), "January");
        assert_eq!(month(12), "December");
        assert_eq!(month_short(11), "Nov");
        Language::select(["sq"]).sync_scope(|| {
            assert_eq!(weekday(1), "e hënë");
            assert_eq!(month(11), "nëntor");
        });
    }

    #[test]
    fn albanian_matches_regional_tags_and_formats_one_and_other() {
        for tag in ["sq", "sq-AL", "sq-XK"] {
            let language = Language::select([tag, "en"]);
            assert_eq!(language.tag(), "sq");
            language.sync_scope(|| {
                assert_eq!(text("admin-language"), "Gjuha");
                assert_eq!(count("admin-count-results", 1_u64), "1 rezultat");
                assert_eq!(count("admin-count-results", 2_u64), "2 rezultate");
            });
        }
    }

    // Unicode CLDR plural rules: French uses one for 0 and 1.
    #[test]
    fn french_matches_regional_tags_and_formats_one_and_other() {
        for tag in ["fr", "fr-FR", "fr-CH", "fr-CA"] {
            let language = Language::select([tag, "en"]);
            assert_eq!(language.tag(), "fr");
            language.sync_scope(|| {
                assert_eq!(text("admin-language"), "Langue");
                assert_eq!(count("admin-count-results", 0_u64), "0 résultat");
                assert_eq!(count("admin-count-results", 1_u64), "1 résultat");
                assert_eq!(count("admin-count-results", 2_u64), "2 résultats");
                assert_eq!(weekday(1), "lundi");
                assert_eq!(month(11), "novembre");
            });
        }
    }

    #[test]
    fn an_incomplete_albanian_catalog_uses_english_for_missing_messages() {
        let catalog = Catalog::new("sq", "Shqip", &["admin-language = Gjuha"]);
        assert_eq!(catalog.label("admin-language"), "Gjuha");
        assert_eq!(catalog.label("admin-settings"), "Settings");
        let mut args = FluentArgs::new();
        args.set("time", "08:30");
        assert_eq!(catalog.message("admin-live-at", &args), "Live · 08:30");
    }

    #[test]
    fn variables_keep_external_text_verbatim() {
        let data = "000a <0.1 {code} & µmol/L";
        assert_eq!(
            message("admin-live-at", [("time", data.into())]),
            format!("Live · {data}")
        );
    }
}
