//! The command line shows what the pages show: the running engine's Overview verdict, Activity, alerts, Data keeping and Cancelled by machine in the pages' words; doctor's checks are in `doctor`.
mod doctor;

use super::more::{MACHINES, Scene};
use super::*;
use crate::admin::command::{Command, engine::EngineError};
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
    assert_eq!(
        crate::admin::command::engine::answering(&program).await?,
        env!("CARGO_PKG_VERSION")
    );
    let answer = crate::admin::command::engine::verdict(&program).await?;
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
    // A key rotated after the engine started is refused, never worked around.
    crate::admin::rotate_token(scene.directory.path())?;
    assert!(matches!(
        crate::admin::command::engine::verdict(&program).await,
        Err(EngineError::KeyRefused(_))
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
        crate::admin::command::engine::verdict(&program).await,
        Err(EngineError::Silent { .. })
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
        crate::admin::command::engine::verdict(&Program::load(&off)?).await,
        Err(EngineError::AdminOff)
    ));
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
