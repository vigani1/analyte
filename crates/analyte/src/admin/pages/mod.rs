//! Renders bounded operator pages and the helpers they share: paging, the store-failure page and the restore pause notice.
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::server::Context;
use crate::deadline;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
pub(super) mod awos;
mod deliveries;
pub(super) use deliveries::deliveries;
mod orders;
pub(super) use orders::orders;
pub(super) mod retention;
use crate::admin::view::{self, format};

pub(super) const PAGE: usize = 50;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    #[serde(default, deserialize_with = "cursor_value")]
    pub(super) after: u64,
}

pub(super) fn cursor_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    i64::try_from(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub(super) fn finish(result: Result<Markup, Error>) -> Response {
    match result {
        Ok(html) => Html(html.markup(view::END)).into_response(),
        Err(error) => view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "This page could not be shown",
            concat!(
                "Analyte could not read its stored data just now. Reload the page; if this ",
                "keeps happening, give the details below to support.",
            ),
            error,
        ),
    }
}

/// The "Next page" link; call it only when rows exist after `after`.
pub(super) fn next(html: &mut Markup, route: &str, after: impl std::fmt::Display) {
    html.push_fragment(
        &Markup::new()
            .markup("<p class=\"actions-row\"><a class=\"button secondary\" rel=\"next\" href=\"")
            .text(route)
            .markup("?after=")
            .text(after.to_string())
            .markup("\">Next page</a></p>"),
    );
}

/// Splits one store read into the rows a page shows and whether more follow:
/// at most `shown` rows are kept, and more follow when the read held more than
/// that or when `rest` finds a row after the last one kept. An empty read has
/// nothing after it.
pub(super) fn one_page<T, E>(
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

/// Says, only while it is true, that order sending is paused after a restore.
pub(super) async fn pause(state: &Context, html: &mut Markup) -> Result<(), Error> {
    let paused = state
        .store
        .request(deadline()?, |s| s.restore_unreconciled())
        .await?;
    if paused {
        html.push_markup(concat!(
            "<p class=\"notice\" title=\"Order sending paused after restore: true\">",
            "<strong>Order sending is paused after a restore.</strong> ",
        ));
        html.push_markup(format::RESTORE_ADVICE);
        html.push_markup("</p>");
    }
    Ok(())
}
