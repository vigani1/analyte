//! Holds what every list page shares: the page size, the cursor in its address, one page out of a store read, and what an empty list says.
use serde::Deserialize;

pub(in crate::admin) const PAGE: usize = 50;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Cursor {
    #[serde(default, deserialize_with = "cursor_value")]
    pub(in crate::admin) after: u64,
}

pub(in crate::admin) fn cursor_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    i64::try_from(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

/// Splits one store read into the rows a page shows and whether more follow:
/// at most `shown` rows are kept, and more follow when the read held more than
/// that or when `rest` finds a row after the last one kept. An empty read has
/// nothing after it.
pub(in crate::admin) fn one_page<T, E>(
    mut rows: Vec<T>,
    shown: usize,
    rest: impl FnOnce(&T) -> Result<bool, E>,
) -> Result<(Vec<T>, bool), E> {
    if rows.len() > shown {
        rows.truncate(shown);
        return Ok((rows, true));
    }
    let more = match rows.last() {
        Some(last) => rest(last)?,
        None => false,
    };
    Ok((rows, more))
}

/// What an empty list says and where to go instead: a list that has rows,
/// or what to do first. `good` marks good news, such as nothing to review,
/// which shows a tick; an empty list is not good news by itself.
pub(in crate::admin) struct Empty {
    pub(in crate::admin) good: bool,
    pub(in crate::admin) title: &'static str,
    pub(in crate::admin) text: &'static str,
    pub(in crate::admin) link: Option<(&'static str, String)>,
}
