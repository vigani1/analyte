use super::*;
use analyte_model::{OrderState, Reason};
use std::{io::Write, sync::Mutex};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

mod pages;

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("log capture poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Space;
impl analyte_store::FreeSpace for Space {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

// Plan section 11: the actual program refuses absent tokens before opening the
// endpoint, owns the enabled server, and joins it before closing the store.
#[tokio::test(flavor = "multi_thread")]
async fn plan11_service_admin_missing_token_and_owned_lifecycle() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("machine.toml"),
        include_str!("../../../../config/examples/hl7.toml"),
    )?;
    let analyzer = std::net::TcpListener::bind("127.0.0.1:0")?;
    let analyzer_address = analyzer.local_addr()?.to_string();
    std::fs::write(
        directory.path().join("site.toml"),
        include_str!("../../../../config/examples/site.toml")
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
        crate::secrets::EnvironmentSecrets::load_with(program, |name| {
            Some(if name.ends_with("SIGNING") {
                "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()
            } else {
                "synthetic-api-key".into()
            })
        })
    };
    drop(reserved);
    let missing = program()?;
    let injected = secrets(&missing)?;
    let failure = crate::service::serve_loopback_test(
        missing,
        Box::new(Space),
        std::future::pending(),
        injected,
    )
    .await
    .err()
    .ok_or("missing credential unexpectedly admitted")?;
    assert!(matches!(
        failure.downcast_ref::<super::super::TokenError>(),
        Some(super::super::TokenError::Missing)
    ));
    let token = super::super::rotate_token(&directory.path().join("data"))?;
    let program = program()?;
    let injected = secrets(&program)?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let owner = tokio::spawn(crate::service::serve_loopback_test(
        program,
        Box::new(Space),
        async {
            stopped.await?;
            Ok(())
        },
        injected,
    ));
    let host = address.to_string();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if tokio::net::TcpStream::connect(&host).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
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
    let logs = Capture(Arc::new(Mutex::new(Vec::new())));
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    let directory = tempfile::tempdir()?;
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
            crate::deadline(),
        )
        .await?,
    );
    store
        .request(crate::deadline(), move |store| {
            // Synthetic read fixtures: two pages ensure the overview does not report
            // a first-page count as a total. No machine commands are invoked.
            let id = |value| Identifier::new(value).map_err(|_| analyte_store::Error::InvalidInput);
            for order in ["first", "second"] {
                let raw = b"protected-raw-never-log";
                let config = b"synthetic-config";
                store.receive(
                    &analyte_model::ReceiptMetadata {
                        id: analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        raw_digest: analyte_model::Digest(analyte_contract::content_digest(raw)),
                        connection: id("synthetic")?,
                        machine: id("<script>alert('hostile')&\"</script>")?,
                        driver: analyte_model::NamespacedId {
                            namespace: id("synthetic")?,
                            value: id("1")?,
                        },
                        config_version: id("1")?,
                        config_digest: analyte_model::Digest(analyte_contract::content_digest(
                            config,
                        )),
                        received_at: jiff::Timestamp::from_second(if order == "first" {
                            100
                        } else {
                            200
                        })
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
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
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        document: "{\"version\":1,\"state\":\"requested\",\"tombstone\":false}"
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
                    .reserve_send(&key, std::num::NonZeroU64::MIN, 101, 30)?
                    .map_err(|_| analyte_store::Error::Conflict)?;
                store.transition_order(
                    reserved.id,
                    OrderState::Uncertain,
                    analyte_model::SourceText::new("timeout")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    102,
                    |order| {
                        Ok(analyte_store::OrderEventDelivery {
                            id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())
                                .map_err(|_| analyte_store::Error::InvalidInput)?,
                            destination: order.key.destination.clone(),
                            order_id: order.key.order.clone(),
                            payload: b"protected-payload-never-log".to_vec(),
                            contract_version: 1,
                        })
                    },
                )?;
            }
            store.audit(&analyte_store::Audit {
                actor: id("operator")?,
                action: id("store-restore")?,
                targets: Vec::new(),
                time: 103,
                reason: Reason::new("synthetic restore")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
            })?;
            Ok(())
        })
        .await?;
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let state = Arc::new(Context {
        token: Token::load(directory.path())?,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines: vec![Machine {
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
        directory: directory.path().to_owned(),
        reserve: 1,
    });
    let server = Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state.clone()),
        owners: BTreeMap::new(),
    };
    let (stop, stopped) = watch::channel(false);
    let owner = tokio::spawn(server.run(stopped));
    let wrong = "wrong-secret-never-log-this";
    for auth in [
        String::new(),
        format!("Authorization: Bearer {wrong}\r\n"),
        "Cookie: analyte_session=wrong-cookie-secret\r\n".into(),
    ] {
        let response = request(&host, "/", &auth, None).await?;
        assert!(response.starts_with("HTTP/1.1 401"));
        assert!(!response.contains(wrong));
        headers_present(&response);
    }
    let response = request(
        &host,
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
    assert!(response.contains("Machine owner</dt><dd>stopped"));
    assert!(response.contains("Last received activity</dt><dd>1970-01-01T00:03:20Z"));
    assert!(response.contains("Pending items</dt><dd>2"));
    assert!(response.contains("Cache fresh and trusted</dt><dd>false"));
    assert!(response.contains("Cache last successful poll (Unix seconds)</dt><dd>100"));
    assert!(!response.contains("protected-raw-never-log"));
    assert!(!response.contains("protected-payload-never-log"));
    assert!(!response.contains("<script>sample</script>"));
    assert!(response.contains("Free disk bytes"));
    let challenge = request(&host, "/login", "", None).await?;
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
        &host,
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
    let response = request(&host, "/", &format!("Cookie: {session}\r\n"), None).await?;
    assert!(response.starts_with("HTTP/1.1 200"));
    let challenge = request(&host, "/login", "", None).await?;
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
        &host,
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
        &host,
        "/login",
        "Origin: https://attacker.invalid\r\nContent-Type: application/x-www-form-urlencoded\r\n",
        Some(&form),
    )
    .await?;
    assert!(cross_origin.starts_with("HTTP/1.1 401"));
    let missing = request(
        &host,
        "/missing",
        &format!("Authorization: Bearer {bearer}\r\n"),
        None,
    )
    .await?;
    assert!(missing.starts_with("HTTP/1.1 404"));
    headers_present(&missing);
    // A peer that never finishes its headers must not prevent bounded shutdown.
    let mut slow = tokio::net::TcpStream::connect(&host).await?;
    slow.write_all(b"GET / HTTP/1.1\r\n").await?;
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    drop(slow);
    drop(state);
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retained store after shutdown")?;
    store.shutdown(crate::deadline()).await?;
    let captured = logs.0.lock().map_err(|_| "log capture poisoned")?;
    let captured = String::from_utf8_lossy(&captured);
    assert!(captured.contains("admin authentication refused"));
    for secret in [
        wrong,
        "wrong-cookie-secret",
        "wrong-login-secret",
        &bearer,
        session,
        "hostile",
        "protected-raw-never-log",
        "protected-payload-never-log",
    ] {
        assert!(!captured.contains(secret));
    }
    Ok(())
}
