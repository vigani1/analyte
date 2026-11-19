//! Shows enabled configuration separately from actual protected receipt interpretation evidence.
use super::finish;
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::{
    server::Context,
    view::{self, Tab, format},
};
use crate::deadline;
use analyte_model::{DepartureEvidence, DepartureMeaning, ReceiptId};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use serde::Deserialize;
use std::sync::Arc;

pub(in crate::admin) fn enabled(config: &analyte_config::Validated) -> Result<Markup, Error> {
    let mut html = Markup::new()
        .markup("<dl><dt title=\"Active configuration digest\">Settings version</dt><dd>")
        .fragment(&view::version(&config.digest()))
        .markup(concat!(
            "</dd></dl><h3 title=\"Enabled machine departures\">Deliberate exceptions ",
            "for this machine</h3><p class=\"muted\">Places where this machine's settings ",
            "knowingly accept something the standard does not. Being switched on does not mean ",
            "an exception was used for a given message; each message's details show which ",
            "ones were.</p>",
        ));
    let rules = &config.effective().machine().deviations;
    if rules.is_empty() {
        html.push_markup("<p>None</p>");
        return Ok(html);
    }
    html.push_markup("<ul>");
    for rule in rules {
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
    html.push_markup("</ul>");
    Ok(html)
}

#[derive(Deserialize)]
pub(in crate::admin) struct Version {
    #[serde(default = "first")]
    version: i64,
}
fn first() -> i64 {
    1
}

pub(in crate::admin) async fn receipt(
    State(state): State<Arc<Context>>,
    Path(id): Path<ReceiptId>,
    Query(version): Query<Version>,
) -> Response {
    if version.version < 1 {
        return view::refusal(
            StatusCode::BAD_REQUEST,
            "Reading not found",
            "Readings are numbered from 1. Open the message again from the Machines page.",
            "positive interpretation version required",
        );
    }
    let until = match deadline() {
        Ok(until) => until,
        Err(error) => return finish(Err(error.into())),
    };
    let result = state
        .store
        .request(until, move |s| s.interpretation(id, version.version))
        .await;
    let interpretation = match result {
        Ok(Some(value)) => value,
        Ok(None) => {
            return view::refusal(
                StatusCode::NOT_FOUND,
                "Reading not found",
                "This message has no reading with this number. Earlier readings stay listed from the Machines page.",
                "interpretation not found",
            );
        }
        Err(error) => return finish(Err(error.into())),
    };
    let id = uuid::Uuid::from(id);
    let mut html = view::page(Tab::Machines, "How this message was read");
    html.push_fragment(&summary(id, version.version, &interpretation));
    match &interpretation.departures {
        Some(evidence) => render(&mut html, evidence),
        None => html.push_markup(concat!(
            "<p class=\"muted\" title=\"Legacy interpretation: departure ledger not ",
            "recorded\">This message was read by an older version that did not record which ",
            "exceptions were switched on or used; that does not mean none were.</p>",
        )),
    }
    html.push_fragment(&outcome(&interpretation.outcome));
    html.push_fragment(
        &Markup::new()
            .markup("<p class=\"actions-row\"><a class=\"button secondary\" href=\"/receipts/")
            .text(id)
            .markup("/departures?version=")
            .text(version.version.saturating_add(1))
            .markup("\" title=\"Next interpretation version\">Next reading</a></p>"),
    );
    finish(Ok(html))
}

/// The message, reading and settings a receipt page is about.
fn summary(id: uuid::Uuid, version: i64, interpretation: &analyte_store::Interpretation) -> Markup {
    Markup::new()
        .markup("<section><dl>")
        .fragment(&view::row(
            "Message",
            Some("Receipt"),
            &Markup::new().markup("<code>").text(id).markup("</code>"),
        ))
        .fragment(&view::row(
            "Reading",
            Some("Interpretation version"),
            &Markup::new().text(version),
        ))
        .fragment(&view::row(
            "Settings version",
            Some("Retained config digest"),
            &view::version(&analyte_contract::content_digest(&interpretation.config)),
        ))
        .fragment(&view::row(
            "Settings label",
            Some("Config version"),
            &Markup::new().text(interpretation.config_version.as_str()),
        ))
        .fragment(&view::row(
            "Read by",
            Some("Driver"),
            &Markup::new().text(interpretation.driver.to_string()),
        ))
        .markup("</dl></section>")
}

/// What the reading produced: each result part's status as sent and as shown, or why it failed.
fn outcome(outcome: &analyte_store::Reading) -> Markup {
    let mut html = Markup::new();
    if let analyte_store::Reading::Results(results) = outcome {
        html.push_markup(
            "<section><h2 title=\"Source and summary statuses\">Result statuses</h2><ul>",
        );
        for part in results.iter().flat_map(|r| r.parts.as_slice()) {
            html.push_fragment(
                &Markup::new()
                    .markup("<li title=\"Received observation status; source absence stays empty\">Status sent by the machine: <code>")
                    .text(part.source_status.as_str())
                    .markup("</code> (empty when none was sent). Shown as: ")
                    .text(part.status)
                    .markup(".</li>"),
            );
        }
        html.push_markup("</ul></section>");
    } else if let analyte_store::Reading::Failed(reason) = outcome {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice\" title=\"Parked interpretation\">Could not be read: ")
                .fragment(&format::reason(reason.as_str()))
                .markup("</p>"),
        );
    }
    html
}

/// One exception used for a message: what was received, how it was read and where.
fn applied_section(applied: &analyte_model::AppliedDeparture) -> Markup {
    Markup::new()
        .markup("<section><h3 title=\"Rule\">")
        .text(applied.rule.as_str())
        .markup("</h3><dl>")
        .fragment(&view::row(
            "As received",
            Some("Source"),
            &Markup::new().text(applied.source.as_str()),
        ))
        .fragment(&view::row(
            "Read as",
            Some("Derived meaning"),
            &Markup::new().text(applied.meaning),
        ))
        .fragment(&view::row(
            "Standard",
            None,
            &Markup::new().text(applied.standard.as_str()),
        ))
        .fragment(&view::row(
            "Where in the message",
            Some("Record and field; coordinates and spans on hover"),
            &Markup::new()
                .markup("<span title=\"Coordinates: ")
                .text(applied.coordinates.as_str())
                .markup("; spans: ")
                .text(&applied.spans)
                .markup("\">Record ")
                .text(applied.record)
                .markup(", field ")
                .text(applied.field)
                .markup("</span>"),
        ))
        .markup(concat!(
            "</dl><p><a href=\"#enabled\" title=\"Protected evidence references and ",
            "scope\">Where this exception comes from</a></p><pre>",
        ))
        .text(applied.declaration.as_str())
        .markup("</pre></section>")
}

fn render(html: &mut Markup, evidence: &DepartureEvidence) {
    if let Some(reason) = &evidence.incomplete {
        html.push_fragment(
            &Markup::new()
                .markup(concat!(
                    "<p class=\"notice bad\" title=\"Applied ledger incomplete\"><strong>Not ",
                    "sent to the lab app: the record of exceptions used is incomplete. ",
                ))
                .fragment(&format::reason(reason.as_str()))
                .markup("</strong></p>"),
        );
    }
    html.push_markup(concat!(
        "<section><h2 id=\"enabled\" title=\"Enabled declaration snapshot\">Exceptions ",
        "switched on for this machine</h2><ul>",
    ));
    for declaration in evidence.enabled.as_slice() {
        html.push_fragment(
            &Markup::new()
                .markup("<li><pre>")
                .text(declaration.as_str())
                .markup("</pre></li>"),
        );
    }
    if evidence.enabled.as_slice().is_empty() {
        html.push_markup("<li>None</li>");
    }
    html.push_markup(concat!(
        "</ul></section><h2 title=\"Actually applied departures and tolerances\">Exceptions ",
        "used for this message</h2>",
    ));
    if evidence.applied.as_slice().is_empty() {
        html.push_markup("<p title=\"Applied list: empty\">None were used.</p>");
    }
    for applied in evidence.applied.as_slice() {
        match applied.meaning {
            DepartureMeaning::FinalReportFromDocumentation
            | DepartureMeaning::FinalObservationFromDocumentation => {
                html.push_markup(concat!(
                    "<p class=\"notice\" title=\"Final from documentation; delivery over ",
                    "contract v1 is parked\"><strong>Marked final because the manual says so, ",
                    "not because the machine sent a final status.</strong> It waits on the Needs ",
                    "review page instead of going to the lab app.</p>",
                ));
            }
            DepartureMeaning::ControlCharacterKept
            | DepartureMeaning::CharsetFromDefinition
            | DepartureMeaning::ZoneFromSite => {
                html.push_markup(concat!(
                    "<p title=\"Deliberate tolerance\"><strong>Accepted on purpose: values ",
                    "are kept exactly as received.</strong></p>",
                ));
            }
            DepartureMeaning::PackedRecords | DepartureMeaning::WireCompatibility => {}
        }
        html.push_fragment(&applied_section(applied));
    }
    html.push_markup(concat!(
        "<p class=\"muted\">The references above point to the manual sections and recordings ",
        "behind each exception. The original bytes stay in the protected store; to see them, ",
        "use Show original bytes on an item in Needs review.</p>",
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
        assert!(html.contains("None were used."));
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
        let mut html = Markup::new();
        render(&mut html, &ledger);
        assert!(html.contains("Marked final because the manual says so"));
        assert!(html.contains("not because the machine sent a final status"));
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
        assert!(html.contains("Accepted on purpose: values are kept exactly as received."));
        assert!(html.contains("astm_control_character_in_text"));
        assert!(html.contains("ControlCharacterKept"));
        assert!(!html.contains("Marked final because the manual says so"));
        Ok(())
    }
}
