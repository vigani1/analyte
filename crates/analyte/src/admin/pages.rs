//! Bounded operator views; raw evidence requires an audited, CSRF-checked POST.
use super::{escape, server::Context, sessions::Session};
use crate::{Error, deadline};
use analyte_model::{Identifier, Reason, ReceiptId};
use analyte_store::{Audit, MachineOrderFilter, Parked, Store};
use axum::{
    Extension, Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::{Arc, atomic::Ordering};

const PAGE: usize = 50;
const RAW_PAGE: usize = 4096;
pub(super) const NAV: &str = "<nav><a href=\"/\">Overview</a> <a href=\"/machines\">Machines</a> <a href=\"/parked\">Parked items</a> <a href=\"/orders\">Machine orders</a></nav>";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
}

fn cursor_value<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    i64::try_from(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

fn start(title: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title>{NAV}<h1>{}</h1>",
        escape(title),
        escape(title)
    )
}

fn finish(result: Result<String, Error>) -> Response {
    match result {
        Ok(mut html) => {
            html.push_str("</html>");
            Html(html).into_response()
        }
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(format!(
                "{NAV}<h1>Read failed</h1><p>{}</p>",
                escape(&error.to_string())
            )),
        )
            .into_response(),
    }
}

fn next(html: &mut String, route: &str, after: impl std::fmt::Display) {
    html.push_str(&format!(
        "<a rel=\"next\" href=\"{}?after={}\">Next page</a>",
        escape(route),
        escape(&after.to_string())
    ));
}

async fn pause(state: &Context, html: &mut String) -> Result<(), Error> {
    let paused = state
        .store
        .request(deadline(), |s| s.restore_unreconciled())
        .await?;
    html.push_str(if paused {
        "<p>Order sending paused after restore: true. CLI reconciliation required.</p>"
    } else {
        "<p>Order sending paused after restore: false.</p>"
    });
    Ok(())
}

pub(super) async fn machines(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(
        async {
            let mut html = start("Machines");
            pause(&state, &mut html).await?;
            let after = usize::try_from(cursor.after)?;
            let page: Vec<_> = state.machines.iter().skip(after).take(PAGE).collect();
            for machine in &page {
                html.push_str(&format!(
                    "<section><h2>{}</h2><p>Owner: {}. Connection state: unknown.</p>",
                    escape(machine.id.as_str()),
                    if machine.running.load(Ordering::Acquire) {
                        "running"
                    } else {
                        "stopped"
                    }
                ));
                let mut cursor = String::new();
                let until = deadline();
                let mut last = None;
                loop {
                    let rows = state
                        .store
                        .request(until, move |s| s.receipts(&cursor))
                        .await?;
                    let Some(end) = rows.last() else {
                        break;
                    };
                    cursor = serde_json::to_string(&end.id)?;
                    for receipt in rows {
                        if receipt.machine == machine.id {
                            last = last.max(Some(receipt.received_at));
                        }
                    }
                }
                html.push_str(&format!(
                    "<p>Last received activity: {}</p></section>",
                    escape(&last.map_or_else(|| "unknown".into(), |time| time.to_string()))
                ));
            }
            if !page.is_empty() {
                next(
                    &mut html,
                    "/machines",
                    after
                        .checked_add(page.len())
                        .ok_or("machine cursor overflow")?,
                );
            }
            Ok(html)
        }
        .await,
    )
}

fn receipt_id(target: &Identifier) -> Option<ReceiptId> {
    target
        .as_str()
        .parse::<uuid::Uuid>()
        .ok()
        .and_then(|id| ReceiptId::try_from(id).ok())
}

fn metadata(store: &Store, parked: &Parked) -> Result<String, analyte_store::Error> {
    let mut html = String::new();
    for target in parked.targets.iter().take(PAGE) {
        let receipt = receipt_id(target)
            .map(|id| store.receipt_metadata(id))
            .transpose()?
            .flatten();
        html.push_str(&format!(
            "<p>Target: {}. Receipt time: {}.</p>",
            escape(target.as_str()),
            escape(&receipt.map_or_else(
                || "unknown (no linked receipt)".into(),
                |m| m.received_at.to_string()
            ))
        ));
    }
    if parked.targets.is_empty() {
        html.push_str("<p>Receipt ID and time: not recorded.</p>");
    }
    if parked.targets.len() > PAGE {
        html.push_str("<p>First 50 targets shown; use the detail target pages for the rest.</p>");
    }
    Ok(html)
}

pub(super) async fn parked(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(async {
        let after = i64::try_from(cursor.after)?;
        let (rows, last) = state.store.request(deadline(), move |s| {
            let mut html = String::new();
            let mut last = None;
            for row in s.parked(after)?.into_iter().take(PAGE) {
                let evidence: Parked = serde_json::from_str(&row.evidence)?;
                html.push_str(&format!("<section><h2><a href=\"/parked/{}\">Parked {}</a></h2><p>Category: {}. Reason: {}.</p>", row.id, row.id, escape(evidence.kind.as_str()), escape(evidence.reason.as_str())));
                html.push_str(&metadata(s, &evidence)?);
                html.push_str(if row.resolution.is_some() { "<p>Resolved</p></section>" } else { "<p>Unresolved</p></section>" });
                last = Some(row.id);
            }
            Ok((html, last))
        }).await?;
        let mut html = start("Parked items");
        html.push_str("<p>No raw bytes are shown on this list. Times below are receipt times; parking time is not recorded by the store.</p>");
        html.push_str(&rows);
        if let Some(last) = last { next(&mut html, "/parked", last); }
        Ok(html)
    }.await)
}

fn raw_form(
    html: &mut String,
    id: i64,
    session: &Session,
    target: &str,
    source: &str,
    cursor: RawCursor,
    reason: &str,
) {
    html.push_str(&format!("<p>Raw source: {}. Target: {}. Cursor: {}, {}.</p><form action=\"/parked/{id}/raw\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"target\" value=\"{}\"><input type=\"hidden\" name=\"source\" value=\"{}\"><input type=\"hidden\" name=\"after\" value=\"{}\"><input type=\"hidden\" name=\"offset\" value=\"{}\"><label>Reason (optional) <input name=\"reason\" maxlength=\"256\" value=\"{}\"></label><button>Show raw (audited)</button></form>", escape(source), escape(target), escape(&cursor.after.to_string()), escape(&cursor.offset.to_string()), escape(session.csrf.form_value()), escape(target), escape(source), escape(&cursor.after.to_string()), escape(&cursor.offset.to_string()), escape(reason)));
}

pub(super) async fn detail(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let result = state
        .store
        .request(deadline(), move |s| {
            let Some(row) = s.parked_item(id)? else {
                return Ok(None);
            };
            let mut evidence: Parked = serde_json::from_str(&row.evidence)?;
            evidence.targets = evidence
                .targets
                .into_iter()
                .skip(
                    usize::try_from(cursor.after)
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                )
                .take(PAGE)
                .collect();
            Ok(Some((metadata(s, &evidence)?, evidence, row.resolution)))
        })
        .await;
    let (meta, evidence, resolution) = match result {
        Ok(Some(row)) => row,
        Ok(None) => return (StatusCode::NOT_FOUND, "parked item not found").into_response(),
        Err(error) => return finish(Err(error.into())),
    };
    let mut html = start("Parked item");
    html.push_str(&format!(
        "<p>Category: {}. Reason: {}.</p>{meta}",
        escape(evidence.kind.as_str()),
        escape(evidence.reason.as_str())
    ));
    if let Some(resolution) = resolution {
        html.push_str(&format!("<p>Resolution: {}</p>", escape(&resolution)));
    }
    html.push_str("<p>Raw bytes are hidden. Each raw page is disclosed only after its audit entry is durable.</p>");
    if let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) {
        for target in &evidence.targets {
            if receipt_id(target).is_some() {
                raw_form(
                    &mut html,
                    id,
                    &session,
                    target.as_str(),
                    "receipt",
                    RawCursor::default(),
                    "",
                );
            }
            raw_form(
                &mut html,
                id,
                &session,
                target.as_str(),
                "frames",
                RawCursor::default(),
                "",
            );
        }
    } else {
        html.push_str("<p><a href=\"/login\">Log in</a> to authorize an audited raw view.</p>");
    }
    if !evidence.targets.is_empty() {
        let Some(after) = u64::try_from(evidence.targets.len())
            .ok()
            .and_then(|len| cursor.after.checked_add(len))
        else {
            return finish(Err(crate::invalid("parked target cursor overflow")));
        };
        next(&mut html, &format!("/parked/{id}"), after);
    }
    finish(Ok(html))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Source {
    Receipt,
    Frames,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawForm {
    csrf: String,
    target: Identifier,
    source: Source,
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
    #[serde(default, deserialize_with = "cursor_value")]
    offset: u64,
    #[serde(default)]
    reason: String,
}

struct RawPage {
    bytes: Vec<u8>,
    next: Option<RawCursor>,
}

#[derive(Clone, Copy, Default)]
struct RawCursor {
    after: u64,
    offset: u64,
}

fn read_raw(s: &Store, form: &RawForm) -> Result<Option<RawPage>, analyte_store::Error> {
    match form.source {
        Source::Receipt => {
            if form.offset != 0 {
                return Err(analyte_store::Error::InvalidInput);
            }
            let Some(id) = receipt_id(&form.target) else {
                return Ok(None);
            };
            let Some(receipt) = s.receipt(id)? else {
                return Ok(None);
            };
            let after =
                usize::try_from(form.after).map_err(|_| analyte_store::Error::InvalidInput)?;
            let Some(tail) = receipt.raw.get(after..) else {
                return Err(analyte_store::Error::InvalidInput);
            };
            let bytes: Vec<u8> = tail.iter().take(RAW_PAGE).copied().collect();
            let next = after
                .checked_add(bytes.len())
                .ok_or(analyte_store::Error::InvalidInput)?;
            Ok(Some(RawPage {
                bytes,
                next: if next < receipt.raw.len() {
                    Some(RawCursor {
                        after: u64::try_from(next)
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        offset: 0,
                    })
                } else {
                    None
                },
            }))
        }
        Source::Frames => {
            let after =
                i64::try_from(form.after).map_err(|_| analyte_store::Error::InvalidInput)?;
            let frames = s.frames(&form.target, after)?;
            let Some(frame) = frames.first() else {
                return Ok(None);
            };
            let offset =
                usize::try_from(form.offset).map_err(|_| analyte_store::Error::InvalidInput)?;
            let tail = frame
                .raw
                .get(offset..)
                .ok_or(analyte_store::Error::InvalidInput)?;
            let bytes: Vec<_> = tail.iter().take(RAW_PAGE).copied().collect();
            let end = offset
                .checked_add(bytes.len())
                .ok_or(analyte_store::Error::InvalidInput)?;
            Ok(Some(RawPage {
                bytes,
                next: Some(if end < frame.raw.len() {
                    RawCursor {
                        after: form.after,
                        offset: u64::try_from(end)
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                    }
                } else {
                    RawCursor {
                        after: u64::try_from(frame.sequence)
                            .map_err(|_| analyte_store::Error::Corruption)?,
                        offset: 0,
                    }
                }),
            }))
        }
    }
}

pub(super) async fn raw(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<RawForm>,
) -> Response {
    if !session.authenticated || !session.csrf_valid(&form.csrf) {
        return (StatusCode::FORBIDDEN, "authenticated CSRF session required").into_response();
    }
    let target = form.target.clone();
    let source = match form.source {
        Source::Receipt => "receipt",
        Source::Frames => "frames",
    };
    let reason_text = form.reason.clone();
    let result = async {
        let reason = Reason::new(if form.reason.is_empty() {
            "operator requested raw evidence"
        } else {
            &form.reason
        })?;
        let audit = Audit {
            actor: session.actor.clone(),
            action: Identifier::new("admin-raw-view")?,
            targets: vec![
                Identifier::new(&format!("parked:{id}"))?,
                target.clone(),
                Identifier::new(&format!("{source}-offset:{}:{}", form.after, form.offset))?,
            ],
            time: jiff::Timestamp::now().as_second(),
            reason,
        };
        Ok::<_, Error>(
            state
                .store
                .request(deadline(), move |s| {
                    let Some(row) = s.parked_item(id)? else {
                        return Ok(None);
                    };
                    let evidence: Parked = serde_json::from_str(&row.evidence)?;
                    if !evidence.targets.contains(&form.target) {
                        return Err(analyte_store::Error::InvalidInput);
                    }
                    let page = read_raw(s, &form)?;
                    if page.is_some() {
                        s.audit(&audit)?;
                    }
                    Ok(page)
                })
                .await?,
        )
    }
    .await;
    match result {
        Ok(Some(page)) => {
            let mut html = start("Audited raw evidence");
            html.push_str(&format!(
                "<p>Target: {}. Actor: {}.</p><pre>",
                escape(target.as_str()),
                escape(session.actor.as_str())
            ));
            for chunk in page.bytes.chunks(16) {
                let hex = chunk
                    .iter()
                    .map(|byte| format!("{byte:02x} "))
                    .collect::<String>();
                let text = chunk
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii_graphic() || *byte == b' ' {
                            char::from(*byte)
                        } else {
                            '.'
                        }
                    })
                    .collect::<String>();
                html.push_str(&escape(&hex));
                html.push_str(" | ");
                html.push_str(&escape(&text));
                html.push('\n');
            }
            html.push_str("</pre><p>Nonprinting and non-ASCII bytes are shown as dots; hex preserves each byte exactly.</p>");
            if let Some(after) = page.next {
                raw_form(
                    &mut html,
                    id,
                    &session,
                    target.as_str(),
                    source,
                    after,
                    &reason_text,
                );
            }
            finish(Ok(html))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            "no linked raw evidence at this cursor",
        )
            .into_response(),
        Err(error) => finish(Err(error)),
    }
}

pub(super) async fn orders(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(async {
        let after = i64::try_from(cursor.after)?;
        let (rows, last) = state.store.request(deadline(), move |s| {
            let mut html = String::new();
            let mut last = None;
            for order in s.machine_orders(&MachineOrderFilter {after, machine: None, state: None})?.into_iter().take(PAGE) {
                html.push_str(&format!("<section><h2>Machine order {}</h2><p>App: {}. Order: {}. Machine: {}. Reserved version: {}. State: {}. Updated (Unix seconds): {}.</p>", order.id, escape(order.key.destination.as_str()), escape(order.key.order.as_str()), escape(order.key.machine.as_str()), escape(&order.version.to_string()), escape(&format!("{:?}", order.state)), escape(&order.updated_at.to_string())));
                if let Some(cached) = s.cached_order(&order.key.destination, &order.key.order)? {
                    html.push_str(&format!("<p>Retained cache sample: {}. Tombstone: {}. Expiry (Unix seconds): {}.</p>", escape(cached.sample.as_str()), escape(&cached.tombstone.to_string()), escape(&cached.expiry.to_string())));
                    match analyte_contract::decode::<analyte_contract::Order>(cached.document.as_bytes()) {
                        Ok(document) => {
                            let fields = document.fields();
                            html.push_str(&format!("<p>Retained cache version: {}. {} Cache items:</p><ul>", escape(&fields.version.to_string()), if fields.version == order.version {"Matches reserved version."} else {"Different version; these are not the historical machine items."}));
                            for item in &fields.tests { html.push_str(&format!("<li>{}: {}</li>", escape(item.namespace.as_str()), escape(item.value.as_str()))); }
                            html.push_str("</ul>");
                        }
                        Err(error) => html.push_str(&format!("<p>Cached document invalid; items unavailable: {}</p>", escape(&error.to_string()))),
                    }
                } else { html.push_str("<p>Cached items unavailable; no item identities are inferred.</p>"); }
                html.push_str("</section>"); last = Some(order.id);
            }
            Ok((html, last))
        }).await?;
        let mut html = start("Machine orders"); pause(&state, &mut html).await?;
        html.push_str(&rows);
        if let Some(last) = last { next(&mut html, "/orders", last); }
        Ok(html)
    }.await)
}
