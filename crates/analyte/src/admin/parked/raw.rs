//! Shows bounded original bytes of an item's linked record (POST /parked/{id}/raw), only after an authenticated session's view is recorded durably.
//!
//! The reason someone gives is optional and recorded with the view; the page
//! warns that the bytes may contain patient data and keeps the record's ID and
//! the session's ID under "Details for support".
use super::page::{Head, Raw, RawRequest};
use super::problem::Problem;
use super::{receipt_id, respond};
use crate::admin::view::list::cursor_value;
use crate::admin::view::{Tab, frame::Frame};
use crate::{
    admin::{Error, server::Context, sessions::Session},
    deadline,
};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, OperatorError, ParkedCounts, Store};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;
/// Refusals and boundary failures of an audited raw-evidence page.
#[derive(Debug, thiserror::Error)]
pub enum RawError {
    /// Retained evidence could not be queried.
    #[error(transparent)]
    Evidence(#[from] OperatorError),
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

/// The audit action of every view of original bytes or of a machine's free
/// text; Activity does not list these views as changes.
pub(in crate::admin) const RAW_VIEW: &str = "admin-raw-view";

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

/// What the store answered for a raw view.
enum Shown {
    /// No parked item has this number.
    NoItem,
    /// Nothing follows at this cursor, so nothing was recorded.
    Nothing,
    /// The view was recorded; these bytes may be shown.
    Page(RawPage, ParkedCounts),
}

impl Source {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Frames => "frames",
        }
    }

    /// What the bytes are, as the page names them.
    const fn what(&self) -> &'static str {
        match self {
            Self::Receipt => "The message as stored",
            Self::Frames => "The pieces as they arrived from the machine",
        }
    }
}

pub(in crate::admin) async fn raw(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<RawForm>,
) -> Response {
    if !session.authenticated {
        return Problem::SignIn.page(&state, None, Some(id));
    }
    if !session.csrf_valid(&form.csrf) {
        return Problem::FormExpired.page(&state, Some(&session), Some(id));
    }
    let target = form.target.clone();
    let source = form.source.as_str();
    let what = form.source.what();
    let cursor = RawCursor {
        after: form.after,
        offset: form.offset,
    };
    let typed = form.reason.clone();
    let shown = async {
        let reason = Reason::new(if form.reason.is_empty() {
            "operator requested raw evidence"
        } else {
            &form.reason
        })?;
        let audit = Audit {
            actor: session.actor.clone(),
            action: Identifier::new(RAW_VIEW)?,
            targets: vec![
                Identifier::new(&format!("parked:{id}"))?,
                target.clone(),
                Identifier::new(&format!("{source}-offset:{}:{}", form.after, form.offset))?,
            ],
            time: Timestamp::now().as_second(),
            reason,
        };
        Ok::<_, Error>(
            state
                .store
                .request::<_, RawError>(deadline()?, move |s| {
                    let Some(row) = s.parked_item(id)? else {
                        return Ok(Shown::NoItem);
                    };
                    let evidence = row.details().map_err(OperatorError::from)?;
                    if !evidence.targets.contains(&form.target) {
                        return Err(OperatorError::AuditMismatch.into());
                    }
                    let Some(page) = read_raw(s, &form)? else {
                        return Ok(Shown::Nothing);
                    };
                    s.audit(&audit)?;
                    Ok(Shown::Page(page, s.parked_counts()?))
                })
                .await?,
        )
    }
    .await;
    let (page, counts) = match shown {
        Ok(Shown::Page(page, counts)) => (page, counts),
        Ok(Shown::NoItem) => return Problem::NotFound.page(&state, Some(&session), Some(id)),
        Ok(Shown::Nothing) => return Problem::NothingMore.page(&state, Some(&session), Some(id)),
        Err(Error::Raw(RawError::Evidence(OperatorError::Purged))) => {
            return Problem::Purged.page(&state, Some(&session), Some(id));
        }
        Err(error) => {
            return Problem::of(&error).page(&state, Some(&session), Some(id));
        }
    };
    let view = shown_page(
        &state,
        &session,
        id,
        (target, source, what),
        cursor,
        typed,
        &page,
        counts,
    );
    respond(
        &state,
        Some(&session),
        Some(id),
        StatusCode::OK,
        view.render(),
    )
}

/// The page showing `page`'s bytes of `target`, read from `source` at `cursor`,
/// with the form for the next part carrying the reason typed before.
#[expect(
    clippy::too_many_arguments,
    reason = "each value is one fact the page shows"
)]
fn shown_page(
    state: &Context,
    session: &Session,
    id: i64,
    (target, source, what): (Identifier, &'static str, &'static str),
    cursor: RawCursor,
    typed: String,
    page: &RawPage,
    counts: ParkedCounts,
) -> Raw {
    let continued = cursor.after != 0 || cursor.offset != 0;
    Raw {
        frame: Frame::new(
            state,
            Tab::NeedsReview,
            "Original message",
            Some(counts.open),
            Some(session),
            Timestamp::now(),
        ),
        head: Head::under(Some(id), false, "Original message"),
        what: if continued {
            format!("{what}, continued")
        } else {
            what.to_owned()
        },
        lines: dump(&page.bytes),
        more: page.next.map(|next| RawRequest {
            action: format!("/parked/{id}/raw"),
            csrf: session.csrf.form_value().to_owned(),
            target: target.as_str().to_owned(),
            after: next.after,
            offset: next.offset,
            field: "raw-reason".into(),
            reason: typed,
            buttons: vec![(source, "Show the next part")],
        }),
        support: vec![
            format!("Linked record: {}", target.as_str()),
            format!(
                "Source: {source}, cursor {}, offset {}",
                cursor.after, cursor.offset
            ),
            format!("View recorded for: {}", session.actor.as_str()),
        ],
    }
}

/// Each 16 bytes as one line: every byte exactly in hex, then the same bytes as
/// text, with bytes that are not plain letters or signs shown as dots.
fn dump(bytes: &[u8]) -> Vec<String> {
    bytes
        .chunks(16)
        .map(|chunk| {
            let hex: String = chunk.iter().map(|byte| format!("{byte:02x} ")).collect();
            let text: String = chunk
                .iter()
                .map(|byte| {
                    if byte.is_ascii_graphic() || *byte == b' ' {
                        char::from(*byte)
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{hex:<48} | {text}")
        })
        .collect()
}
