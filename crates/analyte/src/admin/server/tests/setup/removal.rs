//! Tests delivery to a lab app after the last machine using it was removed on the setup page.
use super::super::pages::{csrf, encode, login};
use super::super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The reference receiver's request counts and its hold on the first result.
struct Seen {
    results: AtomicUsize,
    orders: AtomicUsize,
    /// Fires when the first result arrives; the receiver then waits for `release`.
    arrived: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

async fn count(State(seen): State<Arc<Seen>>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path == "/orders" {
        seen.orders.fetch_add(1, Ordering::SeqCst);
    } else if path == "/results" && seen.results.fetch_add(1, Ordering::SeqCst) == 0 {
        seen.arrived.notify_one();
        let _ = tokio::time::timeout(analyte_fixtures::HANG_GUARD, seen.release.notified()).await;
    }
    next.run(request).await
}

/// Loads the program with the analyzer listener (while listed) and `url` for `demo`.
fn program(
    file: &std::path::Path,
    analyzer: Option<&std::net::TcpListener>,
    url: &str,
) -> Result<(Program, crate::installation::secrets::Environment), Error> {
    let mut program = Program::load(file)?;
    if let Some(analyzer) = analyzer {
        program.set_loopback_listener_for_test("synthetic", analyzer.try_clone()?)?;
    }
    program.set_loopback_destination_for_test("demo", url.into())?;
    program.set_loopback_store_budget_for_test("demo", analyte_fixtures::STORE_BUDGET)?;
    program.set_shutdown_io_budget_for_test(analyte_fixtures::STORE_BUDGET)?;
    // Any order poll that wrongly ran for this destination would show at once.
    program.set_loopback_poll_interval_for_test("demo", Duration::from_millis(1))?;
    let secrets = crate::installation::secrets::Environment::load_with(&program, |name| {
        Some(if name.ends_with("SIGNING") {
            analyte_fixtures::SECRET.into()
        } else {
            "synthetic-api".into()
        })
    })?;
    Ok((program, secrets))
}

fn start(
    program: Program,
    secrets: crate::installation::secrets::Environment,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), crate::runtime::Error>>,
) {
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let owner = tokio::spawn(crate::runtime::service::serve_loopback_test(
        program,
        Box::new(Space),
        async {
            stopped.await?;
            Ok(())
        },
        secrets,
        None,
    ));
    (stop, owner)
}

/// Polls the overview until `done` holds for it; the timeout bounds the wait.
async fn overview_until(
    host: &str,
    auth: &str,
    done: impl Fn(&str) -> bool,
) -> Result<String, Error> {
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
        loop {
            let page = request(host, "/", auth, None).await?;
            if done(&page) {
                return Ok::<_, Error>(page);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?
}

const STILL: &str = concat!(
    "<p class=\"alert__title\">Removed machine synthetic: 1 result or order message ",
    "still being delivered to lab app demo."
);

// Owner request 2026-11-19 (results waiting after a removal; lab app set once,
// item 5); ADR 0003 rule 2; AGENTS.md safety rules 2 and 8; plan §11: removing
// the only machine that sends to a lab app keeps that lab app in lab-apps.toml
// (never in the program file), so after the restart its undelivered result
// reaches the reference receiver exactly once, the pages count it down to
// nothing, the worker ends without stopping the service, and no order poll
// runs for a lab app no machine uses.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario across a restart: store, remove, restart, count, deliver, count down"
)]
async fn removed_machine_result_is_delivered_once() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    analyte_fixtures::write_definition(
        root,
        "synthetic",
        include_str!("../../../../../../core/config/examples/hl7.toml"),
    )?;
    let analyzer = std::net::TcpListener::bind("127.0.0.1:0")?;
    let analyzer_address = analyzer.local_addr()?.to_string();
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../../../../core/config/examples/site.toml")
            .rewrite("127.0.0.1:5600", &analyzer_address),
    )?;
    let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
    let host = reserved.local_addr()?.to_string();
    let file = root.join("program.toml");
    std::fs::write(
        &file,
        format!(
            "{}[admin]\nport = {}\n",
            admin_program(),
            reserved.local_addr()?.port()
        ),
    )?;
    let token = crate::admin::rotate_token(&root.join("data"))?;
    let auth = format!("Authorization: Bearer {token}\r\n");
    // While the machine runs, the lab app refuses connections: a bound socket that
    // does not listen. The reference receiver gets its own port for the restart.
    let outage = tokio::net::TcpSocket::new_v4()?;
    outage.bind("127.0.0.1:0".parse()?)?;
    let refused = format!("http://{}", outage.local_addr()?);
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.bind("127.0.0.1:0".parse()?)?;
    let receiver_url = format!("http://{}", socket.local_addr()?);
    drop(reserved);

    // First run: the machine sends one result the lab app cannot take yet.
    let (first, secrets) = program(&file, Some(&analyzer), &refused)?;
    let (stop, owner) = start(first, secrets);
    wait_for_service_listener(&host).await?;
    let mut peer = tokio::net::TcpStream::connect(&analyzer_address).await?;
    peer.write_all(
        concat!(
            "\x0bMSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|removal-1|P|2.5.1\r",
            "PID|1\rOBR|1||00012a|SYN^Synthetic^synthetic|||||||||||||||||||||F\r",
            "OBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r\x1c\r"
        )
        .as_bytes(),
    )
    .await?;
    let mut reply = [0];
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, peer.read_exact(&mut reply)).await??;
    assert_eq!(reply, [0x0b], "the result is stored and acknowledged");
    drop(peer);
    overview_until(&host, &auth, |page| {
        page.split("<div class=\"node node--lab")
            .skip(1)
            .any(|lab| {
                lab.contains("<span class=\"node__name\" title=\"demo\">demo</span>")
                    && lab.contains(" · 1 item waiting · oldest <time ")
            })
    })
    .await?;

    // Remove the only machine; the page says delivery continues.
    let cookie = login(&host, &token).await?;
    let session = format!("Cookie: {cookie}\r\n");
    let confirm = request(&host, "/setup/synthetic/remove", &session, None).await?;
    assert!(
        confirm.contains("1 result or order message not yet accepted by the lab app"),
        "{confirm}"
    );
    // Owner request 2026-11-19 (item 9): one sentence says what happens, without
    // "retired destination" or "program file".
    assert!(
        confirm.contains("Its 1 waiting result will still be sent to demo."),
        "{confirm}"
    );
    assert!(!confirm.contains("retired destination"), "{confirm}");
    assert!(!confirm.contains("program file"), "{confirm}");
    let removed = request(
        &host,
        "/setup/synthetic/remove",
        &format!("{session}Content-Type: application/x-www-form-urlencoded\r\n"),
        Some(&format!("csrf={}&anyway=yes", encode(&csrf(&confirm)?))),
    )
    .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    stop.send(()).map_err(|_| "service stop receiver lost")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    let kept = Program::load(&file)?;
    assert!(kept.machines().is_empty() && kept.awaiting_setup().is_empty());
    let names: Vec<&str> = kept
        .lab_apps()
        .iter()
        .map(|kept| kept.settings().name.as_str())
        .collect();
    assert_eq!(names, ["demo"]);
    assert!(root.join("lab-apps.toml").exists());
    assert!(!std::fs::read_to_string(&file)?.contains("retired_destinations"));

    // Restart with no machine: the result waits, and both pages say so.
    let (second, secrets) = program(&file, None, &receiver_url)?;
    let (stop, owner) = start(second, secrets);
    wait_for_service_listener(&host).await?;
    let waiting = overview_until(&host, &auth, |page| page.contains(STILL)).await?;
    assert!(
        waiting.contains("<p class=\"empty__title\">No machine is running</p>")
            && !waiting.contains("<a class=\"node"),
        "{waiting}"
    );
    let cookie = login(&host, &token).await?;
    let setup = request(&host, "/setup", &format!("Cookie: {cookie}\r\n"), None).await?;
    assert!(
        setup.contains(concat!(
            "<p class=\"alert__title\">Removed machine synthetic: 1 result or order message ",
            "still being delivered to lab app demo.</p>"
        )),
        "{setup}"
    );

    // The reference receiver comes up and holds the first result until the page
    // has shown it in flight; then it stores it.
    let receiver_path = root.join("reference.db");
    let mut receiver = analyte_receiver::Receiver::open(
        &receiver_path,
        "synthetic-api".into(),
        analyte_contract::SigningSecret::parse(analyte_fixtures::SECRET)?,
    )?;
    let seen = Arc::new(Seen {
        results: AtomicUsize::new(0),
        orders: AtomicUsize::new(0),
        arrived: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let router = receiver
        .router()
        .layer(middleware::from_fn_with_state(seen.clone(), count));
    let (close, closing) = tokio::sync::oneshot::channel::<()>();
    let listener = socket.listen(16)?;
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = closing.await;
            })
            .await
    });
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, seen.arrived.notified()).await?;
    assert!(request(&host, "/", &auth, None).await?.contains(STILL));
    seen.release.notify_one();
    let drained =
        overview_until(&host, &auth, |page| !page.contains("still being delivered")).await?;
    assert!(drained.starts_with("HTTP/1.1 200"), "{drained}");
    let setup = request(&host, "/setup", &format!("Cookie: {cookie}\r\n"), None).await?;
    assert!(!setup.contains("still being delivered"), "{setup}");
    // A quiet window: no repeat delivery and no order poll may follow.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        seen.results.load(Ordering::SeqCst),
        1,
        "delivered exactly once"
    );
    assert_eq!(seen.orders.load(Ordering::SeqCst), 0, "no order poll ran");
    let stored: i64 = rusqlite::Connection::open(&receiver_path)?.query_row(
        "SELECT count(*) FROM deliveries WHERE kind='results'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(stored, 1);
    assert!(
        !owner.is_finished(),
        "the finished worker stopped the service"
    );
    stop.send(()).map_err(|_| "service stop receiver lost")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    let state: String = rusqlite::Connection::open(root.join("data/analyte.db"))?.query_row(
        "SELECT state FROM outbox",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(state, "done");
    close.send(()).map_err(|()| "receiver stopped")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, serving).await???;
    receiver.shutdown().await?;
    Ok(())
}
