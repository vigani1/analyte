//! Tests that every signed-in page's sidebar shows the Needs review count, read once per request, and that the live script reloads a page for it only when it changed.
use super::super::*;
use super::Scene;
use analyte_store::Parked;

// Owner decision 2026-11-20 (review item 8, B4): the sidebar's Needs review
// count is read once for every signed-in page, so it never disappears; the
// pages shown before sign-in name no count, and the live script reloads a
// page for the count only when the count changed.
#[tokio::test(flavor = "multi_thread")]
async fn every_signed_in_page_shows_the_needs_review_count() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    scene
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            for reason in ["synthetic one", "synthetic two"] {
                s.park(&Parked {
                    kind: Identifier::new("mapping_finding").map_err(FixtureError::Text)?,
                    reason: Reason::new(reason).map_err(FixtureError::Text)?,
                    targets: Vec::new(),
                })?;
            }
            Ok(())
        })
        .await?;
    let badge = "<span class=\"nav__count\"><span class=\"sr-only\">, </span>2</span>";
    let held = uuid::Uuid::from(scene.seeded.held.ok_or("held")?);
    let delivered = scene.seeded.delivered.ok_or("delivered")?;
    let pages = [
        "/".to_owned(),
        "/activity".to_owned(),
        "/parked".to_owned(),
        "/parked/1".to_owned(),
        "/machines".to_owned(),
        "/machines/chemistry-analyzer".to_owned(),
        format!("/receipts/{held}/departures?version=1"),
        "/setup".to_owned(),
        "/setup/add".to_owned(),
        "/setup/lab-apps".to_owned(),
        "/orders".to_owned(),
        "/analyzer-cancelled".to_owned(),
        format!("/awos/{}/1", scene.seeded.cancelled),
        "/deliveries".to_owned(),
        format!("/deliveries/{delivered}/resend"),
        "/retention".to_owned(),
        "/settings".to_owned(),
        // A page that only says what went wrong keeps the sidebar too.
        "/machines/no-such-machine".to_owned(),
    ];
    for path in &pages {
        let page = scene.get(path).await?;
        assert!(page.contains("<nav class=\"sidebar\""), "{path}: {page}");
        assert_eq!(page.matches(badge).count(), 1, "{path}: {page}");
        // The count sits beside Needs review and nowhere else.
        assert!(
            page.contains(&format!(
                "<span class=\"nav__label\">Needs review</span>{badge}"
            )),
            "{path}"
        );
    }
    // The admin key alone gets the same sidebar.
    let with_key = scene.get_with_key("/settings").await?;
    assert!(with_key.contains(badge), "{with_key}");
    // Sign-in is shown before any session and names no count.
    let login = request(&scene.started.3, "/login", "", None).await?;
    assert!(!login.contains("nav__count"), "{login}");
    let script = request(
        &scene.started.3,
        &format!(
            "{}live.js",
            crate::admin::view::assets::Root::of_this_build().as_str()
        ),
        "",
        None,
    )
    .await?;
    assert!(script.contains("return area === 'count' || shows.indexOf(area) >= 0;"));
    assert!(!script.contains("area === 'review'"));
    scene.stop().await
}
