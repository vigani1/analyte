//! Renders the Overview states a test server cannot reach cheaply: each read the error page names, its plain fallback, and orders, notices, disk and alerts in every form.
use super::attention::Facts;
use super::page::{AlertRow, Disk, ErrorPage, Notice, Page, Verdict};
use super::{Failure, Reading, alert_row, error_text, fallback, orders};
use crate::admin::view::{
    Tone,
    frame::{Frame, Sidebar},
};
use analyte_model::Identifier;
use analyte_store::{AlertKey, AlertState};
use askama::Template as _;

/// Every reading, in the order the overview reads.
const READINGS: [Reading; 14] = [
    Reading::Store,
    Reading::Restore,
    Reading::Disk,
    Reading::Review,
    Reading::Delivered,
    Reading::Failing,
    Reading::Alerts,
    Reading::Orders,
    Reading::Receipts,
    Reading::LabApps,
    Reading::Kept,
    Reading::Relays,
    Reading::Page,
    Reading::Time,
];

fn frame() -> Frame {
    Frame {
        title: "Overview",
        assets: "/static/0123456789abcdef/".into(),
        sidebar: Sidebar {
            page_time: "10:45:12".into(),
            main: Vec::new(),
            more: Vec::new(),
            more_open: false,
            csrf: None,
        },
    }
}

// Owner request 2026-11-19 (overview states): when a read fails or the reads
// take too long, the error page says in plain words which read it was, what to
// do, and keeps the exact reason for support; a timeout answers 504, any other
// failure 503.
#[test]
fn error_page_names_the_read_that_failed() -> Result<(), askama::Error> {
    for reading in READINGS {
        let html = ErrorPage {
            frame: frame(),
            text: error_text(reading),
            detail: vec!["the exact reason".into()],
        }
        .render()?;
        assert!(html.contains("<div class=\"alert alert--bad\" role=\"note\">"));
        assert!(
            html.contains("<p class=\"alert__title\">The overview could not be shown just now</p>")
        );
        assert!(
            html.contains(&format!(
                "<p class=\"alert__text\">{} Reload the page; if this keeps happening, give the details below to support.</p>",
                reading.sentence().replace('\'', "&#39;")
            )),
            "{reading:?}\n{html}"
        );
        assert!(html.contains("Details for support</summary><pre>the exact reason\n</pre>"));
        let failure = Failure {
            reading,
            error: None,
        };
        let expected = if reading == Reading::Time { 504 } else { 503 };
        assert_eq!(failure.status().as_u16(), expected, "{reading:?}");
    }
    let sentences: std::collections::BTreeSet<&str> =
        READINGS.iter().map(|reading| reading.sentence()).collect();
    assert_eq!(
        sentences.len(),
        READINGS.len(),
        "each read has its own words"
    );
    Ok(())
}

// Owner request 2026-11-19 (overview states): should the error page itself
// fail to render, the plain refusal keeps both reasons.
#[tokio::test]
async fn failed_error_page_falls_back_to_the_plain_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    let response = fallback(
        &Failure {
            reading: Reading::Time,
            error: None,
        },
        &askama::Error::Fmt,
    );
    assert_eq!(response.status().as_u16(), 504);
    let body = axum::body::to_bytes(response.into_body(), 1 << 16).await?;
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("The overview could not be shown just now."));
    assert!(
        body.contains("admin overview deadline of 1500 ms exceeded; the error page failed too: ")
    );
    assert!(body.contains("admin.css"));
    Ok(())
}

fn alert(key: AlertKey, raised_at: Option<i64>, value: Option<u64>) -> AlertState {
    AlertState {
        key,
        responder: "Duty officer".into(),
        active: true,
        raised_at,
        cleared_at: None,
        evaluated_at: 100,
        value,
    }
}

fn page(facts: &Facts, enough: bool, alerts: Vec<AlertRow>, relays: Vec<String>) -> Page {
    Page {
        frame: frame(),
        today: "Thursday 19 November · 10:45".into(),
        counts: "0 machines · 0 lab apps".into(),
        verdict: Verdict::of(&[]),
        machines: Vec::new(),
        labs: Vec::new(),
        figures: Vec::new(),
        orders: orders(facts),
        restore: facts.restore_paused.then(|| Notice {
            tone: Tone::Attention,
            title: "Order sending is paused after a restore".into(),
            text: crate::admin::view::format::RESTORE_ADVICE.into(),
        }),
        kept: vec![
            Notice {
                tone: Tone::Problem,
                title: "Cannot send to lab app old-lab: it is not set up any more.".into(),
                text: String::new(),
            },
            Notice {
                tone: Tone::Attention,
                title: "Removed machine m: 1 result or order message wait for lab app demo.".into(),
                text: String::new(),
            },
        ],
        disk: Disk {
            free: "512 bytes free".into(),
            free_exact: "512 bytes free".into(),
            reserve: "1 GB".into(),
            reserve_exact: "1073741824 bytes".into(),
            enough,
        },
        alerts,
        alert_messages: vec!["The lab app keys are missing.".into()],
        relays,
    }
}

// Owner request 2026-11-19 (overview states): the Orders card is green with
// "No …" at zero and otherwise counts in its own tone, each row linking to the
// page that lists the orders; a paused restore adds its box below.
#[test]
fn orders_rows_read_zero_and_counts_with_their_links() -> Result<(), askama::Error> {
    let facts = Facts {
        uncertain: 2,
        restore_paused: true,
        ..Facts::default()
    };
    let html = page(&facts, true, Vec::new(), Vec::new()).render()?;
    let orders = html
        .split_once("<ul class=\"card rows\">")
        .and_then(|(_, rest)| rest.split_once("</ul>"))
        .map(|(rows, _)| rows)
        .unwrap_or_default();
    let rows: Vec<&str> = orders.split("<li class=\"row\">").skip(1).collect();
    assert_eq!(rows.len(), 2, "{orders}");
    let uncertain = rows.first().copied().unwrap_or_default();
    assert!(uncertain.contains("tone-warn"), "{uncertain}");
    assert!(uncertain.contains(
        "<span class=\"row__main\">2 orders with an unknown outcome (check the machine)</span><a class=\"link\" href=\"/orders\">See machine orders</a>"
    ));
    let cancelled = rows.get(1).copied().unwrap_or_default();
    assert!(cancelled.contains("tone-good"), "{cancelled}");
    assert!(cancelled.contains(
        "<span class=\"row__main\">No orders cancelled by a machine</span><a class=\"link\" href=\"/analyzer-cancelled\">Review cancelled orders</a>"
    ));
    assert!(html.contains("<div class=\"alert alert--warn\" role=\"note\">"));
    assert!(html.contains("<p class=\"alert__title\">Order sending is paused after a restore</p>"));
    Ok(())
}

// Owner request 2026-11-19 (overview states): with no machine and no lab app
// the path shows an empty state in each column, no figures and the key to the
// lines; results kept for removed machines show as boxes in their tone.
#[test]
fn empty_path_and_kept_results_use_design_components() -> Result<(), askama::Error> {
    let html = page(&Facts::default(), true, Vec::new(), Vec::new()).render()?;
    assert!(html.contains(concat!(
        "<div class=\"empty\"><p class=\"empty__title\">No machine is running</p>",
        "<p class=\"empty__text\">Add or set one up on the Setup page.</p>",
        "<a class=\"btn btn--outline btn--sm\" href=\"/setup\">Setup</a></div>"
    )));
    assert!(html.contains(concat!(
        "<div class=\"empty\"><p class=\"empty__title\">No lab app</p>",
        "<p class=\"empty__text\">No running machine sends to a lab app.</p></div>"
    )));
    assert!(!html.contains("class=\"figures"));
    assert!(html.contains(concat!(
        "<ul class=\"legend\" aria-label=\"Line key\"><li><span class=\"legend__line\"></span>Link open</li>",
        "<li><span class=\"legend__line legend__line--wait\"></span>Waiting</li>",
        "<li><span class=\"legend__line legend__line--cut\"></span>Broken</li></ul>"
    )));
    assert!(html.contains(
        "<div class=\"alert alert--bad\" role=\"note\"><svg class=\"icon \" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\""
    ));
    assert!(html.contains(
        "<p class=\"alert__title\">Cannot send to lab app old-lab: it is not set up any more.</p>"
    ));
    assert!(html.contains("<div class=\"alert alert--warn\" role=\"note\">"));
    Ok(())
}

// Owner request 2026-11-19 (overview states): disk and alerts show free space
// and the reserve with exact bytes on hover, "Enough space" or "Almost full:
// receiving refused", the active alerts as a table (with "Not known" where the
// rule recorded nothing), each alert's reason and fix, and the relay lines for
// support only when relays exist.
#[test]
fn disk_and_alerts_cover_full_disk_unknown_values_and_relays() -> Result<(), askama::Error> {
    let now = jiff::Timestamp::from_second(1_795_046_077).map_err(|_| askama::Error::Fmt)?;
    let silent =
        AlertKey::MachineSilent(Identifier::new("cobas-e411").map_err(|_| askama::Error::Fmt)?);
    let rows = vec![
        alert_row(&alert(silent, None, None), now),
        alert_row(
            &alert(AlertKey::DiskReserve, Some(now.as_second() - 60), Some(512)),
            now,
        ),
    ];
    let html = page(
        &Facts::default(),
        false,
        rows,
        vec!["alert_endpoint=relay state=ok".into()],
    )
    .render()?;
    assert!(html.contains(concat!(
        "<dl class=\"kv\">\n<dt>Free disk space</dt><dd><span class=\"num\" title=\"512 bytes free\">512 bytes free</span></dd>\n",
        "<dt>Always kept free</dt><dd><span class=\"num\" title=\"1073741824 bytes\">1 GB</span> <span class=\"muted\">Analyte stops receiving below this</span></dd>"
    )), "{html}");
    assert!(html.contains("<dt>Disk status</dt><dd><span class=\"status status--bad\">"));
    assert!(html.contains("Almost full: receiving refused</span></dd>"));
    assert!(html.contains("<dt>Active alerts</dt><dd class=\"num\">2</dd>"));
    assert!(html.contains("<dt>Alert relays</dt><dd class=\"num\">1</dd>"));
    assert!(html.contains(concat!(
        "<div class=\"card table-wrap\">\n<table class=\"table\">\n<thead><tr><th scope=\"col\">Alert</th>",
        "<th scope=\"col\">Machine</th><th scope=\"col\">Responder</th><th scope=\"col\">Raised</th>",
        "<th scope=\"col\" class=\"right\">Last value</th></tr></thead>"
    )));
    assert!(html.contains(concat!(
        "<tr><td class=\"cell-main\"><span title=\"machine_silent\">Machine has gone quiet</span></td>",
        "<td>cobas-e411</td><td>Duty officer</td><td>Not known</td><td class=\"num right\">Not known</td></tr>"
    )), "{html}");
    assert!(html.contains("<td>—</td><td>Duty officer</td><td><time datetime="));
    assert!(html.contains("<td class=\"num right\">512</td></tr>"));
    assert!(html.contains("<p class=\"alert__title\">The lab app keys are missing.</p>"));
    assert!(
        html.contains(
            "Relay status for support</summary><pre>alert_endpoint=relay state=ok\n</pre>"
        )
    );

    let calm = page(&Facts::default(), true, Vec::new(), Vec::new()).render()?;
    assert!(calm.contains("<span class=\"status status--good\">"));
    assert!(calm.contains("Enough space</span>"));
    assert!(calm.contains("<dt>Active alerts</dt><dd class=\"num\">0</dd>"));
    assert!(!calm.contains("<table"));
    assert!(!calm.contains("Relay status for support"));
    Ok(())
}
