//! Explicit manual Gazelle run; CI must never opt into this external connection.
use super::*;
use crate::support::HANG_GUARD;
use std::sync::{Mutex, atomic::AtomicUsize};
use tracing_subscriber::{Layer, layer::SubscriberExt as _};

#[derive(Default)]
struct Failure {
    reason: Mutex<Option<String>>,
    wake: Notify,
    attempts: AtomicUsize,
}
struct Monitor(Arc<Failure>);
impl<S: tracing::Subscriber> Layer<S> for Monitor {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            message: String,
            reason: String,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "message" => self.message = format!("{value:?}"),
                    "error_kind" | "reason" => self.reason = format!("{value:?}"),
                    _ => {}
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.message.contains("LAW transport attempt failed") {
            self.0.attempts.fetch_add(1, Ordering::SeqCst);
        }
        if fields.message.contains("LAW transport attempt failed")
            || fields.message.contains("conversation capture failed")
            || fields.message.contains("machine session stopped")
        {
            if let Ok(mut reason) = self.0.reason.lock() {
                *reason = Some(format!("{}: {}", fields.message, fields.reason));
            }
            self.0.wake.notify_one();
        }
    }
}

/// Manual evidence only: test service, real LAW stack, real Gazelle peer, synthetic orders.
#[tokio::test]
#[ignore = "external network: requires ANALYTE_GAZELLE_ENDPOINT=151.80.220.161:10112 and explicit --ignored invocation"]
async fn gazelle_lab28_live() -> Result<(), Error> {
    let endpoint = std::env::var("ANALYTE_GAZELLE_ENDPOINT")?;
    if endpoint != "151.80.220.161:10112" {
        return Err("only the owner-authorized Gazelle endpoint is permitted".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let artifacts = root.join("docs/conformance/gazelle/law-run");
    let path = artifacts.join("analyte.toml");
    let mut program = Program::load(&path)?;
    let machine = program.machines().first().ok_or("machine")?;
    if machine.snapshot().effective().site().endpoint != endpoint {
        return Err("configured endpoint differs from explicit authorization".into());
    }
    let data = program.data_directory().to_owned();
    if data.join("analyte.db").exists() {
        return Err("preserve the previous run; refusing to reuse its store or orders".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&data)?;
    }
    #[cfg(windows)]
    std::fs::create_dir_all(&data)?;
    let reference = Reference::start(
        &data.join("receiver.sqlite"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(analyte_contract::decode(&std::fs::read(
            artifacts.join("seed-orders.json"),
        )?)?)
        .await?;
    program.set_loopback_destination_for_test("gazelle-local", reference.url.clone())?;
    let secrets = secrets(&program, "synthetic-api")?;
    let failure = Arc::new(Failure::default());
    let subscriber = tracing_subscriber::registry()
        .with(Monitor(failure.clone()))
        .with(tracing_subscriber::fmt::layer().with_ansi(false));
    tracing::subscriber::set_global_default(subscriber)?;
    let (stop, stopped) = oneshot::channel();
    let service = tokio::spawn(service::serve_loopback_test(
        program,
        Box::new(Space),
        async {
            stopped.await?;
            Ok(())
        },
        secrets,
    ));
    let outcome = tokio::select! {
        () = failure.wake.notified() => Err("Gazelle transport, capture or session failure; preserved original evidence".into()),
        result = tokio::time::timeout(HANG_GUARD, exchanges(&data, &artifacts, &reference)) => match result {
            Ok(result) => result,
            Err(error) => Err(Box::new(error) as Error),
        }
    };
    let _ = stop.send(());
    let shutdown = tokio::time::timeout(HANG_GUARD, service).await;
    let mut report = format!(
        "date={}\nendpoint={endpoint}\noutcome={outcome:?}\nfailed_connection_attempts={}\nshutdown={shutdown:?}\n",
        jiff::Timestamp::now(),
        failure.attempts.load(Ordering::SeqCst)
    );
    if let Ok(reason) = failure.reason.lock() {
        report.push_str(&format!("diagnostic={reason:?}\n"));
    }
    std::fs::write(data.join("run-outcome.txt"), report)?;
    reference.close().await?;
    shutdown???;
    outcome
}

async fn exchanges(data: &Path, artifacts: &Path, reference: &Reference) -> Result<(), Error> {
    let database = data.join("analyte.db");
    wait_orders(&database, false).await?;
    // Apply cancellation only after both original requests have reached a terminal ORL decision.
    reference
        .receiver
        .seed(analyte_contract::decode(&std::fs::read(
            artifacts.join("seed-cancel.json"),
        )?)?)
        .await?;
    wait_orders(&database, true).await?;
    loop {
        let db = rusqlite::Connection::open_with_flags(
            data.join("receiver.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let count: i64 = db.query_row(
            "SELECT count(*) FROM deliveries WHERE kind='order-events'",
            [],
            |row| row.get(0),
        )?;
        if count >= 3 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_orders(path: &Path, cancellation: bool) -> Result<(), Error> {
    loop {
        if path.exists() {
            let db = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            db.busy_timeout(Duration::from_secs(1))?;
            let ready: i64 = db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='machine_order_items'",
                [],
                |row| row.get(0),
            )?;
            if ready != 0 {
                let states = db
                    .prepare(
                        "SELECT state FROM machine_order_items ORDER BY machine_order, item_index",
                    )?
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                if states.len() == 2 {
                    let states: Vec<_> = states.iter().map(|s| s.trim_matches('"')).collect();
                    if cancellation {
                        let refused: i64 = db.query_row(
                            "SELECT count(*) FROM machine_orders WHERE source_code LIKE '%LAW-UC%'",
                            [],
                            |row| row.get(0),
                        )?;
                        if states.contains(&"CancelConfirmed") || refused != 0 {
                            return Ok(());
                        }
                    } else if states.iter().all(|s| *s == "Accepted") {
                        return Ok(());
                    }
                    if states
                        .iter()
                        .any(|s| matches!(*s, "Rejected" | "Uncertain"))
                    {
                        return Err(format!("retained per-item outcomes: {states:?}").into());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
