//! Tests that an order item's notes from the machine show only after the audited "Show the original message", which records who asked and why first.
use super::super::*;
use super::Scene;

/// How many times the audit recorded a view of a machine's free text.
fn note_views(database: &std::path::Path) -> Result<i64, Error> {
    Ok(rusqlite::Connection::open(database)?.query_row(
        "SELECT count(*) FROM audit WHERE json_extract(evidence,'$.action')='admin-raw-view' AND json_extract(evidence,'$.targets[1]')='notes'",
        [],
        |row| row.get(0),
    )?)
}

// Owner decision 2026-11-20 (review B6): a machine's notes are free text that
// may hold patient data, so an order item's page hides them until someone
// asks through the audited "Show the original message", as Needs review does
// for original bytes; the view is recorded before anything shows.
#[tokio::test(flavor = "multi_thread")]
async fn order_notes_show_only_after_the_audited_view() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let order = scene.seeded.cancelled;
    let path = format!("/awos/{order}/1");
    let note = "NTE|||Sample &#60;not&#62; found on the rack";
    let hidden = scene.get(&path).await?;
    assert!(hidden.starts_with("HTTP/1.1 200"), "{hidden}");
    assert!(!hidden.contains(note), "{hidden}");
    assert!(!hidden.contains("May contain patient data"));
    for part in [
        "<p class=\"muted text--sm\">1 note from the machine, hidden</p>",
        "Show the original message</summary>",
        "The machine&#39;s notes are hidden because free text may contain patient data.",
        &format!("<form class=\"stack\" method=\"post\" action=\"/awos/{order}/1/raw\">"),
        "<input type=\"hidden\" name=\"target\" value=\"notes\"><input type=\"hidden\" name=\"after\" value=\"0\"><input type=\"hidden\" name=\"offset\" value=\"0\">",
        "<button type=\"submit\" class=\"btn btn--outline btn--sm\" name=\"source\" value=\"notes\">Show the notes</button>",
    ] {
        assert!(hidden.contains(part), "missing {part}: {hidden}");
    }
    assert_eq!(note_views(&scene.database())?, 0);
    // The admin key alone has no session to record the view with.
    let with_key = scene.get_with_key(&path).await?;
    assert!(
        with_key.contains(
            "<a class=\"link\" href=\"/login\">Sign in</a> to show the original message."
        )
    );
    assert!(!with_key.contains(note));
    let csrf = scene.token().await?;
    let raw = format!("{path}/raw");
    // A form this page did not send shows nothing and records nothing.
    let forged = scene
        .post(
            &raw,
            &format!("csrf={csrf}&target=receipt&source=notes&after=0&offset=0&reason="),
        )
        .await?;
    assert!(forged.starts_with("HTTP/1.1 409"), "{forged}");
    assert!(!forged.contains(note));
    assert_eq!(note_views(&scene.database())?, 0);
    let shown = scene
        .post(
            &raw,
            &format!(
                "csrf={csrf}&target=notes&source=notes&after=0&offset=0&reason=checking+the+rack"
            ),
        )
        .await?;
    assert!(shown.starts_with("HTTP/1.1 200"), "{shown}");
    for part in [
        note,
        "<p class=\"alert__title\">May contain patient data</p>",
        "Notes the machine sent, as sent</summary>",
    ] {
        assert!(shown.contains(part), "missing {part}: {shown}");
    }
    assert!(!shown.contains("Show the original message</summary>"));
    assert_eq!(note_views(&scene.database())?, 1);
    let recorded: String = rusqlite::Connection::open(scene.database())?.query_row(
        "SELECT evidence FROM audit WHERE json_extract(evidence,'$.action')='admin-raw-view'",
        [],
        |row| row.get(0),
    )?;
    let recorded: serde_json::Value = serde_json::from_str(&recorded)?;
    assert_eq!(
        recorded["targets"],
        serde_json::json!([format!("awos:{order}:1"), "notes", "notes-after:0"])
    );
    assert_eq!(recorded["reason"], "checking the rack");
    assert!(
        recorded["actor"]
            .as_str()
            .is_some_and(|actor| actor.starts_with("admin-session-")),
        "{recorded}"
    );
    // A view is not a change, so Activity does not list it.
    let activity = scene.get("/activity").await?;
    assert!(!activity.contains("notes"), "{activity}");
    // An item whose messages carry no notes records nothing.
    let quiet = scene
        .post(
            &format!("/awos/{}/1/raw", scene.seeded.done),
            &format!("csrf={csrf}&target=notes&source=notes&after=0&offset=0&reason="),
        )
        .await?;
    assert!(quiet.starts_with("HTTP/1.1 200"), "{quiet}");
    assert_eq!(note_views(&scene.database())?, 1);
    scene.stop().await
}
