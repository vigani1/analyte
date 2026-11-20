//! Serves the pages under the sidebar's More group (Machine orders with an order item's history, Cancelled by machine, Deliveries with Send again, Data keeping with its holds, and Settings) and what they share: paging, the read budget and their error pages.
use crate::admin::server::Context;
use crate::admin::sessions::Session;
use crate::admin::view::{
    self, Tab, Tone,
    report::{Problem, Report, Section, respond},
};
use askama::Template as _;
use axum::{Extension, http::StatusCode, response::Response};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;
pub(super) mod awos;
mod deliveries;
pub(super) use deliveries::{confirm_resend, deliveries, resend};
mod orders;
pub(super) use orders::orders;
mod page;
pub(super) mod retention;
mod settings;
pub(super) use settings::{Installed, settings};

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

/// Runs a More page's reads within [`view::READ_BUDGET`] and draws the page
/// from what they returned, for the request's session, now; a read that fails
/// or takes too long gets the page saying so, with the reason kept for
/// support. The reads only read, so stopping them leaves nothing behind.
async fn show<T>(
    state: &Context,
    session: Option<&Extension<Arc<Session>>>,
    tab: Tab,
    read: impl Future<Output = Result<T, crate::admin::Error>>,
    draw: impl FnOnce(T, Option<&Session>, Timestamp) -> Result<String, askama::Error>,
) -> Response {
    let session = session.map(|Extension(session)| session.as_ref());
    let section = Section::More(tab);
    match tokio::time::timeout(view::READ_BUDGET, read).await {
        Ok(Ok(value)) => {
            let html = draw(value, session, Timestamp::now());
            respond(state, session, section, StatusCode::OK, html)
        }
        Ok(Err(error)) => Problem::Store(error.to_string()).page(state, session, section),
        Err(_) => Problem::Time.page(state, session, section),
    }
}

/// The address of the next page of a list: `href` with its filter, then the
/// cursor of the last row shown.
fn next_href(href: &str, after: i64) -> String {
    let joiner = if href.contains('?') { '&' } else { '?' };
    format!("{href}{joiner}after={after}")
}

/// What an action did, as its page says it: the page's title, the alert's
/// tone, title and text.
struct Outcome {
    title: &'static str,
    tone: Tone,
    alert: String,
    text: String,
}

/// A page under `tab` that only reports what an action did or why it did not
/// run, with `status`, the way back and the exact IDs for support.
fn report(
    state: &Context,
    session: &Session,
    tab: Tab,
    status: StatusCode,
    outcome: Outcome,
    support: Vec<String>,
) -> Response {
    let section = Section::More(tab);
    let page = Report {
        frame: section.frame(state, outcome.title, Some(session)),
        head: section.head(outcome.title),
        tone: outcome.tone,
        title: outcome.alert,
        text: outcome.text,
        lines: Vec::new(),
        problems: Vec::new(),
        restart: None,
        links: section.links(),
        support,
    };
    respond(state, Some(session), section, status, page.render())
}
