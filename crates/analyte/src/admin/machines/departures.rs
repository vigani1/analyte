//! Serves how one stored message was read (GET /receipts/{id}/departures): the reading, the accepted differences from the standard (departures) switched on and used for it, and each result part's status as sent and as shown.
//!
//! Switched on is not the same as used: a machine's settings switch a
//! departure on, and each reading records which ones it used for those bytes.
//! The page shows both from the stored reading, never from today's settings.
use super::page::{Applied, Departures, Fact, Notice, Value};
use crate::admin::error::CommandError;
use crate::admin::server::Context;
use crate::admin::sessions::Session;
use crate::admin::view::report::{Button, Crumb, Head, Problem, Section, respond};
use crate::admin::view::{self, Tab, Tone, format, frame::Frame};
use crate::deadline;
use analyte_model::{DepartureEvidence, DepartureMeaning, ReceiptId, ReceiptMetadata};
use analyte_store::{Interpretation, OperatorError, Reading};
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

/// Which reading of the message to show; the first one when none is asked for.
#[derive(Deserialize)]
pub(in crate::admin) struct Version {
    #[serde(default = "first")]
    version: i64,
}

const fn first() -> i64 {
    1
}

/// What the page reads in one store request.
struct Read {
    interpretation: Interpretation,
    /// The message's metadata; none once data keeping removed it.
    metadata: Option<ReceiptMetadata>,
    /// A later reading exists.
    next: bool,
}

pub(in crate::admin) async fn receipt(
    State(state): State<Arc<Context>>,
    Path(id): Path<ReceiptId>,
    Query(version): Query<Version>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let version = version.version;
    if version < 1 {
        return Problem::ReadingNumber.page(&state, session, Section::Machines(None));
    }
    let now = Timestamp::now();
    match tokio::time::timeout(view::READ_BUDGET, read(&state, id, version)).await {
        Ok(Ok(Some(read))) => {
            let machine = read
                .metadata
                .as_ref()
                .map(|metadata| metadata.machine.as_str().to_owned());
            let html = askama::Template::render(&page(&state, session, id, version, read, now));
            respond(
                &state,
                session,
                Section::Machines(machine.as_deref()),
                StatusCode::OK,
                html,
            )
        }
        Ok(Ok(None)) => Problem::ReadingNotFound.page(&state, session, Section::Machines(None)),
        Ok(Err(error)) => {
            Problem::Store(error.to_string()).page(&state, session, Section::Machines(None))
        }
        Err(_) => Problem::Time.page(&state, session, Section::Machines(None)),
    }
}

async fn read(
    state: &Context,
    id: ReceiptId,
    version: i64,
) -> Result<Option<Read>, crate::admin::Error> {
    Ok(state
        .store
        .request::<_, CommandError>(deadline()?, move |store| {
            let Some(interpretation) = store.interpretation(id, version)? else {
                return Ok(None);
            };
            let metadata = match store.receipt_metadata(id) {
                Ok(metadata) => metadata,
                Err(OperatorError::Purged) => None,
                Err(error) => return Err(error.into()),
            };
            let next = match version.checked_add(1) {
                Some(next) => store.interpretation(id, next)?.is_some(),
                None => false,
            };
            Ok(Some(Read {
                interpretation,
                metadata,
                next,
            }))
        })
        .await?)
}

fn page(
    state: &Context,
    session: Option<&Session>,
    id: ReceiptId,
    version: i64,
    read: Read,
    now: Timestamp,
) -> Departures {
    let Read {
        interpretation,
        metadata,
        next,
    } = read;
    let title = "How this message was read";
    let mut crumbs = vec![Crumb {
        label: Tab::Machines.label().to_owned(),
        href: Some(Tab::Machines.href().to_owned()),
    }];
    crumbs.extend(metadata.as_ref().map(|metadata| Crumb {
        label: metadata.machine.as_str().to_owned(),
        href: Some(format::machine_href(metadata.machine.as_str())),
    }));
    let digest = analyte_contract::content_digest(&interpretation.config);
    let Outcome {
        notices,
        stored,
        statuses,
    } = outcome(&interpretation);
    let links = readings(id, version, next);
    let id = uuid::Uuid::from(id);
    Departures {
        frame: Frame::new(state, Tab::Machines, title, None, session, now),
        head: Head {
            crumbs,
            title: title.into(),
            status: None,
            lede: metadata.as_ref().map(|metadata| {
                format::arrived(metadata.machine.as_str(), metadata.received_at, now)
                    .then(format::Line::text(format!("reading {version}")))
            }),
        },
        notices,
        facts: vec![
            Fact {
                term: "Reading",
                value: Value::Text(version.to_string()),
            },
            Fact {
                term: "Read by",
                value: Value::Text(format::driver(&interpretation.driver)),
            },
            Fact {
                term: "Settings label",
                value: Value::Text(interpretation.config_version.as_str().to_owned()),
            },
            Fact {
                term: "Settings version",
                value: Value::Code {
                    shown: format::digest_short(&digest),
                    exact: format::digest_hex(&digest),
                },
            },
        ],
        enabled: interpretation.departures.as_ref().map(enabled),
        applied: interpretation.departures.as_ref().map(applied),
        statuses,
        links,
        support: [
            format!("Message: {id}"),
            format!("Reading: {version}"),
            format!("Read by: {}", interpretation.driver),
            format!("Settings version: {}", format::digest_hex(&digest)),
        ]
        .into_iter()
        .chain(stored)
        .collect(),
    }
}

/// What a reading says beside its facts.
struct Outcome {
    /// An incomplete record of differences, or a reading that failed.
    notices: Vec<Notice>,
    /// Their reasons exactly as stored, for support.
    stored: Vec<String>,
    /// Each result part's status as sent and as shown, when it was read.
    statuses: Option<Vec<(String, String)>>,
}

/// What `interpretation` says beside its facts.
fn outcome(interpretation: &Interpretation) -> Outcome {
    let mut notices = Vec::new();
    let mut stored = Vec::new();
    if let Some(reason) = interpretation
        .departures
        .as_ref()
        .and_then(|evidence| evidence.incomplete.as_ref())
    {
        notices.push(Notice {
            tone: Tone::Problem,
            title: "Not sent to the lab app: the record of differences used is incomplete".into(),
            text: format::reason_words(reason.as_str())
                .unwrap_or_else(|| reason.as_str().to_owned()),
        });
        stored.push(format!(
            "Incomplete record, reason as stored: {}",
            reason.as_str()
        ));
    }
    let statuses = match &interpretation.outcome {
        Reading::Results(results) => Some(
            results
                .iter()
                .flat_map(|result| result.parts.as_slice())
                .map(|part| {
                    (
                        part.source_status.as_str().to_owned(),
                        part.status.to_string(),
                    )
                })
                .collect(),
        ),
        Reading::Failed(reason) => {
            notices.push(Notice {
                tone: Tone::Problem,
                title: "This message could not be read".into(),
                text: format::reason_words(reason.as_str())
                    .unwrap_or_else(|| reason.as_str().to_owned()),
            });
            stored.push(format!("Reason as stored: {}", reason.as_str()));
            None
        }
    };
    Outcome {
        notices,
        stored,
        statuses,
    }
}

/// The links to the earlier and the next reading, each only when it exists.
fn readings(id: ReceiptId, version: i64, next: bool) -> Vec<Button> {
    let id = uuid::Uuid::from(id);
    let href = |version: i64| format!("/receipts/{id}/departures?version={version}");
    let mut links = Vec::new();
    if let Some(earlier) = version.checked_sub(1).filter(|&earlier| earlier >= 1) {
        links.push(Button {
            href: href(earlier),
            label: "Earlier reading",
            class: "btn--outline btn--sm",
        });
    }
    if next && let Some(later) = version.checked_add(1) {
        links.push(Button {
            href: href(later),
            label: "Next reading",
            class: "btn--outline btn--sm",
        });
    }
    links
}

/// Each departure switched on for the reading, as its declaration.
fn enabled(evidence: &DepartureEvidence) -> Vec<String> {
    evidence
        .enabled
        .as_slice()
        .iter()
        .map(|declaration| declaration.as_str().to_owned())
        .collect()
}

/// Each departure used for the message: what was received, how it was read,
/// where, and what that means for the result.
fn applied(evidence: &DepartureEvidence) -> Vec<Applied> {
    evidence
        .applied
        .as_slice()
        .iter()
        .map(|applied| Applied {
            rule: applied.rule.as_str().to_owned(),
            note: note(applied.meaning),
            facts: vec![
                Fact {
                    term: "As received",
                    value: Value::Text(applied.source.as_str().to_owned()),
                },
                Fact {
                    term: "Read as",
                    value: Value::Text(applied.meaning.to_string()),
                },
                Fact {
                    term: "Standard",
                    value: Value::Text(applied.standard.as_str().to_owned()),
                },
                Fact {
                    term: "Where in the message",
                    value: Value::Code {
                        shown: format!("Record {}, field {}", applied.record, applied.field),
                        exact: format!(
                            "Coordinates: {}; spans: {}",
                            applied.coordinates.as_str(),
                            applied.spans
                        ),
                    },
                },
            ],
            declaration: vec![applied.declaration.as_str().to_owned()],
        })
        .collect()
}

/// What a departure means for the result, when it needs saying.
fn note(meaning: DepartureMeaning) -> Option<Notice> {
    match meaning {
        DepartureMeaning::FinalReportFromDocumentation
        | DepartureMeaning::FinalObservationFromDocumentation => Some(Notice {
            tone: Tone::Attention,
            title: concat!(
                "Marked final because the manual says so, not because the machine sent a ",
                "final status"
            )
            .into(),
            text: "It waits on the Needs review page instead of going to the lab app.".into(),
        }),
        DepartureMeaning::ControlCharacterKept
        | DepartureMeaning::CharsetFromDefinition
        | DepartureMeaning::ZoneFromSite => Some(Notice {
            tone: Tone::Good,
            title: "Accepted on purpose: values are kept exactly as received".into(),
            text: String::new(),
        }),
        DepartureMeaning::PackedRecords | DepartureMeaning::WireCompatibility => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use analyte_model::{AppliedDeparture, EvidenceList, Identifier, SourceText};

    /// The departures used for a message on their own, as the page draws them.
    #[derive(askama::Template)]
    #[template(
        source = "{% import \"components/display.html\" as display %}{% call display::departures_used(applied) %}{% endcall %}",
        ext = "html"
    )]
    struct Used {
        applied: Option<Vec<Applied>>,
    }

    fn used(evidence: &DepartureEvidence) -> Result<String, askama::Error> {
        askama::Template::render(&Used {
            applied: Some(applied(evidence)),
        })
    }

    // ADR 0015
    #[test]
    fn shows_empty_and_documented_final_without_injection() -> Result<(), Error> {
        let html = used(&DepartureEvidence::default())?;
        assert!(html.contains("None were used."), "{html}");
        assert!(!html.contains("Marked final because the manual says so"));
        let application = AppliedDeparture {
            rule: Identifier::new("hl7_absent_obx11_means_final")?,
            declaration: SourceText::new("scope = '<script>bad</script>'\nlevel = 'documented'")?,
            standard: Identifier::new("HL7 §7.4.2.11")?,
            record: 3,
            field: 11,
            source: SourceText::new("absent; source unchanged")?,
            meaning: DepartureMeaning::FinalObservationFromDocumentation,
            coordinates: Identifier::new("syntax_bytes")?,
            spans: EvidenceList::default(),
        };
        let ledger = DepartureEvidence {
            enabled: EvidenceList::default(),
            incomplete: None,
            applied: EvidenceList::new(vec![application])?,
        };
        let html = used(&ledger)?;
        assert!(
            html.contains("Marked final because the manual says so"),
            "{html}"
        );
        assert!(html.contains("not because the machine sent a final status"));
        assert!(
            html.contains("&#60;script&#62;bad&#60;/script&#62;"),
            "{html}"
        );
        assert!(!html.contains("<script>"));
        Ok(())
    }

    // Deliberate tolerances are listed with their note for operators.
    // CLSI LIS02-A2 §5.1
    #[test]
    fn shows_tolerance_note() -> Result<(), Error> {
        let note = AppliedDeparture {
            rule: Identifier::new("astm_control_character_in_text")?,
            declaration: SourceText::new("always on: deliberate tolerance")?,
            standard: Identifier::new("LIS02-A2 §5.1")?,
            record: 5,
            field: 4,
            source: SourceText::new("value kept byte for byte")?,
            meaning: DepartureMeaning::ControlCharacterKept,
            coordinates: Identifier::new("syntax_bytes")?,
            spans: EvidenceList::default(),
        };
        let ledger = DepartureEvidence {
            enabled: EvidenceList::default(),
            incomplete: None,
            applied: EvidenceList::new(vec![note])?,
        };
        let html = used(&ledger)?;
        assert!(
            html.contains("Accepted on purpose: values are kept exactly as received"),
            "{html}"
        );
        assert!(html.contains("astm_control_character_in_text"));
        assert!(html.contains("ControlCharacterKept"));
        assert!(!html.contains("Marked final because the manual says so"));
        Ok(())
    }
}
