//! Lists the deliveries Analyte sends to a lab app, with a resend link where one is allowed.
use super::{Cursor, PAGE, next, one_page};
use crate::admin::actions::{failure, page};
use crate::admin::error::CommandError;
use crate::admin::markup::Markup;
use crate::admin::{
    server::Context,
    view::{self, Tab, Tone, plain},
};
use crate::deadline;
use analyte_store::DeliveryState;
use axum::{
    extract::{Query, State},
    response::Response,
};
use std::sync::Arc;

pub(in crate::admin) async fn deliveries(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    let after = match i64::try_from(cursor.after) {
        Ok(after) => after,
        Err(error) => return failure(&error),
    };
    let result = state
        .store
        .request::<_, CommandError>(
            {
                match deadline() {
                    Ok(until) => until,
                    Err(error) => return failure(&error),
                }
            },
            move |s| {
                one_page(s.deliveries(after)?, PAGE, |last| {
                    Ok(!s.deliveries(last.sequence)?.is_empty())
                })
            },
        )
        .await;
    match result {
        Ok((rows, more)) => {
            let mut html = Markup::new().markup(concat!(
                "<p class=\"lead\">Results and order messages Analyte sends to a lab app. Each ",
                "keeps the same delivery number on every retry, so the lab app never files it ",
                "twice.</p>",
            ));
            if rows.is_empty() {
                html.push_markup("<p class=\"muted\">No deliveries to show.</p>");
            } else {
                html.push_markup(concat!(
                    "<section><div class=\"table\"><table class=\"stack\"><tr>",
                    "<th title=\"Delivery ID\">Delivery number</th>",
                    "<th title=\"Destination\">Lab app</th><th>Status</th><th></th></tr>",
                ));
            }
            for row in &rows {
                let delivery = uuid::Uuid::from(row.id);
                let tone = match row.state {
                    DeliveryState::Done => Tone::Good,
                    DeliveryState::Pending | DeliveryState::InFlight => Tone::Neutral,
                    DeliveryState::Parked | DeliveryState::Paused | DeliveryState::Conflict => {
                        Tone::Attention
                    }
                };
                html.push_fragment(
                    &Markup::new()
                        .markup("<tr><td><code>")
                        .text(delivery)
                        .markup("</code></td><td data-label=\"Lab app\">")
                        .text(row.destination.as_str())
                        .markup("</td><td data-label=\"Status\"><span title=\"")
                        .text(row.state)
                        .markup("\">")
                        .fragment(&view::pill(tone, plain::delivery_state(row.state)))
                        .markup("</span></td><td class=\"actions\">"),
                );
                match row.state {
                    DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => html
                        .push_fragment(
                            &Markup::new()
                                .markup("<a href=\"/deliveries/")
                                .text(delivery)
                                .markup("/resend\" title=\"Resend this delivery\">Send again</a>"),
                        ),
                    DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => {}
                }
                html.push_markup("</td></tr>");
            }
            if let Some(last) = rows.last() {
                html.push_markup("</table></div></section>");
                if more {
                    next(&mut html, "/deliveries", last.sequence);
                }
            }
            page(Tab::Deliveries, "Deliveries", &html)
        }
        Err(error) => failure(&error),
    }
}
