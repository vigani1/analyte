//! Discloses bounded raw evidence only after authenticated, durable audit.
use super::{cursor_value, finish, receipt_id, start};
use crate::{
    admin::{Error, markup::Markup, server::Context, sessions::Session},
    deadline,
};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, Parked, Store};
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;
/// Refusals and boundary failures of an audited raw-evidence page.
#[derive(Debug, thiserror::Error)]
pub enum RawError {
    /// Retained evidence could not be queried.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Disclosure could not be audited durably.
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    /// A supplied cursor does not fit its representation.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// Receipt pages do not support a frame offset.
    #[error("receipt raw view does not accept a frame offset")]
    ReceiptOffsetNotAllowed,
    /// The cursor lies outside retained bytes.
    #[error("raw view cursor is outside retained evidence")]
    RawCursorOutsideEvidence,
    /// The next cursor cannot be represented.
    #[error("raw view cursor overflow")]
    RawCursorOverflow,
    /// A retained frame sequence is invalid.
    #[error("stored evidence is corrupt")]
    Corruption,
}
impl From<analyte_store::WorkerError> for RawError {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Evidence(error.into())
    }
}
const RAW_PAGE: usize = 4096;
fn raw_form(
    html: &mut Markup,
    id: i64,
    session: &Session,
    target: &str,
    source: &str,
    cursor: RawCursor,
    reason: &str,
) {
    html.push_fragment(
        &Markup::new()
            .markup("<p>Raw source: ")
            .text(source)
            .markup(". Target: ")
            .text(target)
            .markup(". Cursor: ")
            .text(cursor.after.to_string())
            .markup(", ")
            .text(cursor.offset.to_string())
            .markup(".</p><form action=\"/parked/")
            .text(id)
            .markup(concat!(
                "/raw\" method=\"post\"><input type=\"hidden\" ",
                "name=\"csrf\" value=\"",
            ))
            .text(session.csrf.form_value())
            .markup("\"><input type=\"hidden\" name=\"target\" value=\"")
            .text(target)
            .markup("\"><input type=\"hidden\" name=\"source\" value=\"")
            .text(source)
            .markup("\"><input type=\"hidden\" name=\"after\" value=\"")
            .text(cursor.after.to_string())
            .markup("\"><input type=\"hidden\" name=\"offset\" value=\"")
            .text(cursor.offset.to_string())
            .markup(concat!(
                "\"><label>Reason (optional) <input name=\"reason\" ",
                "maxlength=\"256\" value=\"",
            ))
            .text(reason)
            .markup("\"></label><button>Show raw (audited)</button></form>"),
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Receipt,
    Frames,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct RawForm {
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

fn read_raw(s: &Store, form: &RawForm) -> Result<Option<RawPage>, RawError> {
    match form.source {
        Source::Receipt => {
            if form.offset != 0 {
                return Err(RawError::ReceiptOffsetNotAllowed);
            }
            let Some(id) = receipt_id(&form.target) else {
                return Ok(None);
            };
            let Some(receipt) = s.receipt(id)? else {
                return Ok(None);
            };
            let after = usize::try_from(form.after).map_err(RawError::Integer)?;
            let Some(tail) = receipt.raw.get(after..) else {
                return Err(RawError::RawCursorOutsideEvidence);
            };
            let bytes: Vec<u8> = tail.iter().take(RAW_PAGE).copied().collect();
            let next = after
                .checked_add(bytes.len())
                .ok_or(RawError::RawCursorOverflow)?;
            Ok(Some(RawPage {
                bytes,
                next: if next < receipt.raw.len() {
                    Some(RawCursor {
                        after: u64::try_from(next).map_err(RawError::Integer)?,
                        offset: 0,
                    })
                } else {
                    None
                },
            }))
        }
        Source::Frames => {
            let after = i64::try_from(form.after).map_err(RawError::Integer)?;
            let frames = s.frames(&form.target, after)?;
            let Some(frame) = frames.first() else {
                return Ok(None);
            };
            let offset = usize::try_from(form.offset).map_err(RawError::Integer)?;
            let tail = frame
                .raw
                .get(offset..)
                .ok_or(RawError::RawCursorOutsideEvidence)?;
            let bytes: Vec<_> = tail.iter().take(RAW_PAGE).copied().collect();
            let end = offset
                .checked_add(bytes.len())
                .ok_or(RawError::RawCursorOverflow)?;
            Ok(Some(RawPage {
                bytes,
                next: Some(if end < frame.raw.len() {
                    RawCursor {
                        after: form.after,
                        offset: u64::try_from(end).map_err(RawError::Integer)?,
                    }
                } else {
                    RawCursor {
                        after: u64::try_from(frame.sequence).map_err(|_| RawError::Corruption)?,
                        offset: 0,
                    }
                }),
            }))
        }
    }
}

pub(in crate::admin) async fn raw(
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
                .request::<_, RawError>(deadline()?, move |s| {
                    let Some(row) = s.parked_item(id)? else {
                        return Ok(None);
                    };
                    let evidence = row.details().map_err(analyte_store::OperatorError::from)?;
                    if !evidence.targets.contains(&form.target) {
                        return Err(analyte_store::OperatorError::AuditMismatch.into());
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
            html.push_fragment(
                &Markup::new()
                    .markup("<p>Target: ")
                    .text(target.as_str())
                    .markup(". Actor: ")
                    .text(session.actor.as_str())
                    .markup(".</p><pre>"),
            );
            render_raw_bytes(&mut html, &page.bytes);
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

fn render_raw_bytes(html: &mut Markup, bytes: &[u8]) {
    for chunk in bytes.chunks(16) {
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
        html.push_text(&hex);
        html.push_markup(" | ");
        html.push_text(&text);
        html.push_markup("\n");
    }
    html.push_markup(concat!(
        "</pre><p>Nonprinting and non-ASCII bytes are ",
        "shown as dots; hex preserves each byte ",
        "exactly.</p>",
    ));
}

pub(super) fn raw_links(
    html: &mut Markup,
    id: i64,
    evidence: &Parked,
    session: Option<Extension<Arc<Session>>>,
) {
    if let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) {
        for target in &evidence.targets {
            if let Some(receipt) = receipt_id(target) {
                let receipt = uuid::Uuid::from(receipt);
                html.push_fragment(
                    &Markup::new()
                        .markup("<p><a href=\"/receipts/")
                        .text(receipt)
                        .markup("/departures\">Interpretation departure evidence</a></p>"),
                );
                raw_form(
                    html,
                    id,
                    &session,
                    target.as_str(),
                    "receipt",
                    RawCursor::default(),
                    "",
                );
            }
            raw_form(
                html,
                id,
                &session,
                target.as_str(),
                "frames",
                RawCursor::default(),
                "",
            );
        }
    } else {
        html.push_markup(concat!(
            "<p><a href=\"/login\">Log in</a> to authorize an ",
            "audited raw view.</p>",
        ));
    }
}
