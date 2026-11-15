use super::*;
use analyte_model::{Digest, NamespacedId, ReceiptId, ReceiptMetadata, SampleId};
use analyte_store::{Audit, CachedOrder, OrderPage, Parked};

pub(super) fn encode(text: &str) -> String {
    text.bytes().map(|byte| format!("%{byte:02X}")).collect()
}
pub(super) fn csrf(html: &str) -> Result<String, Error> {
    Ok(html
        .split("name=\"csrf\"")
        .nth(1)
        .and_then(|s| s.split("value=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .ok_or("CSRF field missing")?
        .to_owned())
}
pub(super) fn cookie(response: &str) -> Result<String, Error> {
    Ok(response
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: "))
        .and_then(|line| line.split(';').next())
        .ok_or("cookie missing")?
        .to_owned())
}

pub(super) async fn login(host: &str, bearer: &str) -> Result<String, Error> {
    let challenge = request(host, "/login", "", None).await?;
    assert!(challenge.starts_with("HTTP/1.1 401"));
    let csrf = csrf(&challenge)?;
    let challenge_cookie = cookie(&challenge)?;
    let response = request(
        host,
        "/login",
        &format!(
            "Cookie: {challenge_cookie}\r\nContent-Type: application/x-www-form-urlencoded\r\n"
        ),
        Some(&format!("csrf={}&token={}", encode(&csrf), encode(bearer))),
    )
    .await?;
    assert!(response.starts_with("HTTP/1.1 303"), "{response}");
    cookie(&response)
}

// Plan section 11, rules 2/10: lists never disclose bytes; every raw page needs
// its session's CSRF POST and a durable audit, including repeated views.
#[tokio::test(flavor = "multi_thread")]
async fn plan11_raw_audit_csrf_pagination_orders_and_restore_pause() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let limits = analyte_store::Limits {
        reserve_bytes: 1,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
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
        .map_err(|error| format!("admin pages fixture migration: {error}"))?,
    );
    let (receipt_id, parked_id, other_receipt) = store.request(fixture_deadline()?, |store| {
        let id = |text: &str| Identifier::new(text).map_err(|_| analyte_store::Error::InvalidInput);
        let mut first = None;
        for name in ["first", "second"] {
            let mut raw = b"<script>RAW&\"'\x00\xff".to_vec(); raw.extend(vec![b'A'; 4100]); raw.extend(b"TAIL");
            let config = b"config";
            let receipt_id = ReceiptId::try_from(uuid::Uuid::now_v7()).map_err(|_| analyte_store::Error::InvalidInput)?;
            store.receive(&ReceiptMetadata {id: receipt_id, raw_digest: Digest(analyte_contract::content_digest(&raw)), connection: id("test")?, machine: id("machine")?, driver: NamespacedId {namespace: id("synthetic")?, value: id("1")?}, config_version: id("1")?, config_digest: Digest(analyte_contract::content_digest(config)), received_at: jiff::Timestamp::UNIX_EPOCH}, &raw, config)?;
            let target = id(&uuid::Uuid::from(receipt_id).to_string())?;
            let parked_id = store.park(&Parked {kind: id(name)?, reason: Reason::new("<script>hostile reason</script>").map_err(|_| analyte_store::Error::InvalidInput)?, targets: vec![target]})?;
            store.order_page(&OrderPage {destination: id("app")?, cursor: id(name)?, complete: true, polled: 100, orders: vec![CachedOrder {id: id(name)?, sample: SampleId::new("<script>sample</script>").map_err(|_| analyte_store::Error::InvalidInput)?, document: serde_json::json!({"id": name,"version":1,"state":"requested","sample_id":"<script>sample</script>","tests":[{"namespace":"local","value":"<test>"}],"priority":"routine","target_analyzers":null,"expires_at":null,"tombstone":false}).to_string(), tombstone: false, expiry: 200}]})?;
            let key = analyte_store::MachineOrderKey {destination: id("app")?, order: id(name)?, machine: id("machine")?};
            let record = store.reserve_send(&key, std::num::NonZeroU64::MIN, 101, 30)?.map_err(|_| analyte_store::Error::Conflict)?;
            store.transition_order(record.id, OrderState::Uncertain, analyte_model::SourceText::new("timeout").map_err(|_| analyte_store::Error::InvalidInput)?, 102, |order| Ok(analyte_store::OrderEventDelivery {id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7()).map_err(|_| analyte_store::Error::InvalidInput)?, destination: order.key.destination.clone(), order_id: order.key.order.clone(), payload: b"not a raw view".to_vec(), contract_version: 1}))?;
            if let Some((receipt, parked)) = first {
                store.audit(&Audit {actor: id("operator")?, action: id("store-restore")?, targets: Vec::new(), time: 103, reason: Reason::new("synthetic restore").map_err(|_| analyte_store::Error::InvalidInput)?})?;
                return Ok((receipt, parked, receipt_id));
            }
            first = Some((receipt_id, parked_id));
        }
        Err(analyte_store::Error::InvalidInput)
    }).await?;
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let mut machines = Vec::new();
    for index in 0..61 {
        machines.push(Machine {
            id: Identifier::new(&format!("machine-{index}"))?,
            running: Arc::new(AtomicBool::new(true)),
        });
    }
    let (released, release) = tokio::sync::oneshot::channel();
    let state = Arc::new(Context {
        config_path: directory.path().join("program.toml"),
        token: Token::load(directory.path())?,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines,
        destinations: Vec::new(),
        directory: directory.path().to_owned(),
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
    let session = login(&host, &bearer).await?;
    let auth = format!("Cookie: {session}\r\n");
    for (path, sections) in [
        ("/parked", 1),
        ("/orders", 1),
        ("/machines", 50),
        ("/machines?after=50", 11),
    ] {
        let response = request(&host, path, &auth, None).await?;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        headers_present(&response);
        assert_eq!(response.matches("<section>").count(), sections);
        assert!(!response.contains("RAW"));
        assert!(!response.contains("TAIL"));
    }
    let list = request(&host, "/parked", &auth, None).await?;
    assert!(list.contains("&lt;script&gt;hostile reason&lt;/script&gt;"));
    assert!(list.contains("1970-01-01T00:00:00Z"));
    let page2 = request(&host, &format!("/parked?after={parked_id}"), &auth, None).await?;
    assert!(page2.contains("Category: second"));
    assert!(!page2.contains("Category: first"));
    let orders = request(&host, "/orders", &auth, None).await?;
    assert!(orders.contains("Uncertain"));
    assert!(orders.contains("Order sending paused after restore: true"));
    assert!(orders.contains("&lt;script&gt;sample&lt;/script&gt;"));
    assert!(orders.contains("&lt;test&gt;"));
    let orders2 = request(&host, "/orders?after=1", &auth, None).await?;
    assert!(orders2.contains("Order: second"));
    assert!(!orders2.contains("Order: first"));
    for route in ["/machines", "/orders", "/parked"] {
        for query in [
            "after=-1",
            "after=18446744073709551615",
            "after=1&limit=100000",
        ] {
            let invalid = request(&host, &format!("{route}?{query}"), &auth, None).await?;
            assert!(invalid.starts_with("HTTP/1.1 400"), "{invalid}");
            headers_present(&invalid);
        }
    }
    let detail = request(&host, &format!("/parked/{parked_id}"), &auth, None).await?;
    assert!(detail.starts_with("HTTP/1.1 200"));
    assert!(!detail.contains("RAW"));
    assert!(!detail.contains("TAIL"));
    let csrf = csrf(&detail)?;
    let form = format!(
        "csrf={}&target={}&source=receipt&after=0&reason=review",
        encode(&csrf),
        encode(&uuid::Uuid::from(receipt_id).to_string())
    );
    let raw_path = format!("/parked/{parked_id}/raw");
    let post_auth = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let get = request(&host, &raw_path, &auth, None).await?;
    assert!(get.starts_with("HTTP/1.1 405"));
    for rejected in [
        form.replace(&encode(&csrf), "wrong"),
        form.replace(&format!("csrf={}&", encode(&csrf)), ""),
    ] {
        let response = request(&host, &raw_path, &post_auth, Some(&rejected)).await?;
        assert!(response.starts_with("HTTP/1.1 403"));
        assert!(!response.contains("RAW"));
    }
    let other_session = login(&host, &bearer).await?;
    let wrong_session = request(
        &host,
        &raw_path,
        &format!("Cookie: {other_session}\r\nContent-Type: application/x-www-form-urlencoded\r\n"),
        Some(&form),
    )
    .await?;
    assert!(wrong_session.starts_with("HTTP/1.1 403"));
    let unrelated = request(
        &host,
        &raw_path,
        &post_auth,
        Some(&form.replace(
            &encode(&uuid::Uuid::from(receipt_id).to_string()),
            &encode(&uuid::Uuid::from(other_receipt).to_string()),
        )),
    )
    .await?;
    assert!(!unrelated.starts_with("HTTP/1.1 200"));
    assert!(!unrelated.contains("RAW"));
    let count = store
        .request(fixture_deadline()?, |s| Ok(s.audits(1)?.len()))
        .await?;
    assert_eq!(count, 0);
    for _ in 0..2 {
        let response = request(&host, &raw_path, &post_auth, Some(&form)).await?;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        headers_present(&response);
        assert!(response.contains("&lt;script&gt;RAW&amp;&quot;&#39;"));
        assert!(!response.contains("<script>"));
        assert!(response.contains("3c 73 63 72 69 70 74 3e"));
        assert!(!response.contains("TAIL"));
    }
    let tail = request(
        &host,
        &raw_path,
        &post_auth,
        Some(&form.replace("after=0", "after=4096")),
    )
    .await?;
    assert!(tail.starts_with("HTTP/1.1 200"));
    assert!(tail.contains("TAIL"));
    let first_audit = store.request(fixture_deadline()?, |s| s.audits(1)).await?;
    let (sequence, audit) = first_audit.first().ok_or("raw audit absent")?;
    assert_eq!(audit.action.as_str(), "admin-raw-view");
    assert_eq!(audit.reason.as_str(), "review");
    assert!(audit.actor.as_str().starts_with("admin-session-"));
    assert!(
        audit
            .targets
            .iter()
            .any(|t| t.as_str() == format!("parked:{parked_id}"))
    );
    let sequence = *sequence;
    let second = store
        .request(fixture_deadline()?, move |s| s.audits(sequence))
        .await?;
    assert_eq!(second.len(), 1);
    let sequence = second.first().ok_or("second audit missing")?.0;
    assert_eq!(
        store
            .request(fixture_deadline()?, move |s| Ok(s.audits(sequence)?.len()))
            .await?,
        1
    );
    let actor = state
        .sessions
        .lookup(session.strip_prefix("analyte_session="))?
        .ok_or("session lost")?
        .actor
        .clone();
    assert_eq!(audit.actor, actor);
    // Current cache evidence is explicitly distinguished from a reservation's
    // historical version; operator reads never resend or reconcile an order.
    store
        .request(fixture_deadline()?, |s| {
            let destination =
                Identifier::new("app").map_err(|_| analyte_store::Error::InvalidInput)?;
            let key = Identifier::new("first").map_err(|_| analyte_store::Error::InvalidInput)?;
            let mut cached = s
                .cached_order(&destination, &key)?
                .ok_or(analyte_store::Error::Conflict)?;
            let mut document: serde_json::Value = serde_json::from_str(&cached.document)?;
            *document
                .get_mut("version")
                .ok_or(analyte_store::Error::InvalidInput)? = serde_json::json!(2);
            *document
                .get_mut("tests")
                .ok_or(analyte_store::Error::InvalidInput)? =
                serde_json::json!([{"namespace":"local","value":"<newtest>"}]);
            cached.document = serde_json::to_string(&document)?;
            s.order_page(&OrderPage {
                destination,
                cursor: Identifier::new("new").map_err(|_| analyte_store::Error::InvalidInput)?,
                complete: true,
                polled: 200,
                orders: vec![cached],
            })
        })
        .await?;
    let mismatch = request(&host, "/orders", &auth, None).await?;
    assert!(mismatch.contains("Different version; these are not the historical machine items."));
    assert!(mismatch.contains("Uncertain"));
    assert!(mismatch.contains("Machine test: local: &lt;test&gt;"));
    assert!(mismatch.contains("&lt;newtest&gt;"));
    let frame_park = store
        .request(fixture_deadline()?, |s| {
            let transfer =
                Identifier::new("transfer").map_err(|_| analyte_store::Error::InvalidInput)?;
            let mut bytes = b"<frame>".to_vec();
            bytes.extend(vec![b'B'; 4100]);
            bytes.extend(b"FRAME-TAIL");
            s.frame(&transfer, 1, &bytes)?;
            s.park(&Parked {
                kind: Identifier::new("transfer")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                reason: Reason::new("incomplete transfer")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                targets: vec![transfer],
            })
        })
        .await?;
    let frame_form = format!(
        "csrf={}&target=transfer&source=frames&after=0&offset=0",
        encode(&csrf)
    );
    let frame_path = format!("/parked/{frame_park}/raw");
    let frame = request(&host, &frame_path, &post_auth, Some(&frame_form)).await?;
    assert!(frame.starts_with("HTTP/1.1 200"));
    assert!(frame.contains("&lt;frame&gt;"));
    assert!(!frame.contains("FRAME-TAIL"));
    let frame_tail = request(
        &host,
        &frame_path,
        &post_auth,
        Some(&frame_form.replace("offset=0", "offset=4096")),
    )
    .await?;
    assert!(frame_tail.starts_with("HTTP/1.1 200"));
    // Hex/text chunks may split the text marker; the bytes are still all shown.
    let hex = frame_tail
        .lines()
        .filter_map(|line| line.split_once(" | "))
        .map(|(hex, _)| hex.trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(hex.contains("46 52 41 4d 45 2d 54 41 49 4c"));
    // Fault injection: durable audit refusal must suppress all raw output.
    let database = directory.path().join("analyte.db");
    rusqlite::Connection::open(&database)?.execute_batch("CREATE TRIGGER test_refuse_raw_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit refusal'); END;")?;
    let refused = request(&host, &raw_path, &post_auth, Some(&form)).await?;
    assert!(refused.starts_with("HTTP/1.1 503"));
    assert!(!refused.contains("RAW"));
    assert!(!refused.contains("3c 73 63"));
    rusqlite::Connection::open(database)?.execute_batch("DROP TRIGGER test_refuse_raw_audit;")?;
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "pages retained store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}
