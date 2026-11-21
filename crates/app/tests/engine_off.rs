//! Tests the tray's view of a real HTTP engine stand-in: running, gone off, back again, a kind of thing that needs someone, and another program on the engine's port.
use analyte_app::engine::{
    Engine,
    status::{Client, Health, Reply, Silence},
};
use analyte_app::watch::{Shown, Watch};
use std::time::Duration;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// A stand-in for the engine's `/status` on `listener`, answering `body`
/// with `status`; it stops when the returned sender is dropped or sent.
fn serve(
    listener: tokio::net::TcpListener,
    status: u16,
    body: &'static str,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let app = axum::Router::new().route(
        "/status",
        axum::routing::get(move || async move {
            (
                axum::http::StatusCode::from_u16(status)
                    .unwrap_or(axum::http::StatusCode::IM_A_TEAPOT),
                [("content-type", "application/json")],
                body,
            )
        }),
    );
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let owner = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await
    });
    (stop, owner)
}

async fn ask(client: &Client) -> Result<Reply, Error> {
    Ok(tokio::time::timeout(Duration::from_secs(10), client.ask()).await?)
}

// ADR 0019 (Engine off): the app reads the engine's state, counts it off
// after two unanswered questions, and shows the pages again with the first
// answer; only going off notifies.
#[tokio::test]
async fn engine_goes_off_and_comes_back() -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let engine = Engine::at(port).ok_or("port 0")?;
    let client = Client::new(&engine)?;
    let mut watch = Watch::new();
    let now = jiff::Timestamp::now;

    let (stop, owner) = serve(listener, 200, r#"{"state":"good","version":"0.1.0"}"#);
    let up = watch.observe(ask(&client).await?, now());
    assert_eq!(up.notice, None, "the first state is not news");
    let up = up.change.ok_or("no change")?;
    assert_eq!(up.to, Shown::Running(Health::Good));

    stop.send(()).map_err(|()| "stand-in already stopped")?;
    owner.await??;
    let first = ask(&client).await?;
    assert_eq!(first, Reply::Silent(Silence::Refused));
    let missed = watch.observe(first, now());
    assert_eq!(
        (missed.change, missed.notice),
        (None, None),
        "one miss is not off"
    );
    let off = watch.observe(ask(&client).await?, now());
    assert!(off.notice.is_some(), "going off notifies once");
    assert_eq!(off.change.map(|change| change.to), Some(Shown::Off));
    let still = watch.observe(ask(&client).await?, now());
    assert_eq!(
        (still.change, still.notice),
        (None, None),
        "still off, no repeat"
    );

    let again = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    // An engine from before ADR 0023 sends no kinds; its state still counts.
    let (stop, owner) = serve(again, 200, r#"{"state":"attention","version":"0.1.0"}"#);
    let back = watch.observe(ask(&client).await?, now());
    assert_eq!(back.notice, None, "coming back is better, not worse");
    assert_eq!(
        back.change.map(|change| change.to),
        Some(Shown::Running(Health::Attention))
    );
    assert_eq!(
        watch.last_answer().map(|(_, health)| health),
        Some(Health::Attention)
    );
    stop.send(()).map_err(|()| "stand-in already stopped")?;
    owner.await??;
    Ok(())
}

// ADR 0023: the stand-in names a machine's connection off; the notice says
// so in plain words with what to do, and the title bar line keeps it.
#[tokio::test]
async fn a_kind_from_the_engine_heads_the_notice() -> Result<(), Error> {
    let good = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = good.local_addr()?.port();
    let engine = Engine::at(port).ok_or("port 0")?;
    let client = Client::new(&engine)?;
    let mut watch = Watch::new();
    let (stop, owner) = serve(
        good,
        200,
        r#"{"state":"good","reasons":[],"version":"0.1.0"}"#,
    );
    watch.observe(ask(&client).await?, jiff::Timestamp::now());
    stop.send(()).map_err(|()| "stand-in already stopped")?;
    owner.await??;
    let off = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let (stop, owner) = serve(
        off,
        200,
        r#"{"state":"problem","reasons":["machine_link_off","needs_review"],"version":"0.1.0"}"#,
    );
    let now = jiff::Timestamp::now();
    let seen = watch.observe(ask(&client).await?, now);
    let notice = seen.notice.ok_or("a worse state notifies")?;
    assert_eq!(notice.title, "A machine’s connection is off");
    assert_eq!(
        notice.body,
        "Open Analyte to see which machine and what to check. 1 more thing needs a look too."
    );
    let line = watch
        .recap()
        .and_then(|recap| recap.line(now, &jiff::tz::TimeZone::UTC))
        .ok_or("the title bar keeps the notice")?;
    assert!(line.ends_with(" · A machine’s connection is off"), "{line}");
    stop.send(()).map_err(|()| "stand-in already stopped")?;
    owner.await??;
    Ok(())
}

// AGENTS.md (never guess): another program on the engine's port is not the
// engine; the app says so instead of showing a state.
#[tokio::test]
async fn another_program_on_the_port_is_not_the_engine() -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let engine = Engine::at(listener.local_addr()?.port()).ok_or("port 0")?;
    let (stop, owner) = serve(listener, 200, "<html>a different program</html>");
    let reply = ask(&Client::new(&engine)?).await?;
    assert!(matches!(reply, Reply::Unrecognised(_)), "{reply:?}");
    let mut watch = Watch::new();
    assert_eq!(
        watch
            .observe(reply, jiff::Timestamp::now())
            .change
            .map(|c| c.to),
        Some(Shown::Off)
    );
    stop.send(()).map_err(|()| "stand-in already stopped")?;
    owner.await??;
    Ok(())
}

// ADR 0019 (bounds): a stand-in that accepts the connection and never answers
// is given up after the question's time limit.
#[tokio::test]
async fn a_silent_engine_is_given_up_in_time() -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let engine = Engine::at(listener.local_addr()?.port()).ok_or("port 0")?;
    let hold = tokio::spawn(async move {
        let accepted = listener.accept().await;
        tokio::time::sleep(Duration::from_secs(8)).await;
        drop(accepted);
    });
    let started = std::time::Instant::now();
    let reply = ask(&Client::new(&engine)?).await?;
    assert_eq!(reply, Reply::Silent(Silence::TimedOut));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    hold.abort();
    let _ = hold.await;
    Ok(())
}
