//! Shows enabled configuration separately from actual protected receipt interpretation evidence.
use super::{
    pages::{finish, start},
    server::Context,
};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::deadline;
use analyte_model::{DepartureEvidence, DepartureMeaning, ReceiptId};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) fn enabled(config: &analyte_config::Validated) -> Result<Markup, Error> {
    let digest = config
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut html = Markup::new()
        .markup(concat!(
            "<h3>Enabled machine departures</h3><p>Active ",
            "configuration digest: <code>",
        ))
        .text(digest)
        .markup("</code>.</p><ul>");
    for rule in &config.effective().machine().deviations {
        html.push_fragment(
            &Markup::new()
                .markup("<li><b>")
                .text(rule.tag())
                .markup("</b><p>")
                .text(rule.meaning())
                .markup("</p><pre>")
                .text(rule.evidence_snapshot()?.as_str())
                .markup("</pre></li>"),
        );
    }
    html.push_markup(concat!(
        "</ul><p>Enabled is not evidence that a rule ",
        "applied to a receipt.</p>",
    ));
    Ok(html)
}

#[derive(Deserialize)]
pub(super) struct Version {
    #[serde(default = "first")]
    version: i64,
}
fn first() -> i64 {
    1
}

pub(super) async fn receipt(
    State(state): State<Arc<Context>>,
    Path(id): Path<ReceiptId>,
    Query(version): Query<Version>,
) -> Response {
    if version.version < 1 {
        return (
            StatusCode::BAD_REQUEST,
            "positive interpretation version required",
        )
            .into_response();
    }
    let until = match deadline() {
        Ok(until) => until,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let result = state
        .store
        .request(until, move |s| s.interpretation(id, version.version))
        .await;
    let interpretation = match result {
        Ok(Some(value)) => value,
        Ok(None) => return (StatusCode::NOT_FOUND, "interpretation not found").into_response(),
        Err(error) => return finish(Err(error.into())),
    };
    let id = uuid::Uuid::from(id);
    let mut html = start("Receipt interpretation departures");
    html.push_fragment(
        &Markup::new()
            .markup("<p>Receipt ")
            .text(id)
            .markup(". Interpretation ")
            .text(version.version)
            .markup(". Config version: ")
            .text(interpretation.config_version.as_str())
            .markup(". Driver: ")
            .text(interpretation.driver.to_string())
            .markup(".</p>"),
    );
    let digest = analyte_contract::content_digest(&interpretation.config)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    html.push_fragment(
        &Markup::new()
            .markup("<p>Retained config digest: ")
            .text(digest)
            .markup("</p>"),
    );
    match &interpretation.departures {
        Some(evidence) => render(&mut html, evidence),
        None => html.push_markup(concat!(
            concat!(
                "<p>Legacy interpretation: enabled/applied ",
                "departure ledger was not recorded; empty ",
            ),
            "application is not asserted.</p>",
        )),
    }
    if let analyte_store::Reading::Results(results) = &interpretation.outcome {
        html.push_markup("<h2>Source and summary statuses</h2><ul>");
        for part in results.iter().flat_map(|r| r.parts.as_slice()) {
            html.push_fragment(
                &Markup::new()
                    .markup("<li>Received observation status: <code>")
                    .text(part.source_status.as_str())
                    .markup("</code>. Summary: ")
                    .text(part.status)
                    .markup(". Source absence stays empty.</li>"),
            );
        }
        html.push_markup("</ul>");
    } else if let analyte_store::Reading::Failed(reason) = &interpretation.outcome {
        html.push_fragment(
            &Markup::new()
                .markup("<p>Parked interpretation: ")
                .text(reason.as_str())
                .markup("</p>"),
        );
    }
    html.push_fragment(
        &Markup::new()
            .markup("<p><a href=\"/receipts/")
            .text(id)
            .markup("/departures?version=")
            .text(version.version.saturating_add(1))
            .markup("\">Next interpretation version</a></p>"),
    );
    finish(Ok(html))
}

fn render(html: &mut Markup, evidence: &DepartureEvidence) {
    if let Some(reason) = &evidence.incomplete {
        html.push_fragment(
            &Markup::new()
                .markup(concat!(
                    "<p><strong>Applied ledger incomplete; delivery ",
                    "stopped: ",
                ))
                .text(reason.as_str())
                .markup("</strong></p>"),
        );
    }
    html.push_markup("<h2 id=\"enabled\">Enabled declaration snapshot</h2><ul>");
    for declaration in evidence.enabled.as_slice() {
        html.push_fragment(
            &Markup::new()
                .markup("<li><pre>")
                .text(declaration.as_str())
                .markup("</pre></li>"),
        );
    }
    html.push_markup("</ul><h2>Actually applied departures and tolerances</h2>");
    if evidence.applied.as_slice().is_empty() {
        html.push_markup("<p>Applied list: empty.</p>");
    }
    for applied in evidence.applied.as_slice() {
        match applied.meaning {
            DepartureMeaning::FinalReportFromDocumentation
            | DepartureMeaning::FinalObservationFromDocumentation => {
                html.push_markup(concat!(
                    concat!(
                        "<p><strong>Final from documentation — not a ",
                        "received final status. Delivery over ",
                    ),
                    "contract v1 is parked.</strong></p>",
                ));
            }
            DepartureMeaning::ControlCharacterKept => html.push_markup(concat!(
                "<p><strong>Deliberate tolerance — values kept exactly ",
                "as received.</strong></p>",
            )),
            DepartureMeaning::PackedRecords | DepartureMeaning::WireCompatibility => {}
        }
        html.push_fragment(
            &Markup::new()
                .markup("<section><h3>")
                .text(applied.rule.as_str())
                .markup(concat!(
                    "</h3><p><a href=\"#enabled\">Protected evidence ",
                    "references and scope</a></p><p>Source: ",
                ))
                .text(applied.source.as_str())
                .markup(". Derived meaning: ")
                .text(applied.meaning)
                .markup(". Standard: ")
                .text(applied.standard.as_str())
                .markup(". Record ")
                .text(applied.record)
                .markup(", field ")
                .text(applied.field)
                .markup(". Coordinates: ")
                .text(applied.coordinates.as_str())
                .markup(". Spans: ")
                .text(&applied.spans)
                .markup(".</p><pre>")
                .text(applied.declaration.as_str())
                .markup("</pre></section>"),
        );
    }
    html.push_markup(concat!(
        concat!(
            "<p>Manual reference IDs and recording digests ",
            "above identify protected evidence for ",
        ),
        concat!(
            "authorized operators. Original source bytes ",
            "remain in the protected store; raw ",
        ),
        "disclosure requires the audited parked-item action.</p>",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use analyte_model::{
        AppliedDeparture, DepartureEvidence, DepartureMeaning, EvidenceList, Identifier, SourceText,
    };
    // ADR 0015
    #[test]
    fn shows_empty_and_documented_final_without_injection() -> Result<(), Error> {
        let mut html = Markup::new();
        render(&mut html, &DepartureEvidence::default());
        assert!(html.contains("Applied list: empty"));
        assert!(!html.contains("Final from documentation"));
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
        let mut html = Markup::new();
        render(&mut html, &ledger);
        assert!(html.contains("Final from documentation"));
        assert!(html.contains("not a received final status"));
        assert!(html.contains("&lt;script&gt;bad&lt;/script&gt;"));
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
        let mut html = Markup::new();
        render(&mut html, &ledger);
        assert!(html.contains("Deliberate tolerance"));
        assert!(html.contains("astm_control_character_in_text"));
        assert!(html.contains("ControlCharacterKept"));
        assert!(!html.contains("Final from documentation"));
        Ok(())
    }
}
