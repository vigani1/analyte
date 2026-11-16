//! Tests server behavior with synthetic fixtures.
use super::*;
use analyte_model::{OrderState, Reason};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

mod actions;
mod pages;

struct Space;
impl analyte_store::FreeSpace for Space {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

// Fixture migration/seeding is not an assertion about production latency.
fn fixture_deadline() -> Result<std::time::Instant, Error> {
    std::time::Instant::now()
        .checked_add(Duration::from_secs(30))
        .ok_or_else(|| "admin fixture deadline overflow".into())
}

// Plan section 11: the actual program refuses absent tokens before opening the
// endpoint, owns the enabled server, and joins it before closing the store.
#[tokio::test(flavor = "multi_thread")]
async fn plan11_service_admin_missing_token_and_owned_lifecycle() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    std::fs::write(
        directory.path().join("machine.toml"),
        include_str!("../../../../../core/config/examples/hl7.toml"),
    )?;
    let analyzer = std::net::TcpListener::bind("127.0.0.1:0")?;
    let analyzer_address = analyzer.local_addr()?.to_string();
    std::fs::write(
        directory.path().join("site.toml"),
        include_str!("../../../../../core/config/examples/site.toml")
            .replace("127.0.0.1:5600", &analyzer_address),
    )?;
    let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = reserved.local_addr()?;
    let file = directory.path().join("program.toml");
    std::fs::write(
        &file,
        format!(
            "schema_version = 1\ndata_directory = 'data'\n[admin]\nport = {}\n[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n[store]\nreserve_bytes = 1\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 1\nbusy_timeout_ms = 100\n",
            address.port()
        ),
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
            failure.downcast_ref::<super::super::TokenError>(),
            Some(super::super::TokenError::Missing)
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
    assert!(response.contains("Machine owner</dt><dd>running"));
    assert!(response.contains("Last received activity</dt><dd>unknown"));
    headers_present(&response);
    stop.send(()).map_err(|_| "service stop receiver lost")?;
    tokio::time::timeout(Duration::from_secs(10), owner).await???;
    assert!(tokio::net::TcpStream::connect(&host).await.is_err());
    Ok(())
}

async fn request(
    host: &str,
    path: &str,
    headers: &str,
    body: Option<&str>,
) -> Result<String, Error> {
    tokio::time::timeout(Duration::from_secs(6), async {
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
        "referrer-policy: no-referrer",
        "cache-control: no-store",
    ] {
        assert!(lower.contains(expected), "missing {expected}");
    }
}

// Plan sections 11/12 and rule 10: real HTTP proves authorization, escaping,
// bounded metadata reads, honest health and no credential/payload in logs.
#[tokio::test(flavor = "multi_thread")]
async fn plan11_plan12_overview_auth_headers_escaping_and_shutdown() -> Result<(), Error> {
    let logs = crate::runtime::test_tracing::init();
    let directory = crate::admin::private_tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    let bearer = super::super::rotate_token(directory.path())?;
    let limits = analyte_store::Limits {
        reserve_bytes: 1,
        database_bytes: 100_000_000,
        record_bytes: 65536,
        page_rows: 1,
        busy_timeout: Duration::from_millis(100),
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
    let (state, stop, owner, host) = start_overview_server(directory.path(), &store).await?;
    let wrong = "wrong-secret-never-log-this";
    assert_overview_refuses_auth(&host, wrong).await?;
    assert_overview_health(&host, &bearer).await?;
    let (session, form) = assert_overview_login(&host, &bearer).await?;
    assert_overview_rejects_login_and_missing_route(&host, &bearer, &form).await?;
    // A peer that never finishes its headers must not prevent bounded shutdown.
    let mut slow = tokio::net::TcpStream::connect(&host).await?;
    slow.write_all(b"GET / HTTP/1.1\r\n").await?;
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
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
#[tokio::test]
async fn plan11_admin_drain_waits_for_router_state_release() -> Result<(), Error> {
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
    tokio::time::timeout(Duration::from_secs(5), async {
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
        .request(fixture_deadline()?, move |store| {
            store.evaluate_alert(
                &analyte_store::AlertKey::RestorePaused,
                "<script>responder('duty')&</script>",
                true,
                Some(1),
                100,
            )?;
            store.evaluate_alert(
                &analyte_store::AlertKey::MachineSilent(
                    Identifier::new("<busy&machine>").map_err(analyte_store::Error::Text)?,
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
            let id = |value| Identifier::new(value).map_err(analyte_store::Error::Text);
            store.audit(&analyte_store::Audit {
                actor: id("operator")?,
                action: id("store-restore")?,
                targets: Vec::new(),
                time: 103,
                reason: Reason::new("synthetic restore").map_err(analyte_store::Error::Text)?,
            })?;
            Ok(())
        })
        .await?;
    Ok(())
}

fn seed_overview_order(
    store: &mut analyte_store::Store,
    order: &str,
) -> Result<(), analyte_store::Error> {
    let id = |value| Identifier::new(value).map_err(analyte_store::Error::Text);
    let raw = b"protected-raw-never-log";
    let config = b"synthetic-config";
    store.receive(
        &analyte_model::ReceiptMetadata {
            id: analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())
                .map_err(|_| analyte_store::Error::InvalidHostInput)?,
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
                .map_err(|_| analyte_store::Error::InvalidHostInput)?,
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
                            .map_err(|_| analyte_store::Error::InvalidHostInput)?,
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
                identity: &key,
                version: std::num::NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            analyte_store::ReserveOptions::default(),
        )?
        .map_err(|_| analyte_store::Error::Conflict)?;
    store.transition_order(
        reserved.id,
        OrderState::Uncertain,
        analyte_model::SourceText::new("timeout").map_err(analyte_store::Error::Text)?,
        102,
        |order| {
            Ok(analyte_store::OrderEventDelivery {
                id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())
                    .map_err(|_| analyte_store::Error::InvalidHostInput)?,
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
    assert!(response.contains("&lt;script&gt;alert(&#39;hostile&#39;)&amp;&quot;&lt;/script&gt;"));
    assert!(!response.contains("<script>"));
    assert!(response.contains("Order sending paused after restore</dt><dd>true"));
    assert!(response.contains("Uncertain orders</dt><dd>2"));
    assert!(response.contains("Active alerts</dt><dd>2"));
    assert!(response.contains("Alert</dt><dd>restore_paused"));
    assert!(response.contains("Alert</dt><dd>machine_silent"));
    assert!(
        response.contains(
            "Responder</dt><dd>&lt;script&gt;responder(&#39;duty&#39;)&amp;&lt;/script&gt;"
        )
    );
    assert!(response.contains("Alert machine</dt><dd>&lt;busy&amp;machine&gt;"));
    assert!(response.contains("Raised age (seconds)</dt><dd>"));
    assert!(!response.contains("cleared alert responder"));
    assert!(response.contains("Machine owner</dt><dd>stopped"));
    assert!(response.contains("Last received activity</dt><dd>1970-01-01T00:03:20Z"));
    assert!(response.contains("Pending items</dt><dd>2"));
    assert!(response.contains("Cache fresh and trusted</dt><dd>false"));
    assert!(response.contains("Cache last successful poll (Unix seconds)</dt><dd>100"));
    assert!(!response.contains("protected-raw-never-log"));
    assert!(!response.contains("protected-payload-never-log"));
    assert!(!response.contains("<script>sample</script>"));
    assert!(response.contains("Free disk bytes"));
    Ok(())
}

async fn assert_overview_login(host: &str, bearer: &str) -> Result<(String, String), Error> {
    let challenge = request(host, "/login", "", None).await?;
    let csrf = challenge
        .split("name=\"csrf\" type=\"hidden\" value=\"")
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
        .split("name=\"csrf\" type=\"hidden\" value=\"")
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

async fn start_overview_server(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
) -> Result<
    (
        Arc<Context>,
        watch::Sender<bool>,
        tokio::task::JoinHandle<Result<(), Error>>,
        String,
    ),
    Error,
> {
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let (released, release) = tokio::sync::oneshot::channel();
    let state = Arc::new(Context {
        alert_endpoints: vec![],
        retention: Vec::new(),
        config_path: root.join("program.toml"),
        token: Token::load(root)?,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines: vec![Machine {
            departure_html: String::new(),
            id: Identifier::new("<script>alert('hostile')&\"</script>")?,
            running: Arc::new(AtomicBool::new(false)),
        }],
        destinations: vec![analyte_sink::Destination::for_loopback_test(
            Identifier::new("app")?,
            "http://127.0.0.1:1",
            Identifier::new("app")?,
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
        )?],
        directory: root.to_owned(),
        reserve: 1,
        _released: released,
    });
    let server = Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state.clone()),
        released: release,
        owners: BTreeMap::new(),
    };
    let (stop, stopped) = watch::channel(false);
    let owner = tokio::spawn(server.run(stopped));
    Ok((state, stop, owner, host))
}
