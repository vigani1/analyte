//! Renders protected machine orders and retained cache evidence.
use super::{Cursor, PAGE, finish, next, one_page, pause};
use crate::admin::view::{self, Tab, exact, format, row};
use crate::{
    admin::{Error, markup::Markup, server::Context},
    deadline,
};
use analyte_model::Identifier;
use analyte_store::MachineOrderFilter;
use axum::{
    extract::{Query, State},
    response::Response,
};
use std::sync::Arc;
pub(in crate::admin) async fn orders(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    let result = orders_page(&state, cursor).await;

    finish(result)
}

async fn orders_page(state: &Context, cursor: Cursor) -> Result<Markup, Error> {
    let after = i64::try_from(cursor.after)?;
    let (rows, last, more) = state
        .store
        .request::<_, crate::admin::error::CommandError>(deadline()?, move |s| {
            let mut html = Markup::new();
            let mut last = None;
            let now = jiff::Timestamp::now();
            let all = |after| MachineOrderFilter {
                after,
                machine: None,
                state: None,
            };
            let (fetched, more) = one_page::<_, crate::admin::error::CommandError>(
                s.machine_orders(&all(after))?,
                PAGE,
                |last| Ok(!s.machine_orders(&all(last.id))?.is_empty()),
            )?;
            for order in fetched {
                html.push_fragment(&order_section(&order, now));
                for item in &order.items {
                    html.push_fragment(&super::awos::projection(item));
                }
                render_cache(&mut html, s, &order, now)?;
                html.push_markup("</section>");
                last = Some(order.id);
            }
            Ok::<_, crate::admin::error::CommandError>((html, last, more))
        })
        .await?;
    let mut html = view::page(Tab::MachineOrders, "Machine orders").markup(concat!(
        "<p class=\"lead\">Orders from the lab app that Analyte sent to a machine, with what ",
        "the machine did with them. Analyte never sends an order again on its own.</p>",
    ));
    pause(state, &mut html).await?;
    if last.is_none() {
        html.push_markup("<p class=\"muted\">No orders to show.</p>");
    }
    html.push_fragment(&rows);
    if let Some(last) = last
        && more
    {
        next(&mut html, "/orders", last);
    }
    Ok(html)
}

/// One order's heading, facts and the tests sent to the machine, item by item.
fn order_section(order: &analyte_store::MachineOrder, now: jiff::Timestamp) -> Markup {
    let mut html = Markup::new();
    html.push_fragment(
        &Markup::new()
            .markup("<section><h2 title=\"Machine order ")
            .text(order.id)
            .markup("\">Order: ")
            .text(order.key.order.as_str())
            .markup("</h2><dl>")
            .fragment(&row(
                "Machine",
                None,
                &Markup::new().text(order.key.machine.as_str()),
            ))
            .fragment(&row(
                "Lab app",
                Some("Destination"),
                &Markup::new().text(order.key.destination.as_str()),
            ))
            .fragment(&row(
                "Status",
                None,
                &exact(format::order_state(order.state), &order.state.to_string()),
            ))
            .fragment(&row(
                "Order version sent",
                Some("Reserved version"),
                &Markup::new().text(order.version),
            ))
            .fragment(&row(
                "Last change",
                None,
                &format::when_unix(order.updated_at, now),
            ))
            .markup("</dl>"),
    );
    html.push_markup("<h3 title=\"Durable machine items\">Tests sent to the machine</h3><ul>");
    for item in &order.items {
        html.push_fragment(
            &Markup::new()
                .markup("<li><strong>Item ")
                .text(item.index)
                .markup("</strong>: ")
                .fragment(&exact(
                    format::order_state(item.state),
                    &item.state.to_string(),
                ))
                .markup(" <small class=\"muted\" title=\"Placer, namespace and source code\">Order number ")
                .text(&item.placer)
                .markup(" (")
                .text(
                    item.placer_namespace
                        .as_ref()
                        .map_or("namespace not recorded", Identifier::as_str),
                )
                .markup("), last note: ")
                .text(item.source_code.as_str())
                .markup("</small><ul>"),
        );
        if item.coverage_known {
            for test in &item.tests {
                html.push_fragment(
                    &Markup::new()
                        .markup("<li>Machine test: ")
                        .text(test.namespace.as_str())
                        .markup(": ")
                        .text(test.value.as_str())
                        .markup("</li>"),
                );
            }
        } else {
            html.push_markup(concat!(
                "<li title=\"Historical item coverage unknown (legacy evidence)\">",
                "Which tests this item covered was not recorded (older data).</li>",
            ));
        }
        html.push_markup("</ul></li>");
    }
    if order.items.is_empty() {
        html.push_markup(concat!(
            "<li>The tests sent for this order were not recorded (older data); ",
            "Analyte does not guess them.</li>",
        ));
    }
    html.push_markup("</ul>");
    html
}

fn render_cache(
    html: &mut Markup,
    store: &analyte_store::Store,
    order: &analyte_store::MachineOrder,
    now: jiff::Timestamp,
) -> Result<(), crate::admin::error::CommandError> {
    if let Some(cached) = store.cached_order(&order.key.destination, &order.key.order)? {
        html.push_fragment(
            &Markup::new()
                .markup(
                    "<h3 title=\"Retained order cache\">The lab app's copy of this order</h3><dl>",
                )
                .fragment(&row(
                    "Sample",
                    None,
                    &Markup::new().text(cached.sample.as_str()),
                ))
                .fragment(&row(
                    "Cancelled in the lab app",
                    Some("Tombstone"),
                    &Markup::new().text(format::yes_no(cached.tombstone)),
                ))
                .fragment(&row(
                    "Kept until",
                    Some("Cache expiry"),
                    &format::when_unix(cached.expiry, now),
                ))
                .markup("</dl>"),
        );
        match analyte_contract::decode::<analyte_contract::Order>(cached.document.as_bytes()) {
            Ok(document) => {
                let fields = document.fields();
                html.push_fragment(
                    &Markup::new()
                        .markup("<p title=\"Retained cache version\">Lab app order version ")
                        .text(fields.version)
                        .markup(": ")
                        .text(if fields.version == order.version {
                            "the same version that was sent."
                        } else {
                            concat!(
                                "a different version from the one sent, so these are not ",
                                "the tests the machine got.",
                            )
                        })
                        .markup(" Its tests:</p><ul>"),
                );
                for item in &fields.tests {
                    html.push_fragment(
                        &Markup::new()
                            .markup("<li>")
                            .text(item.namespace.as_str())
                            .markup(": ")
                            .text(item.value.as_str())
                            .markup("</li>"),
                    );
                }
                html.push_markup("</ul>");
            }
            Err(error) => html.push_fragment(
                &Markup::new()
                    .markup("<p>The lab app's copy cannot be read, so its tests are not shown. ")
                    .markup("<span class=\"muted\">Details for support: <code>")
                    .text(error.to_string())
                    .markup("</code></span></p>"),
            ),
        }
    } else {
        html.push_markup(concat!(
            "<p class=\"muted\">No copy of this order from the lab app is kept, so its tests ",
            "are not shown; Analyte does not guess them.</p>",
        ));
    }

    Ok(())
}
