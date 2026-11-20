//! Renders the Machines page: each set-up machine's link light, departures and first stored messages.
pub(super) mod departures;

use crate::admin::pages::{Cursor, PAGE, finish, next, pause};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::server::{Context, Machine};
use crate::admin::view::{self, Tab, Tone, exact, format, pill, row};
use crate::deadline;
use crate::machine::link::status::{Light, Link};
use axum::{
    extract::{Query, State},
    response::Response,
};
use jiff::Timestamp;
use std::sync::{Arc, atomic::Ordering};

pub(in crate::admin) async fn machines(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(
        async {
            let mut html = view::page(Tab::Machines, "Machines");
            pause(&state, &mut html).await?;
            let after = usize::try_from(cursor.after)?;
            let page: Vec<_> = state.machines.iter().skip(after).take(PAGE).collect();
            let now = Timestamp::now();
            let activity = crate::admin::overview::activity(&state, deadline()?, PAGE).await?;
            for machine in &page {
                let seen = activity.get(machine.id.as_str());
                let last = seen.and_then(|seen| seen.last);
                let received = seen.map_or(&[][..], |seen| seen.first.as_slice());
                let mut evidence = Markup::new();
                for &(receipt, received_at) in received {
                    let id = uuid::Uuid::from(receipt);
                    evidence.push_fragment(
                        &Markup::new()
                            .markup("<li><a href=\"/receipts/")
                            .text(id)
                            .markup(
                                "/departures\" title=\"Receipt interpretation evidence\">Received ",
                            )
                            .text(format::time(received_at))
                            .markup("</a> <small class=\"muted\" title=\"Receipt ID\">")
                            .text(id)
                            .markup("</small></li>"),
                    );
                }
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2>")
                        .text(machine.id.as_str())
                        .markup("</h2><dl>")
                        .fragment(&LinkLight::of(machine, last, now).rows())
                        .markup("</dl>")
                        .fragment(&machine.departure_html)
                        .markup("<h3>Messages received</h3>"),
                );
                if received.is_empty() {
                    html.push_markup("<p class=\"muted\">Nothing yet.</p>");
                } else {
                    html.push_fragment(
                        &Markup::new()
                            .markup("<ul>")
                            .fragment(&evidence)
                            .markup("</ul>"),
                    );
                }
                html.push_markup("</section>");
            }
            let shown = after
                .checked_add(page.len())
                .ok_or(Error::MachineCursorOverflow)?;
            if shown < state.machines.len() {
                next(&mut html, "/machines", shown);
            }
            Ok(html)
        }
        .await,
    )
}

/// One machine's link light, ready to show. "Last data" is the later of the
/// runtime's last read and the last stored receipt, so it survives a restart. It
/// never shows payloads, sample IDs or peer addresses (safety rule 10).
pub(in crate::admin) struct LinkLight {
    pub(in crate::admin) kind: Markup,
    pub(in crate::admin) state: Markup,
    pub(in crate::admin) last_data: Markup,
    pub(in crate::admin) changed: Markup,
    pub(in crate::admin) service: Markup,
}

impl LinkLight {
    pub(in crate::admin) fn of(
        machine: &Machine,
        stored: Option<Timestamp>,
        now: Timestamp,
    ) -> Self {
        let status = machine.status.borrow().clone();
        let tone = match status.light() {
            Light::Live => Tone::Good,
            Light::Waiting => Tone::Attention,
            Light::Off => Tone::Neutral,
        };
        Self {
            kind: exact(link_words(status.link()), status.link().as_str()),
            state: pill(tone, status.state()),
            last_data: status.last_data().max(stored).map_or_else(
                || Markup::new().text(format::NOTHING_YET),
                |time| format::when(time, now),
            ),
            changed: format::when(status.changed_at(), now),
            service: if machine.running.load(Ordering::Acquire) {
                exact("Running", "machine owner: running")
            } else {
                exact("Stopped", "machine owner: stopped")
            },
        }
    }

    /// The light as key and value rows, for a machine's own card.
    fn rows(&self) -> Markup {
        Markup::new()
            .fragment(&row("Connection", Some("Connection state"), &self.state))
            .fragment(&row("Last data received", None, &self.last_data))
            .fragment(&row("Since", Some("Last state change"), &self.changed))
            .fragment(&row("Connection type", Some("Link"), &self.kind))
            .fragment(&row("Service", Some("Machine owner"), &self.service))
    }
}

/// How the machine is attached, in plain words; the technical link kind shows on hover.
const fn link_words(link: Link) -> &'static str {
    match link {
        Link::TcpServer => "Network: the machine connects to Analyte",
        Link::TcpClient => "Network: Analyte connects to the machine",
        Link::Serial => "Serial cable",
        Link::Folder => "Watched folder",
    }
}
