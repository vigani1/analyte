//! Tests server behavior with synthetic fixtures.
use super::*;
use crate::Error;
use analyte_fixtures::ConfigEdit as _;
use analyte_model::{OrderState, Reason};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

mod actions;
mod activity;
mod assets;
mod command;
mod consistency;
mod live;
mod machines;
mod more;
mod overview;
mod pages;
mod parked;
mod setup;
mod sign_in;

use analyte_fixtures::UnlimitedSpace as Space;

/// Whether `page` holds a script other than the one the signed-in frame loads,
/// `live.js` (owner decision 2026-11-20, PR c2): any inline script, or another file.
fn other_script(page: &str) -> bool {
    let live = format!(
        "<script src=\"{}live.js\" defer></script>",
        crate::admin::view::assets::Root::of_this_build().as_str()
    );
    page.replacen(&live, "", 1).contains("<script")
}

fn admin_program() -> String {
    analyte_fixtures::program_with_limits(
        "[[machines]]\nid = 'synthetic'\ndefinition = 'test/synthetic/machine.toml'\nsite = 'site.toml'\n",
        &analyte_store::Limits {
            reserve_bytes: 1,
            database_bytes: 100_000_000,
            record_bytes: 2_097_152,
            page_rows: 1,
            busy_timeout: Duration::from_millis(100),
        },
    )
}

// Fixture migration/seeding is not an assertion about production latency.
fn fixture_deadline() -> Result<std::time::Instant, Error> {
    std::time::Instant::now()
        .checked_add(analyte_fixtures::STORE_BUDGET)
        .ok_or_else(|| "admin fixture deadline overflow".into())
}

/// Holds the store's one worker for `hold`, as a slow disk would, and returns
/// only once the hold has begun, so a read or write sent next queues behind
/// it. A fixed sleep before the request instead raced a cold Windows worker
/// (test round B, 2026-11-21).
async fn hold_store(
    store: &Arc<analyte_store::Handle>,
    hold: Duration,
) -> Result<tokio::task::JoinHandle<Result<(), Error>>, Error> {
    let (started, holding) = tokio::sync::oneshot::channel();
    let busy = store.clone();
    let blocker = tokio::spawn(async move {
        busy.request::<_, FixtureError>(fixture_deadline()?, move |_| {
            let _ = started.send(());
            std::thread::sleep(hold);
            Ok(())
        })
        .await
        .map_err(Error::from)
    });
    holding.await.map_err(|_| "the store never ran the hold")?;
    Ok(blocker)
}

/// Past the pages' 1.5 s read budget, so a page read behind it times out.
const PAST_READ_BUDGET: Duration = Duration::from_millis(1800);

// Plan section 11: the actual program refuses absent tokens before opening the
// endpoint, owns the enabled server, and joins it before closing the store.
// Plan §11
#[tokio::test(flavor = "multi_thread")]
async fn service_refuses_missing_token_and_owns_admin_server() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    analyte_fixtures::write_definition(
        directory.path(),
        "synthetic",
        include_str!("../../../../../core/config/examples/hl7.toml"),
    )?;
    let analyzer = std::net::TcpListener::bind("127.0.0.1:0")?;
    let analyzer_address = analyzer.local_addr()?.to_string();
    std::fs::write(
        directory.path().join("site.toml"),
        include_str!("../../../../../core/config/examples/site.toml")
            .rewrite("127.0.0.1:5600", &analyzer_address),
    )?;
    let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = reserved.local_addr()?;
    let file = directory.path().join("program.toml");
    std::fs::write(
        &file,
        format!("{}[admin]\nport = {}\n", admin_program(), address.port()),
    )?;
    let program = || -> Result<Program, Error> {
        let mut program = Program::load(&file)?;
        program.set_loopback_listener_for_test("synthetic", analyzer.try_clone()?)?;
        program.set_loopback_destination_for_test("demo", "http://127.0.0.1:1".into())?;
        Ok(program)
    };
    let secrets = |program: &Program| {
        crate::installation::secrets::Environment::load_with(program, |name| {
            Some(if name.ends_with("SIGNING") {
                "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()
            } else {
                "synthetic-api-key".into()
            })
        })
    };
    drop(reserved);
    let missing = program()?;
    // Exercise token refusal against an already migrated store; racing fresh
    // synchronous=FULL migrations against production's 5 s startup budget tests
    // disk speed instead of missing-token behavior.
    let mut prepared = crate::runtime::service::open_store_with_deadline(
        &missing,
        Box::new(Space),
        fixture_deadline()?,
    )
    .await
    .map_err(|error| format!("admin lifecycle fixture migration: {error}"))?;
    prepared.shutdown(fixture_deadline()?).await?;
    let injected = secrets(&missing)?;
    let failure = crate::runtime::service::serve_loopback_test(
        missing,
        Box::new(Space),
        std::future::pending(),
        injected,
        None,
    )
    .await
    .err()
    .ok_or("missing credential unexpectedly admitted")?;
    assert!(
        matches!(
            &failure,
            crate::runtime::Error::Admin(error) if matches!(error.as_ref(), crate::admin::Error::Token(super::super::TokenError::Missing))
        ),
        "expected missing token after fixture migration, received: {failure}"
    );
    let token = super::super::rotate_token(&directory.path().join("data"))?;
    let program = program()?;
    let injected = secrets(&program)?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let owner = tokio::spawn(crate::runtime::service::serve_loopback_test(
        program,
        Box::new(Space),
        async {
            stopped.await?;
            Ok(())
        },
        injected,
        None,
    ));
    let host = address.to_string();
    wait_for_service_listener(&host).await?;
    let response = request(
        &host,
        "/",
        &format!("Authorization: Bearer {token}\r\n"),
        None,
    )
    .await?;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    // The machine's service runs: its node shows the link state, not that Analyte stopped.
    assert!(
        response.contains("<span class=\"node__state tone-"),
        "{response}"
    );
    assert!(!response.contains("Stopped in Analyte"));
    // Owner request 2026-11-19 (item 8): no data yet reads "Nothing yet", not "unknown".
    assert!(response.contains("<span class=\"mono subtle\">Nothing yet</span>"));
    headers_present(&response);
    stop.send(()).map_err(|_| "service stop receiver lost")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    assert!(tokio::net::TcpStream::connect(&host).await.is_err());
    Ok(())
}

// Plan §11 admin page: the light follows the machine runtime, not receipt age.
// A TCP server waits, shows a connected peer, reads a partial chunk as last data
// before any receipt exists, and waits again after the peer leaves.
// Safety rule 10 (ADR 0003): no payload or sample ID reaches the admin page.
#[tokio::test(flavor = "multi_thread")]
async fn tcp_server_light_follows_peer_and_hides_payload() -> Result<(), Error> {
    const SAMPLE: &str = "SYN-LIGHT-4711";
    let directory = crate::admin::private_tempdir()?;
    analyte_fixtures::write_definition(
        directory.path(),
        "synthetic",
        include_str!("../../../../../core/config/examples/hl7.toml"),
    )?;
    let analyzer = std::net::TcpListener::bind("127.0.0.1:0")?;
    let analyzer_address = analyzer.local_addr()?.to_string();
    std::fs::write(
        directory.path().join("site.toml"),
        include_str!("../../../../../core/config/examples/site.toml")
            .rewrite("127.0.0.1:5600", &analyzer_address),
    )?;
    let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = reserved.local_addr()?;
    let file = directory.path().join("program.toml");
    std::fs::write(
        &file,
        format!("{}[admin]\nport = {}\n", admin_program(), address.port()),
    )?;
    let mut program = Program::load(&file)?;
    program.set_loopback_listener_for_test("synthetic", analyzer)?;
    program.set_loopback_destination_for_test("demo", "http://127.0.0.1:1".into())?;
    let secrets = crate::installation::secrets::Environment::load_with(&program, |name| {
        Some(if name.ends_with("SIGNING") {
            "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()
        } else {
            "synthetic-api-key".into()
        })
    })?;
    let token = super::super::rotate_token(&directory.path().join("data"))?;
    drop(reserved);
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
    let host = address.to_string();
    let auth = format!("Authorization: Bearer {token}\r\n");
    wait_for_service_listener(&host).await?;
    let page = |path: &'static str| {
        let (host, auth) = (host.clone(), auth.clone());
        async move { request(&host, path, &auth, None).await }
    };
    let until = |wanted: &'static str| {
        let page = &page;
        async move {
            tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
                loop {
                    let overview = page("/").await?;
                    if overview.contains(wanted) {
                        return Ok::<_, Error>(overview);
                    }
                    // The service has no readiness channel for its light; the timeout bounds polling.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await?
        }
    };
    let waiting = until("<span class=\"node__state tone-off\">Waiting</span>").await?;
    assert_listening(&waiting, &analyzer_address);
    // Owner request 2026-11-19 (item 9): no restore pause, so no pause line on either page.
    assert!(!(waiting.clone() + &page("/machines").await?).contains("Order sending"));
    let mut peer = tokio::net::TcpStream::connect(&analyzer_address).await?;
    until("<span class=\"node__state tone-good\">Connected</span>").await?;
    let message = format!(
        "\x0bMSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|light-1|P|2.5.1\r\
         PID|1\rOBR|1||{SAMPLE}|SYN^Synthetic^synthetic|||||||||||||||||||||F\r\
         OBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r"
    );
    peer.write_all(message.as_bytes()).await?;
    // Only a partial MLLP frame has arrived: no receipt exists, yet the runtime saw data.
    // One check: the green Connected pill next to a last-data time.
    until(CONNECTED_WITH_DATA).await?;
    peer.write_all(b"\x1c\r").await?;
    let mut reply = [0];
    peer.read_exact(&mut reply).await?;
    assert_eq!(reply, [0x0b], "the peer receives an MLLP acknowledgment");
    for path in ["/", "/machines"] {
        let shown = page(path).await?;
        assert!(shown.starts_with("HTTP/1.1 200"), "{shown}");
        assert!(!shown.contains(SAMPLE), "{path} shows a sample ID");
        assert!(!shown.contains("MSH|"), "{path} shows payload bytes");
        assert!(
            !shown.contains(&analyzer_address),
            "{path} shows the endpoint"
        );
    }
    drop(peer);
    until("<span class=\"node__state tone-off\">Waiting</span>").await?;
    stop.send(()).map_err(|_| "service stop receiver lost")?;
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    Ok(())
}

/// A network machine that has not connected: its node says Analyte listens on
/// its port (never the full address) and that no data came yet.
fn assert_listening(page: &str, address: &str) {
    let port = address.rsplit(':').next().unwrap_or_default();
    assert!(page.contains("</svg>Network</span>"), "{page}");
    assert!(
        page.contains(&format!(
            "Listening on port {port}, nothing connected since "
        )),
        "{page}"
    );
    assert!(page.contains(&format!(
        "Waiting</span>{SEPARATOR}{LAST_DATA}Nothing yet</span>"
    )));
}

/// A connected machine node that already shows a last-data time.
const CONNECTED_WITH_DATA: &str = concat!(
    "<span class=\"node__state tone-good\">Connected</span>",
    "<span class=\"subtle\" aria-hidden=\"true\">·</span>",
    "<span class=\"mono\"><span class=\"sr-only\">Last data </span><time"
);

/// A machine node's "last data" with no data yet, as it opens, and the dot before it.
const LAST_DATA: &str = "<span class=\"mono subtle\">";
const SEPARATOR: &str = "<span class=\"subtle\" aria-hidden=\"true\">·</span>";

async fn request(
    host: &str,
    path: &str,
    headers: &str,
    body: Option<&str>,
) -> Result<String, Error> {
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
        let mut stream = tokio::net::TcpStream::connect(host).await?;
        let method = if body.is_some() { "POST" } else { "GET" };
        let body = body.unwrap_or("");
        let message = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{headers}Content-Length: {}\r\n\r\n{body}", body.len());
        stream.write_all(message.as_bytes()).await?;
        let mut bytes = Vec::new();
        stream.take(1_048_576).read_to_end(&mut bytes).await?;
        Ok::<_, Error>(String::from_utf8(bytes)?)
    }).await?
}

fn headers_present(response: &str) {
    let lower = response.to_ascii_lowercase();
    for expected in [
        "content-security-policy: default-src 'self'",
        "x-frame-options: deny",
        "referrer-policy: same-origin",
        "cache-control: no-store",
    ] {
        assert!(lower.contains(expected), "missing {expected}");
    }
}

// Plan sections 11/12 and rule 10: real HTTP proves authorization, escaping,
// bounded metadata reads, honest health and no credential/payload in logs.
// Plan §11, §12
#[tokio::test(flavor = "multi_thread")]
async fn overview_checks_auth_escapes_and_shuts_down_bounded() -> Result<(), Error> {
    let logs = crate::runtime::log_capture::init();
    let directory = crate::admin::private_tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    let bearer = super::super::rotate_token(directory.path())?;
    let limits = {
        let mut limits = analyte_fixtures::store_limits();
        limits.reserve_bytes = 1;
        limits.page_rows = 1;
        limits.busy_timeout = Duration::from_millis(100);
        limits
    };
    let store = Arc::new(
        analyte_store::Handle::start(
            directory.path().join("analyte.db"),
            limits,
            Box::new(Space),
            4,
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("admin overview fixture migration: {error}"))?,
    );
    seed_overview(&store).await?;
    let machines = vec![machine(
        "<script>alert('hostile')&\"</script>",
        false,
        crate::machine::link::status::Reporter::new(crate::machine::link::status::Link::TcpServer)
            .subscribe(),
        "app",
    )?];
    let (state, stop, owner, host) = start_overview_server(
        directory.path(),
        &store,
        machines,
        vec![loopback_destination("app")?],
    )
    .await?;
    let wrong = "wrong-secret-never-log-this";
    assert_overview_refuses_auth(&host, wrong).await?;
    assert_overview_health(&host, &bearer).await?;
    assert_stylesheet(&host, &bearer).await?;
    let (session, form) = assert_overview_login(&host, &bearer).await?;
    assert_overview_rejects_login_and_missing_route(&host, &bearer, &form).await?;
    // A peer that never finishes its headers must not prevent bounded shutdown.
    let mut slow = tokio::net::TcpStream::connect(&host).await?;
    slow.write_all(b"GET / HTTP/1.1\r\n").await?;
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    drop(slow);
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retained store after shutdown")?;
    store.shutdown(fixture_deadline()?).await?;
    let captured = logs.lock().map_err(|_| "log capture poisoned")?;
    let captured = String::from_utf8_lossy(&captured);
    assert!(captured.contains("admin authentication refused"));
    for secret in [
        wrong,
        "wrong-cookie-secret",
        "wrong-login-secret",
        &bearer,
        &session,
        "hostile",
        "protected-raw-never-log",
        "protected-payload-never-log",
    ] {
        assert!(!captured.contains(secret));
    }
    Ok(())
}

// AGENTS ownership: HTTP completion can precede the last router state destructor.
// Plan §11
#[tokio::test]
async fn drain_waits_for_router_state_release() -> Result<(), Error> {
    use std::{future::Future as _, task::Poll};
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let mut drain = Box::pin(drain_server(std::future::ready(Ok(())), released));
    // Poll HTTP completion synchronously while the final state owner is held.
    // No scheduler, socket timing or elapsed-time assertion determines the verdict.
    let poll = std::future::poll_fn(|cx| Poll::Ready(drain.as_mut().poll(cx))).await;
    assert!(
        poll.is_pending(),
        "HTTP completion must not precede state release"
    );
    drop(release);
    drain.await?;
    Ok(())
}

async fn wait_for_service_listener(host: &str) -> Result<(), Error> {
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
        loop {
            if tokio::net::TcpStream::connect(&host).await.is_ok() {
                break;
            }
            // This externally started listener has no readiness channel; the enclosing timeout bounds probes.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

async fn seed_overview(store: &analyte_store::Handle) -> Result<(), Error> {
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |store| {
            store.evaluate_alert(
                &analyte_store::AlertKey::RestorePaused,
                "<script>responder('duty')&</script>",
                true,
                Some(1),
                100,
            )?;
            store.evaluate_alert(
                &analyte_store::AlertKey::MachineSilent(
                    Identifier::new("<busy&machine>").map_err(FixtureError::Text)?,
                ),
                "analyzer operator",
                true,
                None,
                100,
            )?;
            store.evaluate_alert(
                &analyte_store::AlertKey::ParkedItems,
                "cleared alert responder",
                true,
                Some(1),
                100,
            )?;
            store.evaluate_alert(
                &analyte_store::AlertKey::ParkedItems,
                "cleared alert responder",
                false,
                Some(0),
                101,
            )?;
            // Synthetic read fixtures: two pages ensure the overview does not report
            // a first-page count as a total. No machine commands are invoked.
            for order in ["first", "second"] {
                seed_overview_order(store, order)?;
            }
            let id = |value| Identifier::new(value).map_err(FixtureError::Text);
            store.audit(&analyte_store::Audit {
                actor: id("operator")?,
                action: id("store-restore")?,
                targets: Vec::new(),
                time: 103,
                reason: Reason::new("synthetic restore").map_err(FixtureError::Text)?,
            })?;
            Ok::<_, FixtureError>(())
        })
        .await?;
    Ok(())
}

fn seed_overview_order(store: &mut analyte_store::Store, order: &str) -> Result<(), FixtureError> {
    let id = |value| Identifier::new(value).map_err(FixtureError::Text);
    let raw = b"protected-raw-never-log";
    let config = b"synthetic-config";
    store.receive(
        &analyte_model::ReceiptMetadata {
            id: analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())
                .map_err(FixtureError::Identity)?,
            raw_digest: analyte_model::Digest(analyte_contract::content_digest(raw)),
            connection: id("synthetic")?,
            machine: id("<script>alert('hostile')&\"</script>")?,
            driver: analyte_model::NamespacedId {
                namespace: id("synthetic")?,
                value: id("1")?,
            },
            config_version: id("1")?,
            config_digest: analyte_model::Digest(analyte_contract::content_digest(config)),
            received_at: jiff::Timestamp::from_second(if order == "first" { 100 } else { 200 })
                .map_err(FixtureError::EvidenceTimestamp)?,
        },
        raw,
        config,
    )?;
    store.order_page(&analyte_store::OrderPage {
                    destination: id("app")?,
                    cursor: id(order)?,
                    complete: true,
                    polled: 100,
                    orders: vec![analyte_store::CachedOrder {
                        id: id(order)?,
                        sample: analyte_model::SampleId::new("<script>sample</script>")
                            .map_err(FixtureError::Text)?,
                        document: "{\"version\":1,\"state\":\"requested\",\"tombstone\":false,\"tests\":[{\"namespace\":\"local\",\"value\":\"test\"}]}"
                            .into(),
                        tombstone: false,
                        expiry: 200,
                    }],
                })?;
    let key = analyte_store::MachineOrderKey {
        destination: id("app")?,
        order: id(order)?,
        machine: id("machine")?,
    };
    let reserved = store
        .reserve_send(
            &analyte_store::OrderReservation {
                model: &id("example/synthetic")?,
                identity: &key,
                version: std::num::NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            analyte_store::ReserveOptions::default(),
        )?
        .map_err(|_| FixtureError::Conflict)?;
    store.transition_order(
        reserved.id,
        OrderState::Uncertain,
        analyte_model::SourceText::new("timeout").map_err(FixtureError::Text)?,
        102,
        |order| {
            Ok(analyte_store::OrderEventDelivery {
                id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())
                    .map_err(analyte_store::OrderError::Identity)?,
                destination: order.key.destination.clone(),
                order_id: order.key.order.clone(),
                payload: b"protected-payload-never-log".to_vec(),
                contract_version: 1,
            })
        },
    )?;
    Ok(())
}

async fn assert_overview_refuses_auth(host: &str, wrong: &str) -> Result<(), Error> {
    for auth in [
        String::new(),
        format!("Authorization: Bearer {wrong}\r\n"),
        "Cookie: analyte_session=wrong-cookie-secret\r\n".into(),
    ] {
        let response = request(host, "/", &auth, None).await?;
        assert!(response.starts_with("HTTP/1.1 401"));
        assert!(!response.contains(wrong));
        headers_present(&response);
    }
    Ok(())
}

async fn assert_overview_health(host: &str, bearer: &str) -> Result<(), Error> {
    let response = request(
        host,
        "/",
        &format!("Authorization: Bearer {bearer}\r\n"),
        None,
    )
    .await?;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    headers_present(&response);
    assert!(response.contains(HOSTILE));
    assert!(!response.contains("<script>"));
    assert_overview_counts(&response);
    assert_overview_status(&response)?;
    assert!(
        response.contains("<span title=\"restore_paused\">Order sending is paused after a restore")
    );
    assert!(response.contains("<span title=\"machine_silent\">Machine has gone quiet"));
    assert!(
        response
            .contains("<td>&#60;script&#62;responder(&#39;duty&#39;)&#38;&#60;/script&#62;</td>")
    );
    assert!(response.contains("<td>&#60;busy&#38;machine&#62;</td>"));
    assert!(response.contains(
        "<time datetime=\"1970-01-01T00:01:40Z\" title=\"1970-01-01T00:01:40Z (Unix 100)\">"
    ));
    assert!(!response.contains("cleared alert responder"));
    assert!(response.contains("<span class=\"node__state tone-bad\">Stopped in Analyte</span>"));
    // Last data is the stored receipt's time (Unix 200), exact on hover.
    assert!(response.contains(concat!(
        "<span class=\"mono\"><span class=\"sr-only\">Last data </span>",
        "<time datetime=\"1970-01-01T00:03:20Z\" ",
        "title=\"1970-01-01T00:03:20Z (Unix 200)\">"
    )));
    // How long ago shows beside the oldest waiting item (queued at Unix 102).
    assert!(response.contains(" days ago</span>"));
    assert!(response.contains(" · 2 items waiting · oldest <time datetime="));
    assert!(
        response
            .contains("<dd>2 <span class=\"figure__aside\">results and order messages</span></dd>")
    );
    let list = response
        .split("<div class=\"figure\">")
        .find(|figure| figure.contains("<dt>Order list · app</dt>"))
        .ok_or("order list figure missing")?;
    assert!(list.contains("tone-warn"), "{list}");
    assert!(list.contains(concat!(
        "Out of date <span class=\"figure__aside\">· checked ",
        "<time datetime=\"1970-01-01T00:01:40Z\""
    )));
    assert!(!response.contains("protected-raw-never-log"));
    assert!(!response.contains("protected-payload-never-log"));
    assert!(!response.contains("<script>sample</script>"));
    assert!(response.contains("<dt>Free disk space</dt>"));
    assert!(response.contains(" bytes free\">"));
    Ok(())
}

/// The fixture machine's name as the page escapes it.
const HOSTILE: &str = "&#60;script&#62;alert(&#39;hostile&#39;)&#38;&#34;&#60;/script&#62;";

/// The to-do line titled `title` names its fix as a button to `href` labelled `label`.
fn todo_item(verdict: &str, title: &str, href: &str, label: &str) -> bool {
    verdict.split("<li>").any(|item| {
        item.contains(&format!("<p class=\"todo__title\">{title}</p>"))
            && item.contains(&format!(
                "<a class=\"btn btn--outline\" href=\"{href}\">{label}</a>"
            ))
    })
}

/// Owner request 2026-11-19 (overview item 1): the answer to "is everything OK?"
/// comes first, one line per thing with its fix, before the detail sections.
fn assert_overview_status(response: &str) -> Result<(), Error> {
    let (top, details) = response
        .split_once("<section class=\"section\"")
        .ok_or("overview without sections")?;
    let (head, verdict) = top
        .split_once("</header>")
        .ok_or("overview without a page head")?;
    assert!(
        head.contains("<main class=\"main\" data-live-region=\"main\">"),
        "{head}"
    );
    assert!(head.contains("<h1>Overview</h1>"), "{head}");
    assert!(
        verdict
            .trim_start()
            .starts_with("<section class=\"verdict verdict--bad\""),
        "{verdict}"
    );
    assert!(
        verdict.contains("class=\"verdict__title\">4 things need you</h2>"),
        "{verdict}"
    );
    for (title, href, label) in [
        (
            format!("Analyte stopped working with {HOSTILE}"),
            "/machines",
            "Machines",
        ),
        (
            "Orders with unknown outcome: 2".into(),
            "/orders",
            "Machine orders",
        ),
        (
            "Order sending is paused after a restore".into(),
            "/#orders",
            "Orders",
        ),
        (
            "&#60;busy&#38;machine&#62; has gone quiet".into(),
            "/machines",
            "Machines",
        ),
    ] {
        assert!(
            todo_item(verdict, &title, href, label),
            "{title}\n{verdict}"
        );
    }
    assert!(!top.contains("All good"));
    for id in ["sending", "orders", "disk"] {
        assert!(response.contains(&format!("id=\"{id}\"")), "{id}");
    }
    assert!(details.contains("id=\"orders\""));
    Ok(())
}

/// Owner request 2026-11-19 (items 8 and 9): the paused line shows when it is Yes,
/// and counts are plain numbers, red above zero, with the renamed links.
fn assert_overview_counts(response: &str) {
    assert!(response.contains("<div class=\"alert alert--warn\" role=\"note\">"));
    assert!(response.contains(concat!(
        "<p class=\"alert__title\">Order sending is paused after a restore</p>",
        "<p class=\"alert__text\">Ask IT to reconcile orders"
    )));
    assert!(response.contains(concat!(
        "<span class=\"row__main\">2 orders with an unknown outcome (check the machine)</span>",
        "<a class=\"link\" href=\"/orders\">See machine orders</a>"
    )));
    assert!(response.contains(concat!(
        "<span class=\"row__main\">No orders cancelled by a machine</span>",
        "<a class=\"link\" href=\"/analyzer-cancelled\">Review cancelled orders</a>"
    )));
    assert!(response.contains("<dt>Active alerts</dt><dd class=\"num\">2</dd>"));
    assert!(!response.contains("class=\"pill"));
}

// ADR 0018 (admin restyle b6): every page is in the new look, so the old
// stylesheet is gone and its address answers "File not found" in the plain
// frame. The design's stylesheets hold no data, so they load before any session
// exists; the host check still applies.
async fn assert_stylesheet(host: &str, bearer: &str) -> Result<(), Error> {
    let root = crate::admin::view::assets::Root::of_this_build();
    let old = request(host, "/static/admin.css", "", None).await?;
    assert!(old.starts_with("HTTP/1.1 404"), "{old}");
    headers_present(&old);
    assert!(old.contains("<h1 id=\"rf-title\" class=\"panel__title\">File not found</h1>"));
    assert_stylesheets("/static/admin.css", &old);
    let css = request(host, "/static/app.css", "", None).await?;
    assert!(css.starts_with("HTTP/1.1 200"), "{css}");
    headers_present(&css);
    let lower = css.to_ascii_lowercase();
    assert!(lower.contains("content-type: text/css; charset=utf-8"));
    assert!(lower.contains("x-content-type-options: nosniff"));
    let mut stream = tokio::net::TcpStream::connect(host).await?;
    stream
        .write_all(
            format!(
                "GET {}app.css HTTP/1.1
Host: attacker.invalid
Connection: close

",
                root.as_str()
            )
            .as_bytes(),
        )
        .await?;
    let mut refused = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(6),
        stream.take(1_048_576).read_to_end(&mut refused),
    )
    .await??;
    assert!(String::from_utf8(refused)?.starts_with("HTTP/1.1 401"));
    let auth = format!(
        "Authorization: Bearer {bearer}
"
    );
    for (path, headers) in [
        ("/login", ""),
        ("/", auth.as_str()),
        ("/activity", auth.as_str()),
        ("/machines", auth.as_str()),
        ("/parked", auth.as_str()),
        ("/setup", auth.as_str()),
        ("/orders", auth.as_str()),
        ("/analyzer-cancelled", auth.as_str()),
        ("/retention", auth.as_str()),
        ("/deliveries", auth.as_str()),
        ("/settings", auth.as_str()),
    ] {
        let page = request(host, path, headers, None).await?;
        assert_stylesheets(path, &page);
        assert_logo(path, &page);
        assert!(!page.contains("style="), "{path} has an inline style");
        assert!(!page.contains("<style"), "{path} has a style block");
        assert_scripts(path, &page);
        assert_eq!(page.matches("</html>").count(), 1, "{path}");
        // Owner request 2026-11-19 (item 6): every page but the login has the
        // sidebar's main links and the More group, with its own page marked.
        let menu = [
            "/",
            "/activity",
            "/parked",
            "/machines",
            "/setup",
            "/orders",
            "/analyzer-cancelled",
            "/deliveries",
            "/retention",
            "/settings",
        ];
        if path == "/login" {
            assert!(!page.contains("<nav"), "the login page has a menu");
            continue;
        }
        for href in menu {
            assert!(
                page.contains(&format!("<a href=\"{href}\"")),
                "{path} lacks {href}"
            );
        }
        assert!(page.contains("<summary"), "{path} lacks More");
        assert!(
            page.contains(&format!("<a href=\"{path}\" aria-current=\"page\">")),
            "{path} is not marked"
        );
    }
    Ok(())
}

/// Owner decision 2026-11-20 (PR c2): one script file, live.js, on signed-in
/// pages; never an inline script, and none before sign-in.
fn assert_scripts(path: &str, page: &str) {
    if path == "/login" {
        assert!(!page.contains("<script"), "{path} has a script");
    } else {
        assert_eq!(page.matches("<script").count(), 1, "{path}");
        assert!(!other_script(page), "{path}");
    }
}

/// Owner request 2026-11-19 (Aperture logo, PR #273): every page links the
/// Analyte icon for its browser tab, the hand-tuned files at 16 and 20 px; the
/// sidebar draws the mark inline in the theme colours at 30 px, and sign-in at
/// 44 px as the `SignIn` mockup does.
fn assert_logo(path: &str, page: &str) {
    let root = crate::admin::view::assets::Root::of_this_build();
    for (file, sizes) in [
        ("analyte-app-icon-16.svg", "16x16"),
        ("analyte-app-icon-20.svg", "20x20"),
        (
            "analyte-app-icon.svg",
            "24x24 32x32 48x48 64x64 128x128 256x256",
        ),
    ] {
        let link = format!(
            "<link rel=\"icon\" type=\"image/svg+xml\" sizes=\"{sizes}\" href=\"{}icons/{file}\">",
            root.as_str()
        );
        assert!(page.contains(&link), "{path} lacks {link}");
    }
    let mark = if path == "/login" {
        "<svg class=\"logo\" width=\"44\" height=\"44\" "
    } else {
        "<div class=\"brand\">
<svg class=\"logo\" width=\"30\" height=\"30\" "
    };
    assert!(
        page.contains(&format!(
            "{mark}viewBox=\"0 0 48 48\" aria-hidden=\"true\"><path class=\"logo__tile\" "
        )),
        "{path} lacks the logo"
    );
}

/// ADR 0018: every page loads fonts.css, analyte.css and app.css, in that
/// order and under this build's version, and nothing else.
fn assert_stylesheets(path: &str, page: &str) {
    let root = crate::admin::view::assets::Root::of_this_build();
    let sheets: Vec<&str> = page
        .split("<link rel=\"stylesheet\" href=\"")
        .skip(1)
        .filter_map(|link| link.split('"').next())
        .collect();
    assert_eq!(
        sheets,
        ["fonts.css", "analyte.css", "app.css"].map(|name| format!("{}{name}", root.as_str())),
        "{path}"
    );
}

async fn assert_overview_login(host: &str, bearer: &str) -> Result<(String, String), Error> {
    let challenge = request(host, "/login", "", None).await?;
    let csrf = challenge
        .split("type=\"hidden\" name=\"csrf\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("CSRF challenge missing")?;
    let challenge_cookie = challenge
        .lines()
        .find_map(|s| s.strip_prefix("set-cookie: "))
        .and_then(|s| s.split(';').next())
        .ok_or("challenge cookie missing")?;
    let form = format!(
        "csrf={}&token={}",
        csrf.replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D"),
        bearer
            .replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D")
    );
    let login = request(
        host,
        "/login",
        &format!("Cookie: {challenge_cookie}\r\nOrigin: http://{host}\r\nContent-Type: application/x-www-form-urlencoded\r\n"),
        Some(&form),
    )
    .await?;
    assert!(login.starts_with("HTTP/1.1 303"));
    headers_present(&login);
    let cookie = login
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: "))
        .ok_or("session cookie missing")?;
    assert!(cookie.contains("HttpOnly; SameSite=Strict"));
    assert!(!cookie.contains("Domain="));
    assert!(!cookie.contains("Max-Age="));
    assert!(!cookie.contains("Secure"));
    assert!(Token::session()?.cookie(true).contains("; Secure"));
    let session = cookie.split(';').next().ok_or("cookie missing value")?;
    let response = request(host, "/", &format!("Cookie: {session}\r\n"), None).await?;
    assert!(response.starts_with("HTTP/1.1 200"));
    Ok((session.to_owned(), form))
}

async fn assert_overview_rejects_login_and_missing_route(
    host: &str,
    bearer: &str,
    form: &str,
) -> Result<(), Error> {
    let challenge = request(host, "/login", "", None).await?;
    let csrf = challenge
        .split("type=\"hidden\" name=\"csrf\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("CSRF challenge missing")?;
    let challenge_cookie = challenge
        .lines()
        .find_map(|s| s.strip_prefix("set-cookie: "))
        .and_then(|s| s.split(';').next())
        .ok_or("challenge cookie missing")?;
    let bad_login = request(
        host,
        "/login",
        &format!(
            "Cookie: {challenge_cookie}\r\nContent-Type: application/x-www-form-urlencoded\r\n"
        ),
        Some(&format!(
            "csrf={}&token=wrong-login-secret",
            csrf.replace('+', "%2B")
                .replace('/', "%2F")
                .replace('=', "%3D")
        )),
    )
    .await?;
    assert!(bad_login.starts_with("HTTP/1.1 401"));
    headers_present(&bad_login);
    let cross_origin = request(
        host,
        "/login",
        "Origin: https://attacker.invalid\r\nContent-Type: application/x-www-form-urlencoded\r\n",
        Some(form),
    )
    .await?;
    assert!(cross_origin.starts_with("HTTP/1.1 401"));
    // An opaque origin stays refused: the referrer policy, not a looser check,
    // is what lets real browsers send their own origin on our forms.
    let opaque_origin = request(
        host,
        "/login",
        "Origin: null
Content-Type: application/x-www-form-urlencoded
",
        Some(form),
    )
    .await?;
    assert!(opaque_origin.starts_with("HTTP/1.1 401"));
    let missing = request(
        host,
        "/missing",
        &format!("Authorization: Bearer {bearer}\r\n"),
        None,
    )
    .await?;
    assert!(missing.starts_with("HTTP/1.1 404"));
    headers_present(&missing);
    Ok(())
}

/// A set-up machine for page tests, attached as its light's link says and
/// sending to lab app `lab_app`. A listening one accepts only 192.168.1.50,
/// as a machine set up with the connection test does (ADR 0026).
fn machine(
    id: &str,
    running: bool,
    status: watch::Receiver<crate::machine::link::status::Status>,
    lab_app: &str,
) -> Result<Machine, Error> {
    use crate::machine::link::status::Link;
    let connection = match status.borrow().link() {
        Link::Serial => Connection::Serial("COM3".into()),
        Link::TcpServer => Connection::Listening(Some(5600)),
        Link::TcpClient => Connection::Dialing,
        Link::Folder => Connection::Folder("results".into()),
    };
    Ok(Machine {
        software: None,
        rules: Vec::new(),
        digest: [0xab; 32],
        id: Identifier::new(id)?,
        running: Arc::new(AtomicBool::new(running)),
        status,
        model: Some("Synthetic analyzer".into()),
        allowed_peers: matches!(connection, Connection::Listening(_))
            .then(|| analyte_config::AllowedPeers::parse(&["192.168.1.50"]))
            .transpose()?,
        choices: None,
        cable: matches!(connection, Connection::Serial(_)).then(|| {
            "9600 baud, 8 data bits, no parity, 1 stop bit, no flow control; as the manual suggests"
                .to_owned()
        }),
        connection,
        lab_app: Some(Identifier::new(lab_app)?),
    })
}

/// A lab app destination on a closed loopback port, for pages that only read its health.
fn loopback_destination(name: &str) -> Result<analyte_sink::Destination, Error> {
    Ok(analyte_sink::Destination::for_loopback_test(
        Identifier::new(name)?,
        "http://127.0.0.1:1",
        Identifier::new(name)?,
        analyte_sink::Limits {
            request_timeout: Duration::from_secs(1),
            store_timeout: Duration::from_secs(1),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(1),
            freshness: Duration::from_secs(30),
            poll_interval: Duration::from_secs(5),
            page_size: 1,
            max_pages: 1,
        },
    )?)
}

/// A test server's owner task.
type Owner = tokio::task::JoinHandle<Result<(), crate::admin::Error>>;

/// A running test server: its state, stop switch, owner task and `host:port`.
type Started = (Arc<Context>, watch::Sender<bool>, Owner, String);

/// Runs a fixture's server on `socket`: `state`'s router and, on Windows, the
/// real sign-in pipe for its port, handing out `state`'s codes. Returns its
/// stop switch and owner task.
fn run_fixture(
    socket: tokio::net::TcpListener,
    state: Arc<Context>,
    restart: watch::Receiver<bool>,
    closing: watch::Sender<bool>,
    released: tokio::sync::oneshot::Receiver<()>,
) -> Result<(watch::Sender<bool>, Owner), Error> {
    let server = Server {
        #[cfg(windows)]
        windows: windows::start(socket.local_addr()?.port()),
        #[cfg(windows)]
        sign_in: state.windows.clone(),
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state),
        restart,
        closing,
        released,
        owners: BTreeMap::new(),
    };
    let (stop, stopped) = watch::channel(false);
    Ok((stop, tokio::spawn(server.run(stopped))))
}

async fn start_overview_server(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
    machines: Vec<Machine>,
    destinations: Vec<analyte_sink::Destination>,
) -> Result<Started, Error> {
    start_with(
        root,
        store,
        (machines, destinations),
        crate::admin::Draining::new(),
        root,
    )
    .await
}

/// [`start_overview_server`] with kept lab apps and the data folder whose free
/// space the overview reads.
async fn start_with(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
    parts: (Vec<Machine>, Vec<analyte_sink::Destination>),
    retired: crate::admin::Draining,
    directory: &std::path::Path,
) -> Result<Started, Error> {
    start_full(
        root,
        store,
        parts,
        (retired, directory),
        (
            Vec::new(),
            crate::admin::more::Installed::for_test(Vec::new()),
        ),
    )
    .await
}

/// [`start_with`] with each machine's data keeping and the Settings page's values.
async fn start_full(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
    parts: (Vec<Machine>, Vec<analyte_sink::Destination>),
    kept: (crate::admin::Draining, &std::path::Path),
    settings: (
        Vec<(Identifier, Option<u16>)>,
        crate::admin::more::Installed,
    ),
) -> Result<Started, Error> {
    start_bounded(
        root,
        store,
        (parts, kept, settings),
        crate::admin::server::live::Bounds::standard(),
    )
    .await
}

/// The parts a test server is started with: machines and lab apps, kept lab
/// apps and the data folder, and each machine's data keeping with the Settings
/// page's values.
type Parts<'a> = (
    (Vec<Machine>, Vec<analyte_sink::Destination>),
    (crate::admin::Draining, &'a std::path::Path),
    (
        Vec<(Identifier, Option<u16>)>,
        crate::admin::more::Installed,
    ),
);

/// [`start_full`] with the live streams' `bounds`.
async fn start_bounded(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
    ((machines, destinations), (retired, directory), (retention, installed)): Parts<'_>,
    bounds: crate::admin::server::live::Bounds,
) -> Result<Started, Error> {
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let (released, release) = tokio::sync::oneshot::channel();
    let (closing, closed) = watch::channel(false);
    let state = Arc::new(Context {
        alert_endpoints: vec![],
        retention,
        token: Token::load(root)?,
        sessions: Sessions::default(),
        windows: Arc::new(windows::SignIn::new(Some(windows::Status::On))),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines,
        destinations,
        retired,
        directory: directory.to_owned(),
        reserve: 1,
        setup: Arc::new(crate::admin::setup::Setup::for_test(
            &root.join("program.toml"),
            root,
        )),
        assets: crate::admin::view::assets::Root::of_this_build(),
        installed,
        write_budget: WRITE_BUDGET,
        live: bounds,
        closing: closed,
        _released: released,
    });
    let (stop, owner) = run_fixture(
        socket,
        state.clone(),
        watch::channel(false).1,
        closing,
        release,
    )?;
    Ok((state, stop, owner, host))
}

/// Classifies failures while constructing and inspecting synthetic admin fixtures.
#[derive(Debug, thiserror::Error)]
enum FixtureError {
    #[error(transparent)]
    Maintenance(#[from] analyte_store::MaintenanceError),
    #[error(transparent)]
    Intake(#[from] analyte_store::IntakeError),
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    #[error(transparent)]
    Reconciliation(#[from] analyte_store::ReconciliationError),
    #[error(transparent)]
    Operator(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    #[error(transparent)]
    Audit(#[from] analyte_store::StorageError),
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    #[error(transparent)]
    Identity(#[from] analyte_model::InvalidUuid),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    EvidenceTimestamp(#[from] jiff::Error),
    #[error("identity or state constraint conflict")]
    Conflict,
    #[error("stored evidence is corrupt")]
    Corruption,
    #[error("receipt does not exist")]
    UnknownReceipt,
    #[error("delivery does not exist")]
    UnknownDelivery,
    #[error("audit evidence does not exist")]
    UnknownAudit,
    #[error("order document lacks {field}")]
    MissingOrderField { field: &'static str },
}
