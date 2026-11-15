//! Plan sections 11/12 and ADR 0003: real HTTP action confirmation, CSRF and audit atomicity.
use super::pages::{csrf, encode, login};
use super::*;

#[tokio::test]
async fn plan11_admin_reprocess_resend_confirmation_csrf_actor_and_audit_failure_atomicity()
-> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    let definition = include_str!("../../../../../config/examples/hl7.toml");
    std::fs::write(
        root.join("machine.toml"),
        definition.replace("machine_code=\"SYN\"", "machine_code=\"OTHER\""),
    )?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../../../config/examples/site.toml"),
    )?;
    let config_path = root.join("program.toml");
    std::fs::write(
        &config_path,
        "schema_version=1\ndata_directory='data'\n[[machines]]\nid='synthetic'\ndefinition='machine.toml'\nsite='site.toml'\n[store]\nreserve_bytes=1\ndatabase_bytes=100000000\nrecord_bytes=2097152\npage_rows=100\nbusy_timeout_ms=100\n",
    )?;
    let program = Program::load(&config_path)?;
    let store = Arc::new(
        crate::service::open_store_with_deadline(&program, Box::new(Space), fixture_deadline()?)
            .await
            .map_err(|error| format!("admin action fixture migration: {error}"))?,
    );
    let machine = program.machines().first().ok_or("machine missing")?;
    let receipt = analyte_model::ReceiptId::new();
    let raw = analyte_framing::mllp::encode(b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01|message1|P|2.5.1\rPID|1\rOBR|1||00012\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r", analyte_framing::mllp::Limit::new(1048576)?)?;
    crate::evidence::receive(
        &store,
        &crate::evidence::Connection::new(machine)?,
        receipt,
        raw,
    )
    .await?;
    assert!(!crate::evidence::interpret(&store, machine, receipt).await?);
    let parked = store
        .request(fixture_deadline()?, |s| {
            Ok(s.parked(0)?
                .first()
                .ok_or(analyte_store::Error::Corruption)?
                .id)
        })
        .await?;
    std::fs::write(root.join("machine.toml"), definition)?;
    let bearer = super::super::super::rotate_token(program.data_directory())?;
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let (released, release) = tokio::sync::oneshot::channel();
    let state = Arc::new(Context {
        config_path: config_path.clone(),
        token: Token::load(program.data_directory())?,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines: Vec::new(),
        destinations: Vec::new(),
        directory: program.data_directory().to_owned(),
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
    let cookie = login(&host, &bearer).await?;
    let auth = format!("Cookie: {cookie}\r\n");
    let post_auth = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let path = format!("/parked/{parked}/reprocess");
    let confirm = request(&host, &path, &auth, None).await?;
    assert!(confirm.starts_with("HTTP/1.1 200"));
    assert!(confirm.contains("Confirm reprocess"));
    assert!(confirm.contains("method=\"post\""));
    let token = csrf(&confirm)?;
    let form = format!(
        "csrf={}&confirm=yes&reason=mapping%20repaired",
        encode(&token)
    );
    for body in [
        "confirm=yes&reason=missing".to_owned(),
        form.replace(&encode(&token), "wrong"),
        form.replace("confirm=yes", "confirm=no"),
    ] {
        assert!(
            request(&host, &path, &post_auth, Some(&body))
                .await?
                .starts_with("HTTP/1.1 403")
        );
    }
    let original_program = std::fs::read_to_string(&config_path)?;
    std::fs::create_dir(root.join("other"))?;
    std::fs::write(
        &config_path,
        original_program.replace("data_directory='data'", "data_directory='other'"),
    )?;
    let wrong_store = request(&host, &path, &post_auth, Some(&form)).await?;
    assert!(wrong_store.starts_with("HTTP/1.1 409"));
    assert!(wrong_store.contains("different store"));
    std::fs::write(&config_path, original_program)?;
    let database = program.data_directory().join("analyte.db");
    rusqlite::Connection::open(&database)?.execute_batch("CREATE TRIGGER refuse_operator_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(
        request(&host, &path, &post_auth, Some(&form))
            .await?
            .starts_with("HTTP/1.1 409")
    );
    store
        .request(fixture_deadline()?, move |s| {
            assert!(s.interpretation(receipt, 2)?.is_none());
            assert!(
                s.parked_item(parked)?
                    .ok_or(analyte_store::Error::Corruption)?
                    .resolution
                    .is_none()
            );
            assert!(s.audits(0)?.is_empty());
            assert!(s.deliveries(0)?.is_empty());
            Ok(())
        })
        .await?;
    rusqlite::Connection::open(&database)?.execute_batch("DROP TRIGGER refuse_operator_audit;")?;
    let response = request(&host, &path, &post_auth, Some(&form)).await?;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        request(&host, &path, &post_auth, Some(&form))
            .await?
            .starts_with("HTTP/1.1 409")
    );
    let delivery = store
        .request(fixture_deadline()?, move |s| {
            assert!(
                s.parked_item(parked)?
                    .ok_or(analyte_store::Error::Corruption)?
                    .resolution
                    .is_some()
            );
            assert_eq!(s.audits(0)?.len(), 1);
            let claim = s.claim(0)?.ok_or(analyte_store::Error::Corruption)?;
            s.attempted(claim.id, &analyte_store::Attempt::Done, 0)?;
            Ok(claim.id)
        })
        .await?;
    let path = format!("/deliveries/{}/resend", uuid::Uuid::from(delivery));
    let confirm = request(&host, &path, &auth, None).await?;
    assert!(confirm.starts_with("HTTP/1.1 200"));
    assert!(confirm.contains("Confirm resend"));
    assert!(
        request(&host, "/deliveries", &auth, None)
            .await?
            .contains("Resend this delivery")
    );
    assert!(
        request(&host, &path, &post_auth, Some("reason=test&confirm=yes"))
            .await?
            .starts_with("HTTP/1.1 403")
    );
    rusqlite::Connection::open(&database)?.execute_batch("CREATE TRIGGER refuse_operator_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(
        request(&host, &path, &post_auth, Some(&form))
            .await?
            .starts_with("HTTP/1.1 409")
    );
    store
        .request(fixture_deadline()?, move |s| {
            assert_eq!(
                s.delivery(delivery)?
                    .ok_or(analyte_store::Error::UnknownDelivery)?
                    .state,
                analyte_store::DeliveryState::Done
            );
            assert_eq!(s.audits(0)?.len(), 1);
            Ok(())
        })
        .await?;
    rusqlite::Connection::open(&database)?.execute_batch("DROP TRIGGER refuse_operator_audit;")?;
    assert!(
        request(&host, &path, &post_auth, Some(&form))
            .await?
            .starts_with("HTTP/1.1 200")
    );
    store
        .request(fixture_deadline()?, move |s| {
            assert_eq!(
                s.delivery(delivery)?
                    .ok_or(analyte_store::Error::UnknownDelivery)?
                    .state,
                analyte_store::DeliveryState::Pending
            );
            let audits = s.audits(0)?;
            assert_eq!(audits.len(), 2);
            assert!(
                audits
                    .iter()
                    .all(|(_, a)| a.actor.as_str().starts_with("admin-session-"))
            );
            assert_eq!(
                audits
                    .first()
                    .ok_or(analyte_store::Error::Corruption)?
                    .1
                    .actor,
                audits
                    .last()
                    .ok_or(analyte_store::Error::Corruption)?
                    .1
                    .actor
            );
            Ok(())
        })
        .await?;
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retains store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}
