//! Adds and removes `[[machines]]` entries and old `[[retired_destinations]]` tables in the program file text, keeping every other byte.
//!
//! Each edit is checked by parsing: the edited file must hold exactly the old
//! entries plus or minus the one changed. A layout the edit cannot follow (an
//! inline `machines = [...]` array) is refused with a message, never guessed at.
use super::{SetupError, site::EntrySpans};
use crate::installation::config::plain_name;
use analyte_model::Identifier;

/// Paths the setup page itself uses under `/setup/`; a machine cannot take them.
const RESERVED: [&str; 5] = ["add", "first", "lab-apps", "listen", "restart"];

/// Checks a new machine's name. It is an [`Identifier`], and because it also
/// names the machine's site file, it uses only ASCII letters, digits, `-` and `_`.
pub(super) fn name(text: &str) -> Result<Identifier, SetupError> {
    let id = Identifier::new(text.trim()).map_err(SetupError::InvalidName)?;
    if !plain_name(id.as_str()) {
        return Err(SetupError::NameCharacters);
    }
    if RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(id.as_str()))
    {
        return Err(SetupError::ReservedName(id.as_str().into()));
    }
    Ok(id)
}

/// The site file a machine added on the page gets, beside the program file.
pub(super) fn site_file(id: &Identifier) -> String {
    format!("{}.site.toml", id.as_str())
}

/// The installers' starter site settings for a new machine: the retention only;
/// no connection and no lab app, which the setup page chooses.
pub(super) fn starter_site(id: &Identifier) -> String {
    format!(
        concat!(
            "# Starter site settings for {}, written by the Analyte setup page. The setup\n",
            "# page fills in the connection and picks the lab app, which is set once on\n",
            "# the Lab apps page.\n",
            "schema_version = 1\n",
            "# Keep every result until the lab sets a retention period (ADR 0013).\n",
            "retention = \"keep\"\n"
        ),
        id.as_str()
    )
}

/// The first `machine-N` not listed, compared without case as file names are on Windows.
pub(super) fn suggestion(listed: &[EntrySpans]) -> String {
    (1_u32..)
        .map(|n| format!("machine-{n}"))
        .find(|name| !taken(listed, name))
        .unwrap_or_default()
}

/// A listed machine already has this name, ignoring case.
pub(super) fn taken(listed: &[EntrySpans], name: &str) -> bool {
    listed
        .iter()
        .any(|entry| entry.id.get_ref().eq_ignore_ascii_case(name))
}

/// The program text with one more entry, after the last `[[machines]]` entry, or
/// at the end when there is none.
pub(super) fn added(
    program: &str,
    listed: &[EntrySpans],
    id: &Identifier,
    site: &str,
) -> Result<String, SetupError> {
    let layout = || MACHINES.refused();
    let at = match listed.last() {
        Some(last) => {
            span(program, last.id.span().start, MACHINES.header)
                .ok_or_else(layout)?
                .end
        }
        None => program.len(),
    };
    let (before, after) = (
        program.get(..at).ok_or_else(layout)?,
        program.get(at..).ok_or_else(layout)?,
    );
    let gap = if before.is_empty() || before.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let entry = format!(
        "{gap}\n[[machines]]\nid = {}\nsite = {}\n",
        toml::Value::String(id.as_str().into()),
        toml::Value::String(site.into())
    );
    let edited = format!("{before}{entry}{after}");
    let mut expected = parsed(program)?;
    let mut table = toml::Table::new();
    table.insert("id".into(), toml::Value::String(id.as_str().into()));
    table.insert("site".into(), toml::Value::String(site.into()));
    let toml::Value::Array(machines) = expected
        .entry(MACHINES.key)
        .or_insert_with(|| toml::Value::Array(Vec::new()))
    else {
        return Err(layout());
    };
    machines.push(toml::Value::Table(table));
    MACHINES.check(&edited, &expected)?;
    Ok(edited)
}

/// The program text without the entry of machine `id`; comments after the entry stay.
pub(super) fn removed(
    program: &str,
    listed: &[EntrySpans],
    id: &str,
) -> Result<String, SetupError> {
    let entry = listed
        .iter()
        .find(|entry| entry.id.get_ref() == id)
        .ok_or_else(|| SetupError::UnknownMachine(id.into()))?;
    MACHINES.cut(program, entry.id.span().start, id)
}

/// The names of the kept destinations as written, with their byte positions.
#[derive(serde::Deserialize)]
struct RetiredSpans {
    #[serde(default)]
    retired_destinations: Vec<RetiredName>,
}

#[derive(serde::Deserialize)]
struct RetiredName {
    name: toml::Spanned<String>,
}

/// The program text without the old `[[retired_destinations]]` table called
/// `name`, now that `lab-apps.toml` holds that lab app; every other byte stays.
/// Text without such a table is returned unchanged.
pub(super) fn without_retired(program: &str, name: &str) -> Result<String, SetupError> {
    let spans: RetiredSpans = toml::from_str(program).map_err(SetupError::ProgramFile)?;
    match spans
        .retired_destinations
        .iter()
        .find(|entry| entry.name.get_ref() == name)
    {
        Some(entry) => RETIRED.cut(program, entry.name.span().start, name),
        None => Ok(program.to_owned()),
    }
}

/// One kind of array-of-tables entry the page edits in the program file.
struct Tables {
    /// The header line of one entry.
    header: &'static str,
    /// The array's key.
    key: &'static str,
    /// The setting that names one entry.
    field: &'static str,
    /// How to write the entries when the text cannot be edited in place.
    layout: &'static str,
}

/// One machine entry.
const MACHINES: Tables = Tables {
    header: "[[machines]]",
    key: "machines",
    field: "id",
    layout: "write each machine as its own [[machines]] table so setup can add or remove machines",
};

/// One destination kept after its machine was removed.
const RETIRED: Tables = Tables {
    header: "[[retired_destinations]]",
    key: "retired_destinations",
    field: "name",
    layout: "write each kept lab app destination as its own [[retired_destinations]] table",
};

impl Tables {
    fn refused(&self) -> SetupError {
        SetupError::ProgramLayout(self.layout)
    }

    /// The program text without the entry called `name`, whose naming value
    /// starts at byte `at`; the key goes when no entry is left.
    fn cut(&self, program: &str, at: usize, name: &str) -> Result<String, SetupError> {
        let layout = || self.refused();
        let range = span(program, at, self.header).ok_or_else(layout)?;
        let edited = format!(
            "{}{}",
            program.get(..range.start).ok_or_else(layout)?,
            program.get(range.end..).ok_or_else(layout)?
        );
        let mut expected = parsed(program)?;
        if let Some(toml::Value::Array(listed)) = expected.get_mut(self.key) {
            listed
                .retain(|entry| entry.get(self.field).and_then(toml::Value::as_str) != Some(name));
            if listed.is_empty() {
                expected.remove(self.key);
            }
        }
        self.check(&edited, &expected)?;
        Ok(edited)
    }

    /// The edit holds exactly the expected tables; anything else means the text
    /// was laid out in a way the edit did not follow.
    fn check(&self, edited: &str, expected: &toml::Table) -> Result<(), SetupError> {
        if parsed(edited).ok().as_ref() == Some(expected) {
            Ok(())
        } else {
            Err(self.refused())
        }
    }
}

fn parsed(text: &str) -> Result<toml::Table, SetupError> {
    text.parse().map_err(SetupError::ProgramFile)
}

/// Lines of `text` with their byte ranges, newline included.
fn lines(text: &str) -> impl Iterator<Item = (std::ops::Range<usize>, &str)> {
    text.split_inclusive('\n').scan(0_usize, |start, line| {
        let begin = *start;
        *start = begin.saturating_add(line.len());
        Some((begin..*start, line))
    })
}

/// From the `header` line (such as `[[machines]]`) above the entry whose value
/// starts at `id` to the end of its last setting line, before the next table
/// header; `None` when the text is not laid out that way.
fn span(text: &str, id: usize, header: &str) -> Option<std::ops::Range<usize>> {
    let header = |line: &str| {
        let content = line.split('#').next().unwrap_or_default().trim();
        content == header
    };
    let start = lines(text)
        .take_while(|(range, _)| range.start <= id)
        .filter(|(_, line)| header(line))
        .last()
        .map(|(range, _)| range.start)?;
    let mut end = start;
    for (range, line) in lines(text)
        .skip_while(|(range, _)| range.end <= start)
        .skip(1)
    {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            break;
        }
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            end = range.end;
        }
    }
    (end > id).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(text: &str) -> Result<Vec<EntrySpans>, crate::Error> {
        Ok(toml::from_str::<super::super::site::ProgramSpans>(text)?.machines)
    }

    // Owner request 2026-11-18 (add and remove machines): an entry is added after
    // the last machine and removed without touching comments or other tables.
    #[test]
    fn entries_are_added_and_removed_in_place() -> Result<(), crate::Error> {
        let program = concat!(
            "# keep me\nschema_version = 1\n[admin]\nport = 8080\n\n",
            "[[machines]]\nid = 'a'\nsite = 'a.toml' # a\n\n# about b\n",
            "[[machines]]\nid = \"b\"\ndefinition = 'x.toml'\nsite = 'b.toml'\n\n",
            "# alerts follow\n[[alerts]]\nkind = 'restore_paused'\nresponder = 'duty'\n"
        );
        let id = name("machine-3")?;
        let with = added(program, &listed(program)?, &id, &site_file(&id))?;
        assert!(
            with.contains("site = 'b.toml'\n\n[[machines]]\nid = \"machine-3\"\nsite = \"machine-3.site.toml\"\n\n# alerts follow\n"),
            "{with}"
        );
        let without = removed(&with, &listed(&with)?, "a")?;
        assert!(without.starts_with("# keep me\nschema_version = 1\n[admin]\nport = 8080\n\n\n# about b\n[[machines]]\nid = \"b\""), "{without}");
        assert!(without.ends_with(
            "# alerts follow\n[[alerts]]\nkind = 'restore_paused'\nresponder = 'duty'\n"
        ));
        let mut rest = removed(&without, &listed(&without)?, "b")?;
        rest = removed(&rest, &listed(&rest)?, "machine-3")?;
        assert!(listed(&rest)?.is_empty());
        assert!(!rest.contains("[[machines]]"));
        let again = added(&rest, &listed(&rest)?, &id, &site_file(&id))?;
        assert_eq!(listed(&again)?.len(), 1);
        // An inline array cannot be edited line by line: refuse, never guess.
        let inline = "machines = [{ id = 'a', site = 'a.toml' }]\n";
        assert!(removed(inline, &listed(inline)?, "a").is_err());
        assert!(added(inline, &listed(inline)?, &id, "x.toml").is_err());
        Ok(())
    }

    // Owner request 2026-11-19 (lab app set once), item 3: an old kept destination
    // whose lab app moved to lab-apps.toml leaves the program file without
    // touching other bytes; a name with no table changes nothing.
    #[test]
    fn retired_table_is_removed_in_place() -> Result<(), crate::Error> {
        let program = concat!(
            "schema_version = 1\n# about a\n[[machines]]\nid = 'a'\nsite = 'a.toml'\n\n",
            "[[retired_destinations]]\n# Kept by the setup page\nname = 'demo'\n",
            "base_url = 'https://lab.example.invalid'\napi_key_name = 'demo-api'\n",
            "signing_key_name = 'demo-signing'\n\n[[retired_destinations]]\nname = 'old'\n",
            "base_url = 'https://old.example.invalid'\napi_key_name = 'old-api'\n",
            "signing_key_name = 'old-signing'\n"
        );
        let without = without_retired(program, "demo")?;
        assert!(
            without.starts_with(
                "schema_version = 1\n# about a\n[[machines]]\nid = 'a'\nsite = 'a.toml'\n\n\n[[retired_destinations]]\nname = 'old'"
            ),
            "{without}"
        );
        assert!(!without.contains("demo"), "{without}");
        assert_eq!(without_retired(&without, "demo")?, without);
        let none = without_retired(&without, "old")?;
        assert!(!none.contains("retired_destinations"), "{none}");
        assert!(none.starts_with("schema_version = 1\n# about a\n[[machines]]"));
        Ok(())
    }

    // Owner request 2026-11-18 (add machines): the name names the site file, so
    // it is one plain file-name part, and the setup page's own paths are refused.
    #[test]
    fn machine_names_are_plain_file_names() {
        for good in ["machine-2", "cobas_e411", "B2"] {
            assert!(name(good).is_ok(), "{good}");
        }
        for bad in ["", "two words", "../up", "a.b", "listen", "ADD", "é"] {
            assert!(name(bad).is_err(), "{bad}");
        }
    }
}
