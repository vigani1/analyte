//! Tests the setup writes' own time budget: a save slower than a page read still gets its answer, and one slower than the write budget gets "This took too long".
use super::*;

// AGENTS.md rule 7 (bound every wait); ADR 0003 rule 1: a save that takes
// longer than a page read may, and longer than a connection first lives, still
// answers "Settings saved" within the setup write budget; on a slow PC it used
// to get "This took too long" although it had been saved.
#[tokio::test(flavor = "multi_thread")]
async fn save_slower_than_a_page_read_still_answers() -> Result<(), Error> {
    let fixture = start().await?;
    let body = fixture.form(&[("port", "usb:A10K3XYZ")]).await?;
    let hold = listener::LIFETIME.saturating_add(Duration::from_millis(500));
    let blocker = hold_store(&fixture.store, hold).await?;
    let started = std::time::Instant::now();
    let saved = fixture.post("/setup/synthetic", &body).await?;
    let took = started.elapsed();
    blocker.await??;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("Settings saved"), "{saved}");
    assert!(took > listener::LIFETIME, "{took:?}");
    assert_eq!(fixture.audited("setup-save").await?.len(), 1);
    Ok(())
}

// AGENTS.md rule 7 (bound every wait): a setup write that outlasts its own
// budget gets "This took too long", which warns that it may already be saved,
// and that answer still reaches the browser.
#[tokio::test(flavor = "multi_thread")]
async fn save_slower_than_its_budget_is_too_long() -> Result<(), Error> {
    // Past the first lifetime too, so only a connection kept open can carry the answer.
    let budget = listener::LIFETIME.saturating_add(Duration::from_millis(500));
    let fixture = start_bounded(Bounds {
        write: budget,
        ..BOUNDS
    })
    .await?;
    let body = fixture.form(&[("port", "usb:A10K3XYZ")]).await?;
    let blocker = hold_store(
        &fixture.store,
        budget.saturating_add(Duration::from_millis(700)),
    )
    .await?;
    let started = std::time::Instant::now();
    let slow = fixture.post("/setup/synthetic", &body).await?;
    let took = started.elapsed();
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow}");
    assert!(
        slow.contains("an action may already have been saved."),
        "{slow}"
    );
    assert!(took >= budget, "{took:?}");
    // It was: the files were written before the audit row waited.
    assert_eq!(fixture.backups()?.len(), 1);
    blocker.await??;
    Ok(())
}
