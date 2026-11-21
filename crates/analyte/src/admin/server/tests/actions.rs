//! Tests operator action confirmation, CSRF checks and audit writes.
use super::pages::{csrf, encode, login};
use super::*;

/// The Process again dialog's title.
const PROCESS_AGAIN_TITLE: &str =
    "<h2 id=\"dlg-title\" class=\"dialog__title\">Process this message again?</h2>";

// Plan §11
#[tokio::test]
async fn reprocess_and_resend_need_confirmation_csrf_and_audit() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    let (program, store, receipt, parked, config_path) = seed_parked_receipt(root).await?;
    let bearer = super::super::super::rotate_token(program.data_directory())?;
    let (state, stop, owner, host) = start_action_server(&config_path, &program, &store).await?;
    let cookie = login(&host, &bearer).await?;
    let auth = format!("Cookie: {cookie}\r\n");
    let post_auth = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let path = list_opens_reprocess(&host, &auth, parked).await?;
    let confirm = request(&host, &path, &auth, None).await?;
    assert!(confirm.starts_with("HTTP/1.1 200"));
    assert!(confirm.contains(PROCESS_AGAIN_TITLE));
    assert!(confirm.contains("method=\"post\""));
    let token = csrf(&confirm)?;
    let form = format!(
        "csrf={}&confirm=yes&reason=mapping%20repaired",
        encode(&token)
    );
    for body in [
        "confirm=yes&reason=missing".to_owned(),
        form.rewrite(&encode(&token), "wrong"),
        form.rewrite("confirm=yes", "confirm=no"),
    ] {
        assert!(
            request(&host, &path, &post_auth, Some(&body))
                .await?
                .starts_with("HTTP/1.1 403")
        );
    }
    let oversized = format!("{form}&padding={}", "x".repeat(1024));
    assert!(
        request(&host, &path, &post_auth, Some(&oversized))
            .await?
            .starts_with("HTTP/1.1 413")
    );
    let original_program = std::fs::read_to_string(&config_path)?;
    std::fs::create_dir(root.join("other"))?;
    std::fs::write(
        &config_path,
        original_program.rewrite("data_directory = 'data'", "data_directory = 'other'"),
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
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            assert!(s.interpretation(receipt, 2)?.is_none());
            assert!(
                s.parked_item(parked)?
                    .ok_or(FixtureError::Corruption)?
                    .resolution
                    .is_none()
            );
            assert!(s.audits(0)?.is_empty());
            assert!(s.deliveries(0)?.is_empty());
            Ok::<_, FixtureError>(())
        })
        .await?;
    rusqlite::Connection::open(&database)?.execute_batch("DROP TRIGGER refuse_operator_audit;")?;
    let response = request(&host, &path, &post_auth, Some(&form)).await?;
    assert_processed_again(&response);
    assert!(
        request(&host, &path, &post_auth, Some(&form))
            .await?
            .starts_with("HTTP/1.1 409")
    );
    let destination =
        crate::orders::destination(program.machines().first().ok_or("missing machine")?);
    let delivery = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            assert!(
                s.parked_item(parked)?
                    .ok_or(FixtureError::Corruption)?
                    .resolution
                    .is_some()
            );
            assert_eq!(s.audits(0)?.len(), 1);
            let claim = s
                .claim_destination(0, &destination)?
                .ok_or(FixtureError::Corruption)?;
            s.attempted(claim.id, &analyte_store::Attempt::Done, 0)?;
            Ok(claim.id)
        })
        .await?;
    assert_resend_audit_atomicity(&store, &host, &auth, &post_auth, &form, &database, delivery)
        .await?;
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retains store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}

/// Owner request 2026-11-19 (Needs review item 3): the list's button opens the
/// same confirmation page, so the reason, CSRF and audit checks on it cover the
/// list. Returns that page's path.
async fn list_opens_reprocess(host: &str, auth: &str, parked: i64) -> Result<String, Error> {
    let path = format!("/parked/{parked}/reprocess");
    let list = request(host, "/parked", auth, None).await?;
    assert!(
        list.contains(&format!("href=\"{path}\">Process again</a>")),
        "{list}"
    );
    assert!(list.contains("<strong>What to do:</strong>"), "{list}");
    Ok(path)
}

async fn seed_parked_receipt(
    root: &std::path::Path,
) -> Result<
    (
        Program,
        Arc<analyte_store::Handle>,
        analyte_model::ReceiptId,
        i64,
        std::path::PathBuf,
    ),
    Error,
> {
    let definition = include_str!("../../../../../core/config/examples/hl7.toml");
    std::fs::write(
        root.join("machine.toml"),
        definition.rewrite("machine_code=\"SYN\"", "machine_code=\"OTHER\""),
    )?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../../../core/config/examples/site.toml"),
    )?;
    let config_path = root.join("program.toml");
    std::fs::write(
        &config_path,
        analyte_fixtures::program_with_limits(
            "[[machines]]\nid='synthetic'\ndefinition='machine.toml'\nsite='site.toml'\n",
            &analyte_store::Limits {
                reserve_bytes: 1,
                database_bytes: 100000000,
                record_bytes: 2097152,
                page_rows: 100,
                busy_timeout: Duration::from_millis(100),
            },
        ),
    )?;
    let program = Program::load(&config_path)?;
    let store = Arc::new(
        crate::runtime::service::open_store_with_deadline(
            &program,
            Box::new(Space),
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("admin action fixture migration: {error}"))?,
    );
    let machine = program.machines().first().ok_or("machine missing")?;
    let receipt = analyte_model::ReceiptId::new();
    let raw = analyte_framing::hl7::encode(b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r", analyte_framing::hl7::Limit::new(1048576)?)?;
    crate::intake::receive(
        &store,
        &crate::intake::ConnectionEvidence::new(machine)?,
        receipt,
        raw,
    )
    .await?;
    assert!(!crate::intake::interpret(&store, machine, receipt).await?);
    let parked = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            Ok::<_, FixtureError>(s.parked(0)?.first().ok_or(FixtureError::Corruption)?.id)
        })
        .await?;
    std::fs::write(root.join("machine.toml"), definition)?;
    Ok((program, store, receipt, parked, config_path))
}

async fn assert_resend_audit_atomicity(
    store: &analyte_store::Handle,
    host: &str,
    auth: &str,
    post_auth: &str,
    form: &str,
    database: &std::path::Path,
    delivery: analyte_model::DeliveryId,
) -> Result<(), Error> {
    let path = format!("/deliveries/{}/resend", uuid::Uuid::from(delivery));
    let confirm = request(host, &path, auth, None).await?;
    assert!(confirm.starts_with("HTTP/1.1 200"));
    assert!(confirm.contains("Send this delivery again?"));
    assert!(
        request(host, "/deliveries", auth, None)
            .await?
            .contains(">Send again</a>")
    );
    assert!(
        request(host, &path, post_auth, Some("reason=test&confirm=yes"))
            .await?
            .starts_with("HTTP/1.1 403")
    );
    rusqlite::Connection::open(database)?.execute_batch("CREATE TRIGGER refuse_operator_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    assert!(
        request(host, &path, post_auth, Some(form))
            .await?
            .starts_with("HTTP/1.1 409")
    );
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            assert_eq!(
                s.delivery(delivery)?
                    .ok_or(FixtureError::UnknownDelivery)?
                    .state,
                analyte_store::DeliveryState::Done
            );
            assert_eq!(s.audits(0)?.len(), 1);
            Ok::<_, FixtureError>(())
        })
        .await?;
    rusqlite::Connection::open(database)?.execute_batch("DROP TRIGGER refuse_operator_audit;")?;
    assert!(
        request(host, &path, post_auth, Some(form))
            .await?
            .starts_with("HTTP/1.1 200")
    );
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            assert_eq!(
                s.delivery(delivery)?
                    .ok_or(FixtureError::UnknownDelivery)?
                    .state,
                analyte_store::DeliveryState::Pending
            );
            let audits = s.audits(0)?;
            assert_eq!(audits.len(), 2);
            assert_eq!(
                audits
                    .first()
                    .ok_or(FixtureError::UnknownAudit)?
                    .1
                    .action
                    .as_str(),
                "receipt-reprocess"
            );
            assert_eq!(
                audits
                    .get(1)
                    .ok_or(FixtureError::UnknownAudit)?
                    .1
                    .action
                    .as_str(),
                "delivery-resend"
            );
            assert!(
                audits
                    .iter()
                    .all(|(_, audit)| audit.reason.as_str() == "mapping repaired")
            );
            assert!(
                audits
                    .iter()
                    .all(|(_, a)| a.actor.as_str().starts_with("admin-session-"))
            );
            assert_eq!(
                audits.first().ok_or(FixtureError::Corruption)?.1.actor,
                audits.last().ok_or(FixtureError::Corruption)?.1.actor
            );
            Ok(())
        })
        .await?;
    Ok(())
}

async fn start_action_server(
    config_path: &std::path::Path,
    program: &Program,
    store: &Arc<analyte_store::Handle>,
) -> Result<
    (
        Arc<Context>,
        watch::Sender<bool>,
        tokio::task::JoinHandle<Result<(), crate::admin::Error>>,
        String,
    ),
    Error,
> {
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let (released, release) = tokio::sync::oneshot::channel();
    let (closing, closed) = watch::channel(false);
    let state = Arc::new(Context {
        alert_endpoints: vec![],
        retention: crate::runtime::retention::policies(program),
        token: Token::load(program.data_directory())?,
        sessions: Sessions::default(),
        windows: Arc::new(windows::SignIn::new(None)),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines: Vec::new(),
        destinations: Vec::new(),
        retired: crate::admin::Draining::new(),
        directory: program.data_directory().to_owned(),
        reserve: 1,
        setup: Arc::new(crate::admin::setup::Setup::for_test(
            config_path,
            program.data_directory(),
        )),
        assets: crate::admin::view::assets::Root::of_this_build(),
        installed: crate::admin::more::Installed::for_test(Vec::new()),
        write_budget: WRITE_BUDGET,
        live: crate::admin::server::live::Bounds::standard(),
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

// ADR 0013 items 4 and 7
#[tokio::test]
async fn retention_page_and_receipt_hold_are_audited() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (program, store, receipt, _, config_path) = seed_parked_receipt(directory.path()).await?;
    let bearer = super::super::super::rotate_token(program.data_directory())?;
    let (state, stop, owner, host) = start_action_server(&config_path, &program, &store).await?;
    let cookie = login(&host, &bearer).await?;
    let auth = format!("Cookie: {cookie}\r\n");
    let page = request(&host, "/retention", &auth, None).await?;
    for text in [
        "<h1>Data keeping</h1>",
        "Keeping times",
        "Kept, never deleted",
        "Last clean-up",
        "Oldest message kept",
        "it never expires",
    ] {
        assert!(page.contains(text), "{text}");
    }
    let token = csrf(&page)?;
    let post_auth = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let form = format!(
        "csrf={}&confirm=yes&scope=receipt&id={}&destination=&operation=place&reason=review%20required",
        encode(&token),
        uuid::Uuid::from(receipt)
    );
    let refused = request(
        &host,
        "/retention",
        &post_auth,
        Some(&form.rewrite("confirm=yes", "confirm=no")),
    )
    .await?;
    assert!(refused.starts_with("HTTP/1.1 403"));
    let database = program.data_directory().join("analyte.db");
    rusqlite::Connection::open(&database)?.execute_batch("CREATE TRIGGER refuse_hold_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    let refused = request(&host, "/retention", &post_auth, Some(&form)).await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |store| {
            assert!(!store.retention_hold(&analyte_store::HoldTarget::Receipt(receipt))?);
            assert!(store.audits(0)?.is_empty());
            Ok::<_, FixtureError>(())
        })
        .await?;
    rusqlite::Connection::open(&database)?.execute_batch("DROP TRIGGER refuse_hold_audit;")?;
    let placed = request(&host, "/retention", &post_auth, Some(&form)).await?;
    assert!(placed.starts_with("HTTP/1.1 200"), "{placed}");
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |store| {
            assert!(store.retention_hold(&analyte_store::HoldTarget::Receipt(receipt))?);
            let audits = store.audits(0)?;
            let audit = &audits.last().ok_or(FixtureError::Corruption)?.1;
            assert_eq!(audit.action.as_str(), "retention-hold-place");
            assert_eq!(audit.reason.as_str(), "review required");
            Ok(())
        })
        .await?;
    let release = form.rewrite("operation=place", "operation=release");
    assert!(
        request(&host, "/retention", &post_auth, Some(&release))
            .await?
            .starts_with("HTTP/1.1 200")
    );
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |store| {
            assert!(!store.retention_hold(&analyte_store::HoldTarget::Receipt(receipt))?);
            let audits = store.audits(0)?;
            assert_eq!(audits.len(), 2);
            assert_eq!(
                audits
                    .get(1)
                    .ok_or(FixtureError::UnknownAudit)?
                    .1
                    .action
                    .as_str(),
                "retention-hold-release"
            );
            assert_eq!(audits[1].1.reason.as_str(), "review required");
            assert_eq!(audits[0].1.actor, audits[1].1.actor);
            assert!(audits[0].1.actor.as_str().starts_with("admin-session-"));
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

/// Owner decision 2026-11-19 (admin restyle b1): the result says which reading
/// was recorded and what it decided.
fn assert_processed_again(response: &str) {
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("<h1>Processed again</h1>"), "{response}");
    assert!(response.contains("<p class=\"alert__title\">Reading 2 was recorded</p>"));
    assert!(response.contains("What happened: "));
}
