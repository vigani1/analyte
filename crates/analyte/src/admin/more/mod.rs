//! Serves the pages under the sidebar's More group (Machine orders with an order item's history, Cancelled by machine, Deliveries with Send again, Data keeping with its holds, and Settings) and what they share: the read budget and their error pages.
use crate::admin::server::Context;
use crate::admin::server::sessions::Session;
use crate::admin::view::{
    self, Tab, Tone,
    report::{Problem, Report, Section, respond},
};
use askama::Template as _;
use axum::{Extension, http::StatusCode, response::Response};
use jiff::Timestamp;
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

/// Runs a page's reads within [`view::READ_BUDGET`] and draws the page
/// from what they returned, for the request's session, now; a read that fails
/// or takes too long gets the page saying so, with the reason kept for
/// support. The reads only read, so stopping them leaves nothing behind.
pub(super) async fn show<T>(
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
        Ok(Err(error)) => Problem::of(&error).page(state, session, section),
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
