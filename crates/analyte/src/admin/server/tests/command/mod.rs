//! The command line shows what the pages show: the running engine's Overview verdict, Activity, alerts, Data keeping and Cancelled by machine in the pages' words; doctor's checks are in `doctor`.
mod doctor;

use super::more::{MACHINES, Scene};
use super::*;
use crate::admin::command::Command;
use crate::admin::engine::{self, Engine, EngineError};
use crate::admin::overview::answer::Verdict;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A program file in `folder` with data in `data`, the admin page at `port`
/// when given, and `machines`.
pub(super) fn program_file(
    folder: &Path,
    data: &Path,
    port: Option<u16>,
    machines: &str,
) -> Result<PathBuf, Error> {
    let mut text = analyte_fixtures::program(machines).replace(
        "data_directory = 'data'",
        &format!("data_directory = '{}'", data.display()),
    );
    if let Some(port) = port {
        text.push_str(&format!("[admin]\nport = {port}\n"));
    }
    let file = folder.join("program.toml");
    std::fs::write(&file, text)?;
    Ok(file)
}

/// The port of `host`, as `127.0.0.1:port`.
fn port(host: &str) -> Result<u16, Error> {
    Ok(host.rsplit(':').next().ok_or("host has no port")?.parse()?)
}

/// A loopback port nothing listens on.
pub(super) fn closed_port() -> Result<u16, Error> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

fn models() -> BTreeMap<String, String> {
    MACHINES
        .iter()
        .map(|&(id, model)| (id.to_owned(), model.to_owned()))
        .collect()
}

// ADR 0023: the Overview, its public status and the command line take one
// verdict from one rule; the command adds where and how to fix each line.
#[tokio::test(flavor = "multi_thread")]
async fn overview_command_gives_the_pages_verdict_and_lines() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let folder = tempfile::tempdir()?;
    let file = program_file(
        folder.path(),
        scene.directory.path(),
        Some(port(&scene.started.3)?),
        "",
    )?;
    let program = Program::load(&file)?;
    let proven = Engine::proven(&program).await?;
    assert_eq!(proven.version().await?, env!("CARGO_PKG_VERSION"));
    let answer = proven.verdict().await?;
    let page = scene.get("/").await?;
    assert_ne!(answer.verdict, Verdict::Good);
    assert_ne!(answer.verdict, Verdict::Unknown);
    assert!(!answer.needs.is_empty());
    assert!(page.contains(&answer.title()), "{}", answer.title());
    for need in &answer.needs {
        assert!(page.contains(&need.text), "page lacks {}", need.text);
        assert!(page.contains(&format!("href=\"{}\"", need.href)));
    }
    let uncertain = answer
        .needs
        .iter()
        .find(|need| need.reason == "orders_uncertain")
        .ok_or("the seeded unknown outcome is a line")?;
    assert_eq!(uncertain.page, "Machine orders");
    assert_eq!(uncertain.command.as_deref(), Some("analyte orders list"));
    // After a key rotation the engine can no longer prove the new key, so the
    // new key is never sent to it.
    crate::admin::rotate_token(scene.directory.path())?;
    assert!(matches!(
        engine::verdict(&program).await,
        Err(EngineError::Unproven)
    ));
    scene.stop().await
}

// Safety rule 9 in spirit: not knowing is said as not knowing. An engine that
// does not answer is "unknown" (exit 3), never "All good".
#[tokio::test(flavor = "multi_thread")]
async fn overview_command_without_an_engine_is_unknown() -> Result<(), Error> {
    let folder = crate::admin::private_tempdir()?;
    crate::admin::rotate_token(folder.path())?;
    let file = program_file(folder.path(), folder.path(), Some(closed_port()?), "")?;
    let actor = Identifier::new("tester")?;
    let code = crate::admin::command::run(&file, actor, Command::Overview { json: true }).await?;
    assert_eq!(code, ExitCode::from(3));
    let program = Program::load(&file)?;
    assert!(matches!(
        engine::verdict(&program).await,
        Err(EngineError::NotRunning(_))
    ));
    // Without an admin page the program needs a machine to load.
    std::fs::write(
        folder.path().join("machine.toml"),
        include_str!("../../../../../../core/config/examples/hl7.toml"),
    )?;
    std::fs::write(
        folder.path().join("site.toml"),
        include_str!("../../../../../../core/config/examples/site.toml"),
    )?;
    let off = program_file(
        folder.path(),
        folder.path(),
        None,
        "[[machines]]
id = 'synthetic'
definition = 'machine.toml'
site = 'site.toml'
",
    )?;
    assert!(matches!(
        engine::verdict(&Program::load(&off)?).await,
        Err(EngineError::AdminOff)
    ));
    Ok(())
}

/// A listener on a free loopback port that records what it hears and gives
/// each connection `answer` (or nothing), as another program on the admin
/// port might.
async fn impostor(
    answer: Option<String>,
) -> Result<
    (
        u16,
        Arc<std::sync::Mutex<Vec<u8>>>,
        tokio::task::JoinHandle<()>,
    ),
    Error,
> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = heard.clone();
    let owner = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = [0_u8; 4096];
            if let Ok(Ok(read)) =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut bytes)).await
                && let (Some(heard), Ok(mut all)) = (bytes.get(..read), kept.lock())
            {
                all.extend_from_slice(heard);
            }
            if let Some(answer) = &answer {
                let _ = stream.write_all(answer.as_bytes()).await;
            }
        }
    });
    Ok((port, heard, owner))
}

// ADR 0025 and ADR 0027 (threat: another program on the admin port while the
// service is stopped): `overview` and `doctor` send the admin key only to a
// listener that proves it holds it. A wrong proof, an error or silence never
// receives the key or an Authorization header; overview is unknown (exit 3)
// and doctor fails the engine check.
#[tokio::test(flavor = "multi_thread")]
async fn overview_and_doctor_never_send_the_key_to_an_unproven_listener() -> Result<(), Error> {
    let wrong = format!("{{\"proof\":\"{}\"}}", "00".repeat(32));
    let answers = [
        Some(format!(
            "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{wrong}",
            wrong.len()
        )),
        Some(
            "HTTP/1.1 404 Not Found
Content-Length: 0
Connection: close

"
            .to_owned(),
        ),
        None,
    ];
    for answer in answers {
        let (port, heard, owner) = impostor(answer).await?;
        let folder = crate::admin::private_tempdir()?;
        let key = crate::admin::rotate_token(folder.path())?;
        let file = program_file(folder.path(), folder.path(), Some(port), "")?;
        let actor = Identifier::new("tester")?;
        let code =
            crate::admin::command::run(&file, actor, Command::Overview { json: true }).await?;
        assert_eq!(code, ExitCode::from(3));
        let checks = crate::admin::command::doctor::checks(&file).await;
        let engine = checks
            .iter()
            .find(|check| check.what == "Engine")
            .ok_or("an engine check")?;
        assert_eq!(engine.level, crate::admin::command::doctor::Level::Fail);
        assert!(
            engine
                .found
                .contains("could not prove it is this Analyte engine")
                || engine.found.contains("did not answer"),
            "{}",
            engine.found
        );
        owner.abort();
        let heard = String::from_utf8_lossy(&heard.lock().map_err(|_| "poisoned")?).into_owned();
        assert!(heard.contains("GET /proof?nonce="), "{heard}");
        assert!(
            !heard.to_ascii_lowercase().contains("authorization"),
            "{heard}"
        );
        assert!(!heard.contains(&key), "the admin key was sent");
        assert!(!heard.contains("/overview/verdict"), "{heard}");
    }
    Ok(())
}

// Owner request 2026-11-21 (command-line parity): Activity, Data keeping and
// Cancelled by machine print what their pages show, from the same reads.
#[tokio::test(flavor = "multi_thread")]
async fn page_commands_print_what_the_pages_show() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    // Activity: the page's lines, newest first, in bounded pages.
    let (all, _) = crate::admin::activity::printed(&scene.store, &models(), None, None, 50).await?;
    let feed = scene.get("/activity").await?;
    assert!(!all.is_empty());
    for line in &all {
        assert!(feed.contains(line.kind), "feed lacks {}", line.kind);
    }
    let (first, next) =
        crate::admin::activity::printed(&scene.store, &models(), None, None, 2).await?;
    assert_eq!(first.len(), 2);
    let next = crate::admin::activity::parse_position(&next.ok_or("a second page")?)?;
    let (second, _) =
        crate::admin::activity::printed(&scene.store, &models(), None, Some(next), 50).await?;
    assert_eq!(
        all.iter()
            .skip(2)
            .map(|line| &line.text)
            .collect::<Vec<_>>(),
        second.iter().map(|line| &line.text).collect::<Vec<_>>()
    );
    // Data keeping: each machine's policy in the page's words, and the hold.
    let retention = vec![
        (Identifier::new("chemistry-analyzer")?, Some(365)),
        (Identifier::new("hematology-analyzer")?, None),
    ];
    let keeping = crate::admin::more::retention::printed(&scene.store, &retention, 0).await?;
    let keeping_page = scene.get("/retention").await?;
    for (machine, kept) in &keeping.kept {
        assert!(keeping_page.contains(kept), "{machine}: {kept}");
    }
    assert!(!keeping.holds.is_empty());
    for hold in &keeping.holds {
        assert!(keeping_page.contains(&hold.target), "{}", hold.target);
    }
    assert!(
        keeping
            .holds
            .iter()
            .any(|hold| hold.keeps.starts_with("Message from "))
    );
    // Cancelled by machine: the card's sample, machine, tests and lines.
    let (cancelled, more) = crate::admin::more::awos::printed(&scene.store, &models(), 0).await?;
    assert_eq!(more, None);
    let [item] = cancelled.as_slice() else {
        return Err("one cancelled item".into());
    };
    assert_eq!(item.sample.as_deref(), Some("261119-018"));
    assert_eq!(item.model.as_deref(), Some("Coagulation analyzer CG-7"));
    assert_eq!(item.tests.as_deref(), Some("1 test"));
    assert!(
        item.lines
            .iter()
            .any(|line| line.starts_with("Cancelled by the machine · recorded "))
    );
    assert!(
        item.lines
            .iter()
            .any(|line| line == "The lab app's copy says: requested")
    );
    // Run as commands, both reads that show sample IDs are audited as the actor.
    let folder = tempfile::tempdir()?;
    let file = program_file(
        folder.path(),
        scene.directory.path(),
        Some(port(&scene.started.3)?),
        "",
    )?;
    let actor = Identifier::new("tester")?;
    let page = Command::Activity {
        limit: 5,
        machine: None,
        before: None,
        json: true,
    };
    let code = crate::admin::command::run(&file, actor.clone(), page).await?;
    assert_eq!(code, ExitCode::SUCCESS);
    crate::admin::command::cancelled(&Program::load(&file)?, actor, 0, true).await?;
    let db = rusqlite::Connection::open(scene.database())?;
    for action in ["activity-list", "orders-cancelled-list"] {
        let count: i64 = db.query_row(
            "SELECT count(*) FROM audit WHERE evidence LIKE ?1 AND evidence LIKE '%tester%'",
            [format!("%{action}%")],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1, "{action}");
    }
    drop(db);
    scene.stop().await
}

// ADR 0014 and the Overview's alert rows: each active alert lists its reason
// in the verdict's words and where it is fixed.
#[tokio::test(flavor = "multi_thread")]
async fn alerts_list_gives_each_reason_and_fix() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    scene
        .store
        .request::<_, analyte_store::OperatorError>(fixture_deadline()?, |s| {
            s.evaluate_alert(
                &analyte_store::AlertKey::ParkedItems,
                "duty",
                true,
                Some(3),
                100,
            )?;
            s.evaluate_alert(
                &analyte_store::AlertKey::RestorePaused,
                "duty",
                true,
                Some(1),
                100,
            )
        })
        .await?;
    let now = jiff::Timestamp::now();
    let alerts: Vec<_> = crate::operator::alerts::active(&scene.store)
        .await?
        .iter()
        .map(|alert| crate::admin::overview::PrintedAlert::of(alert, now))
        .collect();
    let review = alerts
        .iter()
        .find(|alert| alert.signal == "parked_items")
        .ok_or("review alert")?;
    assert_eq!(review.reason, "3 items need review");
    assert_eq!(review.page, "Needs review");
    assert_eq!(review.command, Some("analyte review list"));
    let restore = alerts
        .iter()
        .find(|alert| alert.signal == "restore_paused")
        .ok_or("restore alert")?;
    assert_eq!(restore.page, "Orders");
    assert!(
        restore
            .command
            .is_some_and(|command| command.contains("orders resume-after-restore"))
    );
    scene.stop().await
}
