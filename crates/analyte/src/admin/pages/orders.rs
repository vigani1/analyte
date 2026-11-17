//! Renders protected machine orders and retained cache evidence.
use super::{Cursor, PAGE, finish, next, pause, start};
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
    let (rows, last) = state
        .store
        .request::<_, analyte_store::Error>(deadline()?, move |s| {
            let mut html = Markup::new();
            let mut last = None;
            for order in s
                .machine_orders(&MachineOrderFilter {
                    after,
                    machine: None,
                    state: None,
                })?
                .into_iter()
                .take(PAGE)
            {
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2>Machine order ")
                        .text(order.id)
                        .markup("</h2><p>App: ")
                        .text(order.key.destination.as_str())
                        .markup(". Order: ")
                        .text(order.key.order.as_str())
                        .markup(". Machine: ")
                        .text(order.key.machine.as_str())
                        .markup(". Reserved version: ")
                        .text(order.version.to_string())
                        .markup(". State: ")
                        .text(order.state.to_string())
                        .markup(". Updated (Unix seconds): ")
                        .text(order.updated_at.to_string())
                        .markup(".</p>"),
                );
                html.push_markup("<h3>Durable machine items</h3><ul>");
                for item in &order.items {
                    html.push_fragment(
                        &Markup::new()
                            .markup("<li>Item ")
                            .text(item.index.to_string())
                            .markup(". State: ")
                            .text(item.state.to_string())
                            .markup(". Placer: ")
                            .text(&item.placer)
                            .markup(". Namespace: ")
                            .text(
                                item.placer_namespace
                                    .as_ref()
                                    .map_or("not recorded", Identifier::as_str),
                            )
                            .markup(". Source: ")
                            .text(item.source_code.as_str())
                            .markup(".<ul>"),
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
                            "<li>Historical item coverage unknown (legacy ",
                            "evidence).</li>",
                        ));
                    }
                    html.push_markup("</ul></li>");
                }
                if order.items.is_empty() {
                    html.push_markup(concat!(
                        "<li>Historical items unavailable; none are ",
                        "inferred.</li>",
                    ));
                }
                html.push_markup("</ul>");
                for item in &order.items {
                    html.push_fragment(&crate::admin::awos::projection(item));
                }
                render_cache(&mut html, s, &order)?;
                html.push_markup("</section>");
                last = Some(order.id);
            }
            Ok::<_, analyte_store::Error>((html, last))
        })
        .await?;
    let mut html = start("Machine orders");
    pause(state, &mut html).await?;
    html.push_fragment(&rows);
    if let Some(last) = last {
        next(&mut html, "/orders", last);
    }
    Ok(html)
}

fn render_cache(
    html: &mut Markup,
    store: &analyte_store::Store,
    order: &analyte_store::MachineOrder,
) -> Result<(), analyte_store::Error> {
    if let Some(cached) = store.cached_order(&order.key.destination, &order.key.order)? {
        html.push_fragment(
            &Markup::new()
                .markup("<p>Retained cache sample: ")
                .text(cached.sample.as_str())
                .markup(". Tombstone: ")
                .text(cached.tombstone.to_string())
                .markup(". Expiry (Unix seconds): ")
                .text(cached.expiry.to_string())
                .markup(".</p>"),
        );
        match analyte_contract::decode::<analyte_contract::Order>(cached.document.as_bytes()) {
            Ok(document) => {
                let fields = document.fields();
                html.push_fragment(
                    &Markup::new()
                        .markup("<p>Retained cache version: ")
                        .text(fields.version.to_string())
                        .markup(". ")
                        .text(if fields.version == order.version {
                            "Matches reserved version."
                        } else {
                            concat!(
                                "Different version; these are not the historical ",
                                "machine items.",
                            )
                        })
                        .markup(" Cache items:</p><ul>"),
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
                    .markup("<p>Cached document invalid; items unavailable: ")
                    .text(error.to_string())
                    .markup("</p>"),
            ),
        }
    } else {
        html.push_markup(concat!(
            "<p>Cached items unavailable; no item identities ",
            "are inferred.</p>",
        ));
    }

    Ok(())
}
