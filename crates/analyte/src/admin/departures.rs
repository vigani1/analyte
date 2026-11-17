//! Shows enabled configuration separately from actual protected receipt interpretation evidence.
use super::{
    escape,
    pages::{finish, start},
    server::Context,
};
use crate::admin::Error;
use crate::deadline;
use analyte_model::{DepartureEvidence, DepartureMeaning, ReceiptId};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) fn enabled(config: &analyte_config::Validated) -> Result<String, Error> {
    let digest = config
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut html = format!(
        "<h3>Enabled machine departures</h3><p>Active configuration digest: <code>{digest}</code>.</p><ul>"
    );
    for rule in &config.effective().machine().deviations {
        html.push_str(&format!(
            "<li><b>{}</b><p>{}</p><pre>{}</pre></li>",
            escape(rule.tag()),
            escape(rule.meaning()),
            escape(rule.evidence_snapshot()?.as_str())
        ));
    }
    html.push_str("</ul><p>Enabled is not evidence that a rule applied to a receipt.</p>");
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
    let result = state
        .store
        .request(deadline(), move |s| s.interpretation(id, version.version))
        .await;
    let interpretation = match result {
        Ok(Some(value)) => value,
        Ok(None) => return (StatusCode::NOT_FOUND, "interpretation not found").into_response(),
        Err(error) => return finish(Err(error.into())),
    };
    let id = uuid::Uuid::from(id);
    let mut html = start("Receipt interpretation departures");
    html.push_str(&format!(
        "<p>Receipt {id}. Interpretation {}. Config version: {}. Driver: {}.</p>",
        version.version,
        escape(interpretation.config_version.as_str()),
        escape(&interpretation.driver.to_string())
    ));
    let digest = analyte_contract::content_digest(&interpretation.config)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    html.push_str(&format!("<p>Retained config digest: {digest}</p>"));
    match &interpretation.departures {
        Some(evidence)=>render(&mut html,evidence),
        None=>html.push_str("<p>Legacy interpretation: enabled/applied departure ledger was not recorded; empty application is not asserted.</p>"),
    }
    if let analyte_store::Reading::Results(results) = &interpretation.outcome {
        html.push_str("<h2>Source and summary statuses</h2><ul>");
        for part in results.iter().flat_map(|r| r.parts.as_slice()) {
            html.push_str(&format!("<li>Received observation status: <code>{}</code>. Summary: {}. Source absence stays empty.</li>",escape(part.source_status.as_str()),part.status));
        }
        html.push_str("</ul>");
    } else if let analyte_store::Reading::Failed(reason) = &interpretation.outcome {
        html.push_str(&format!(
            "<p>Parked interpretation: {}</p>",
            escape(reason.as_str())
        ));
    }
    html.push_str(&format!(
        "<p><a href=\"/receipts/{id}/departures?version={}\">Next interpretation version</a></p>",
        version.version.saturating_add(1)
    ));
    finish(Ok(html))
}

fn render(html: &mut String, evidence: &DepartureEvidence) {
    if let Some(reason) = &evidence.incomplete {
        html.push_str(&format!(
            "<p><strong>Applied ledger incomplete; delivery stopped: {}</strong></p>",
            escape(reason.as_str())
        ));
    }
    html.push_str("<h2 id=\"enabled\">Enabled declaration snapshot</h2><ul>");
    for declaration in evidence.enabled.as_slice() {
        html.push_str(&format!(
            "<li><pre>{}</pre></li>",
            escape(declaration.as_str())
        ));
    }
    html.push_str("</ul><h2>Actually applied departures</h2>");
    if evidence.applied.as_slice().is_empty() {
        html.push_str("<p>Applied list: empty.</p>");
    }
    for applied in evidence.applied.as_slice() {
        if matches!(
            applied.meaning,
            DepartureMeaning::FinalReportFromDocumentation
                | DepartureMeaning::FinalObservationFromDocumentation
        ) {
            html.push_str("<p><strong>Final from documentation — not a received final status. Delivery over contract v1 is parked.</strong></p>");
        }
        html.push_str(&format!("<section><h3>{}</h3><p><a href=\"#enabled\">Protected evidence references and scope</a></p><p>Source: {}. Derived meaning: {}. Standard: {}. Record {}, field {}. Coordinates: {}. Spans: {}.</p><pre>{}</pre></section>",
            escape(applied.rule.as_str()),escape(applied.source.as_str()),applied.meaning,escape(applied.standard.as_str()),applied.record,applied.field,escape(applied.coordinates.as_str()),applied.spans,escape(applied.declaration.as_str())));
    }
    html.push_str("<p>Manual reference IDs and recording digests above identify protected evidence for authorized operators. Original source bytes remain in the protected store; raw disclosure requires the audited parked-item action.</p>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use analyte_model::{
        AppliedDeparture, DepartureEvidence, DepartureMeaning, EvidenceList, Identifier, SourceText,
    };
    #[test]
    fn adr0015_admin_distinguishes_empty_applied_and_documented_final_without_html_injection()
    -> Result<(), Error> {
        let mut html = String::new();
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
        let mut html = String::new();
        render(&mut html, &ledger);
        assert!(html.contains("Final from documentation"));
        assert!(html.contains("not a received final status"));
        assert!(html.contains("&lt;script&gt;bad&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
        Ok(())
    }
}
