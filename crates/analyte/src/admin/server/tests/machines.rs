//! Tests the Machines list, a machine's page and how a message was read, through the real handlers.
use super::overview::{empty_store, stop};
use super::pages::login;
use super::*;
use crate::machine::link::status::{Link, Reporter};
use analyte_model::{
    Digest, EvidenceList, IdentityStatus, NamespacedId, NonEmptyEvidenceList, ObservationIdentity,
    ProcessingMode, QcClass, ReceiptId, ReceiptMetadata, ResultContent, ResultIdentity, ResultPart,
    ResultStatus, ResultValue, Revision, SampleId, SampleIdentity, SampleSource, SourceText,
    ValueInterpretation,
};
use analyte_store::{Interpretation, Parked, Reading};

/// A fixed "now" for stored times, in the past so pages read it as a date.
pub(super) const AT: i64 = 1_795_046_077;

/// A test code and a value that must never show on a page.
pub(super) const CODE: &str = "XLT9";
pub(super) const VALUE: &str = "positive-value";

pub(super) fn id(text: &str) -> Result<Identifier, FixtureError> {
    Identifier::new(text).map_err(FixtureError::Text)
}

/// A machine Analyte runs, attached by `connection`; its link is listening or
/// watching when `ready`, open when `open`, and its service runs when `service`.
fn running(
    name: &str,
    (connection, link): (Connection, Link),
    (ready, open): (bool, bool),
    service: bool,
) -> Result<Machine, Error> {
    let reporter = Reporter::new(link);
    if ready {
        reporter.ready(true);
    }
    if open {
        (reporter.observer())(analyte_transport::Activity::Opened);
    }
    Ok(Machine {
        rules: Vec::new(),
        digest: [0xab; 32],
        id: Identifier::new(name)?,
        running: Arc::new(AtomicBool::new(service)),
        status: reporter.subscribe(),
        model: Some(format!("Model of {name}")),
        connection,
        lab_app: Identifier::new("main-lab")?,
    })
}

/// One machine per state the list shows: a network machine connected, a
/// serial port that does not open, a USB adapter's open port, a network
/// machine Analyte waits for, a watched folder, a machine Analyte cannot
/// reach, one Analyte stopped working with, and one removed from the settings.
fn scene() -> Result<Vec<Machine>, Error> {
    let mut connected = running(
        "net-1",
        (Connection::Listening(Some(5600)), Link::TcpServer),
        (true, true),
        true,
    )?;
    // Built from a real declaration, so the page shows what the formatting module says for it.
    let declaration = concat!(
        "rule = 'hl7_absent_obx11_means_final'\n",
        "declaration = { scope = { model = 'synthetic', firmware = 'test-1', mode = 'test', ",
        "edition = { protocol = 'astm' }, direction = 'machine_to_host', ",
        "message = 'astm_results' }, level = 'documented', evidence = [{ kind = 'manual', ",
        "reference = 'synthetic-test-policy', title = 'Synthetic policy', revision = '1', ",
        "section = 'ADR 0015 test', guarantee = 'compatibility', ",
        "covers_unknown_firmware = false, contradicts = false }] }\n",
    );
    let departure: analyte_config::MachineDeparture = toml::from_str(declaration)?;
    connected.rules = vec![crate::admin::machines::Rule {
        words: crate::admin::view::format::departure(&departure),
        meaning: departure.meaning(),
        declaration: declaration.into(),
    }];
    Ok(vec![
        connected,
        running(
            "serial-2",
            (Connection::Serial("COM3".into()), Link::Serial),
            (false, false),
            true,
        )?,
        running(
            "usb-3",
            (Connection::UsbAdapter("A10K3XYZ".into()), Link::Serial),
            (false, true),
            true,
        )?,
        running(
            "wait-4",
            (Connection::Listening(Some(5601)), Link::TcpServer),
            (true, false),
            true,
        )?,
        running(
            "folder-5",
            (Connection::Folder("results".into()), Link::Folder),
            (true, false),
            true,
        )?,
        running(
            "client-6",
            (Connection::Dialing, Link::TcpClient),
            (false, false),
            true,
        )?,
        running(
            "stopped-7",
            (Connection::Listening(Some(5602)), Link::TcpServer),
            (true, false),
            false,
        )?,
        running(
            "removed-8",
            (Connection::Serial("COM8".into()), Link::Serial),
            (false, true),
            true,
        )?,
    ])
}

/// The settings file: every machine of [`scene`] but `removed-8`, then one not
/// set up and one set up since the last start.
const PROGRAM: &str = r"
[[machines]]
id = 'net-1'
definition = 'machines/a/machine.toml'
site = 'net-1.site.toml'
[[machines]]
id = 'serial-2'
definition = 'machines/a/machine.toml'
site = 'serial-2.site.toml'
[[machines]]
id = 'usb-3'
definition = 'machines/a/machine.toml'
site = 'usb-3.site.toml'
[[machines]]
id = 'wait-4'
definition = 'machines/a/machine.toml'
site = 'wait-4.site.toml'
[[machines]]
id = 'folder-5'
definition = 'machines/a/machine.toml'
site = 'folder-5.site.toml'
[[machines]]
id = 'client-6'
definition = 'machines/a/machine.toml'
site = 'client-6.site.toml'
[[machines]]
id = 'stopped-7'
definition = 'machines/a/machine.toml'
site = 'stopped-7.site.toml'
[[machines]]
id = 'new-9'
site = 'new-9.site.toml'
[[machines]]
id = 'saved-10'
definition = 'machines/a/machine.toml'
site = 'saved-10.site.toml'
";

/// A server over `machines` with [`PROGRAM`] as its settings, and a signed-in session's cookie header.
async fn started(
    root: &std::path::Path,
    machines: Vec<Machine>,
) -> Result<(Arc<analyte_store::Handle>, Started, String), Error> {
    std::fs::write(root.join("program.toml"), PROGRAM)?;
    let bearer = crate::admin::rotate_token(root)?;
    let store = empty_store(root).await?;
    let started = start_overview_server(root, &store, machines, Vec::new()).await?;
    let cookie = login(&started.3, &bearer).await?;
    Ok((store, started, format!("Cookie: {cookie}\r\n")))
}

/// Every Machines page is in the new look, Machines marked, with the strict headers.
fn assert_new_look(page: &str) {
    headers_present(page);
    assert!(
        page.contains("<nav class=\"sidebar\" aria-label=\"Main\">"),
        "{page}"
    );
    assert!(page.contains("app.css"));
    assert!(!page.contains("admin.css"));
    assert!(page.contains("<a href=\"/machines\" aria-current=\"page\">"));
    assert!(!page.contains("style="));
    assert!(!other_script(page));
}

/// The row of `name` on the list.
fn row<'a>(page: &'a str, name: &str) -> &'a str {
    let start = format!("<a href=\"/machines/{name}\">{name}</a>");
    page.split("<tr>")
        .find(|row| row.contains(&start))
        .and_then(|row| row.split("</tr>").next())
        .unwrap_or_default()
}

/// One stored message from `machine` received at `at`.
pub(super) fn receive(
    store: &mut analyte_store::Store,
    machine: &str,
    at: i64,
) -> Result<ReceiptId, FixtureError> {
    let raw = format!("synthetic message {CODE} {VALUE}");
    let config = b"synthetic config";
    let receipt = ReceiptId::try_from(uuid::Uuid::now_v7()).map_err(FixtureError::Identity)?;
    store.receive(
        &ReceiptMetadata {
            id: receipt,
            raw_digest: Digest(analyte_contract::content_digest(raw.as_bytes())),
            connection: id("synthetic")?,
            machine: id(machine)?,
            driver: NamespacedId {
                namespace: id("synthetic")?,
                value: id("1")?,
            },
            config_version: id("1")?,
            config_digest: Digest(analyte_contract::content_digest(config)),
            received_at: jiff::Timestamp::from_second(at)
                .map_err(FixtureError::EvidenceTimestamp)?,
        },
        raw.as_bytes(),
        config,
    )?;
    Ok(receipt)
}

/// One result for `receipt` from sample `sample`, with a test code and value no page may show.
pub(super) fn result(receipt: ReceiptId, sample: Option<&str>) -> Result<ResultContent, Error> {
    let test = NamespacedId {
        namespace: Identifier::new("local")?,
        value: Identifier::new(CODE)?,
    };
    Ok(ResultContent {
        receipt,
        sample: sample
            .map(|sample| {
                Ok::<_, Error>(SampleIdentity {
                    id: SampleId::new(sample)?,
                    source: SampleSource::ScannedByMachine,
                    position: None,
                })
            })
            .transpose()?,
        processing: ProcessingMode::Production,
        identity: IdentityStatus::Resolved(ResultIdentity {
            observation: ObservationIdentity {
                code: test.clone(),
                sub_id: SourceText::new("1")?,
                run: SourceText::new("")?,
                order: test.clone(),
            },
            revision: Revision(0),
        }),
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![ResultPart {
            test,
            values: EvidenceList::new(vec![ResultValue::new(
                SourceText::new(VALUE)?,
                ValueInterpretation::Text,
            )])?,
            units: None,
            dilution: None,
            flags: EvidenceList::new(vec![])?,
            reference_range: None,
            comments: EvidenceList::new(vec![])?,
            qc_class: QcClass::Patient,
            source_status: SourceText::new("F")?,
            status: ResultStatus::Final,
            source_path: EvidenceList::new(vec![])?,
            times: EvidenceList::new(vec![])?,
            received_at: jiff::Timestamp::from_second(AT)?,
        }])?)?,
    })
}

/// A reading's stored evidence with `outcome` and `departures`.
pub(super) fn evidence(
    outcome: Reading,
    departures: Option<analyte_model::DepartureEvidence>,
) -> Result<String, Error> {
    Ok(serde_json::to_string(&Interpretation {
        driver: NamespacedId {
            namespace: Identifier::new("synthetic")?,
            value: Identifier::new("1")?,
        },
        config_version: Identifier::new("site-7")?,
        config: b"synthetic = true".to_vec(),
        outcome,
        departures,
    })?)
}

/// The store's key for `id`: its UUID as a JSON string.
pub(super) fn key(id: ReceiptId) -> Result<String, Error> {
    Ok(serde_json::to_string(&uuid::Uuid::from(id).to_string())?)
}

/// Stores reading `version` of `receipt` with `evidence`, as the runtime writes it.
fn read_as(
    database: &std::path::Path,
    receipt: ReceiptId,
    version: i64,
    evidence: &str,
) -> Result<(), Error> {
    rusqlite::Connection::open(database)?.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,?2,?3)",
        rusqlite::params![key(receipt)?, version, evidence],
    )?;
    Ok(())
}

/// Queues one delivery of reading `version` of `receipt` in `state`.
fn deliver(
    database: &std::path::Path,
    receipt: ReceiptId,
    version: i64,
    state: &str,
) -> Result<(), Error> {
    let delivery = serde_json::to_string(&uuid::Uuid::now_v7().to_string())?;
    rusqlite::Connection::open(database)?.execute(
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'result',?2,?3,'main-lab','k',x'00',x'00',1,?4,0)",
        rusqlite::params![delivery, key(receipt)?, version, state],
    )?;
    Ok(())
}

/// The messages [`seed`] stored for `net-1`, oldest first, and the open item naming one.
struct Seeded {
    unread: ReceiptId,
    delivered: ReceiptId,
    failed: ReceiptId,
    waiting: ReceiptId,
    item: i64,
}

/// Stores four messages from `net-1`: one never read, one read into two
/// results of one sample and delivered, one that could not be read and waits
/// in Needs review, and one read twice whose delivery waits; and one from
/// another machine.
async fn seed(root: &std::path::Path, store: &analyte_store::Handle) -> Result<Seeded, Error> {
    let (unread, delivered, failed, waiting, item) = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let unread = receive(s, "net-1", AT - 3600)?;
            let delivered = receive(s, "net-1", AT - 2400)?;
            let failed = receive(s, "net-1", AT - 1200)?;
            let waiting = receive(s, "net-1", AT - 600)?;
            receive(s, "serial-2", AT)?;
            let item = s.park(&Parked {
                kind: id("parked_reading")?,
                reason: Reason::new("checksum mismatch in frame 2").map_err(FixtureError::Text)?,
                targets: vec![id(&uuid::Uuid::from(failed).to_string())?],
            })?;
            Ok((unread, delivered, failed, waiting, item))
        })
        .await?;
    let database = root.join("analyte.db");
    read_as(
        &database,
        delivered,
        1,
        &evidence(
            Reading::Results(vec![
                result(delivered, Some("000123"))?,
                result(delivered, Some("000123"))?,
            ]),
            Some(analyte_model::DepartureEvidence::default()),
        )?,
    )?;
    deliver(&database, delivered, 1, "done")?;
    deliver(&database, delivered, 1, "done")?;
    read_as(
        &database,
        failed,
        1,
        &evidence(
            Reading::Failed(Reason::new(
                "Position { record: 4, field: 3 }: UnknownTest",
            )?),
            Some(analyte_model::DepartureEvidence::default()),
        )?,
    )?;
    for version in [1, 2] {
        read_as(
            &database,
            waiting,
            version,
            &evidence(
                Reading::Results(vec![
                    result(waiting, Some("S-0042"))?,
                    result(waiting, Some("s-0042"))?,
                ]),
                Some(analyte_model::DepartureEvidence::default()),
            )?,
        )?;
    }
    deliver(&database, waiting, 1, "done")?;
    deliver(&database, waiting, 2, "pending")?;
    Ok(Seeded {
        unread,
        delivered,
        failed,
        waiting,
        item,
    })
}

// Owner decisions 2026-11-20 (admin restyle b2, Machines mockup): the list
// shows every machine Analyte runs or its settings list, each linked to its own
// page: the model named at startup, the link in its connection type's own words
// with what was observed while it is not open (never "Connected" for serial,
// "Cable not connected" or "Disconnected"), the last data, how it is attached,
// since when an open link is open, and the lab app. Removed, not set up and
// saved machines say so; the line under the title counts them; "Waiting" gets
// its note. No sample ID, test code or value shows.
#[tokio::test(flavor = "multi_thread")]
async fn machines_list_shows_every_machine_in_its_own_words() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), scene()?).await?;
    seed(directory.path(), &store).await?;
    let page = request(&host, "/machines", &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_new_look(&page);
    assert!(
        page.contains(concat!(
            "<p class=\"page-head__lede\">4 of 8 links open · 3 problems · 1 waiting for the ",
            "machine · 1 not set up yet · 1 starts after a restart · 1 removed, stops at the ",
            "restart</p>"
        )),
        "{page}"
    );
    assert!(page.contains("<a class=\"btn btn--primary\" href=\"/setup/add\"><svg"));
    let names = [
        "net-1",
        "serial-2",
        "usb-3",
        "wait-4",
        "folder-5",
        "client-6",
        "stopped-7",
        "removed-8",
        "new-9",
        "saved-10",
    ];
    let mut at = 0;
    for name in names {
        let found = page
            .get(at..)
            .and_then(|rest| rest.find(&format!("<a href=\"/machines/{name}\">{name}</a>")))
            .ok_or(name)?;
        at = at.saturating_add(found);
    }
    let net = row(&page, "net-1");
    for part in [
        "<div class=\"cell-sub\">Model of net-1</div>",
        "title=\"TCP server: Connected\">",
        "</svg>Connected</span>",
        "</svg>Network</span><div class=\"cell-sub mono\">port 5600</div>",
        "<td>main-lab</td>",
        "<td class=\"num\"><time datetime=",
    ] {
        assert!(net.contains(part), "{part}\n{net}");
    }
    assert!(
        !net.contains("cell-note"),
        "an open link needs no note: {net}"
    );
    let serial = row(&page, "serial-2");
    assert!(serial.contains("status--bad"), "{serial}");
    assert!(serial.contains("</svg>Port not open</span>"));
    assert!(serial.contains("<div class=\"cell-sub cell-note\">Port COM3 not open since "));
    assert!(serial.contains("<div class=\"cell-sub mono\">COM3</div>"));
    let usb = row(&page, "usb-3");
    assert!(usb.contains("</svg>Port open</span>"), "{usb}");
    assert!(
        usb.contains("</svg>USB adapter</span><div class=\"cell-sub mono\">serial A10K3XYZ</div>")
    );
    let waiting = row(&page, "wait-4");
    assert!(waiting.contains("status--off"), "{waiting}");
    assert!(waiting.contains("Listening on port 5601, nothing connected since "));
    assert!(row(&page, "folder-5").contains("</svg>Watching</span>"));
    assert!(row(&page, "client-6").contains("No connection to client-6 since "));
    let halted = row(&page, "stopped-7");
    assert!(
        halted.contains("</svg>Stopped in Analyte</span>"),
        "{halted}"
    );
    assert!(halted.contains("Analyte stopped working with this machine"));
    let removed = row(&page, "removed-8");
    assert!(
        removed.contains("</svg>Removed: stops at the restart</span>"),
        "{removed}"
    );
    assert!(removed.contains("Port COM8 open since "));
    for (name, words) in [
        ("new-9", "Not set up yet"),
        ("saved-10", "Saved: starts after a restart"),
    ] {
        let listed = row(&page, name);
        assert!(
            listed.contains(&format!("</svg>{words}</span>")),
            "{listed}"
        );
        assert!(listed.contains("<td class=\"num subtle\">Nothing yet</td>"));
        assert_eq!(
            listed.matches("<td class=\"subtle\">—</td>").count(),
            2,
            "{listed}"
        );
    }
    for word in ["Cable not connected", "Disconnected", "000123", CODE, VALUE] {
        assert!(!page.contains(word), "{word}");
    }
    assert!(page.contains("<p class=\"alert__title\">“Waiting” is normal after a restart</p>"));
    assert!(!page.contains("Order sending is paused"));
    assert!(!page.contains("rel=\"next\""));
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b2): the list keeps every state
// the old page showed: the restore-paused notice, results still waiting for
// removed machines (and that they could not be counted), paging, the empty list
// and its end, and the settings file that could not be read.
#[tokio::test(flavor = "multi_thread")]
async fn machines_list_keeps_its_notices_paging_and_empty_states() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) =
        started(directory.path(), Vec::new()).await?;
    std::fs::write(directory.path().join("program.toml"), "")?;
    let empty = request(&host, "/machines", &auth, None).await?;
    assert!(empty.starts_with("HTTP/1.1 200"), "{empty}");
    assert_new_look(&empty);
    assert!(
        empty.contains("<p class=\"empty__title\">No machine is set up yet</p>"),
        "{empty}"
    );
    assert!(empty.contains("<p class=\"page-head__lede\">No machine is running</p>"));
    assert!(!empty.contains("<table"));
    let past = request(&host, "/machines?after=5", &auth, None).await?;
    assert!(
        past.contains("<p class=\"empty__title\">No machine is set up yet</p>"),
        "{past}"
    );
    let database = directory.path().join("analyte.db");
    let gone = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let receipt = receive(s, "gone-1", AT)?;
            s.audit(&analyte_store::Audit {
                actor: id("operator")?,
                action: id("store-restore")?,
                targets: Vec::new(),
                time: AT,
                reason: Reason::new("synthetic restore").map_err(FixtureError::Text)?,
            })?;
            Ok(receipt)
        })
        .await?;
    read_as(
        &database,
        gone,
        1,
        &evidence(Reading::Results(vec![]), None)?,
    )?;
    deliver(&database, gone, 1, "pending")?;
    let notices = request(&host, "/machines", &auth, None).await?;
    assert!(
        notices.contains(concat!(
            "<p class=\"alert__title\">Order sending is paused after a restore</p>",
            "<p class=\"alert__text\">Ask IT to reconcile orders"
        )),
        "{notices}"
    );
    assert!(notices.contains("<div class=\"alert alert--bad\" role=\"note\">"));
    assert!(notices.contains(
        "Cannot send to lab app main-lab: it is not set up any more. Removed machine gone-1: 1 result or order message is kept and never deleted."
    ), "{notices}");
    rusqlite::Connection::open(&database)?
        .execute_batch("ALTER TABLE outbox RENAME TO outbox_gone;")?;
    let uncounted = request(&host, "/machines", &auth, None).await?;
    assert!(uncounted.starts_with("HTTP/1.1 200"), "{uncounted}");
    assert!(uncounted.contains(
        "<p class=\"alert__title\">Results still waiting from removed machines could not be counted</p>"
    ), "{uncounted}");
    assert!(uncounted.contains("Details for support</summary><pre>"));
    rusqlite::Connection::open(&database)?
        .execute_batch("ALTER TABLE outbox_gone RENAME TO outbox;")?;
    std::fs::remove_file(directory.path().join("program.toml"))?;
    let unreadable = request(&host, "/machines", &auth, None).await?;
    assert!(unreadable.starts_with("HTTP/1.1 200"), "{unreadable}");
    assert!(unreadable.contains(
        "<p class=\"alert__title\">Analyte’s settings could not be read</p><p class=\"alert__text\">Machines set up or removed since Analyte started may be missing here.</p>"
    ), "{unreadable}");
    stop(state, stopper, owner, store).await?;

    // Paging: 51 running machines make two pages; the second has the last one.
    let directory = crate::admin::private_tempdir()?;
    let many = (0..51)
        .map(|n| {
            running(
                &format!("m-{n:02}"),
                (Connection::Serial(format!("COM{n}")), Link::Serial),
                (false, true),
                true,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), many).await?;
    std::fs::write(directory.path().join("program.toml"), "")?;
    let first = request(&host, "/machines", &auth, None).await?;
    assert_eq!(first.matches("<div class=\"cell-main\">").count(), 50);
    assert!(
        first.contains("rel=\"next\" href=\"/machines?after=50\""),
        "{first}"
    );
    let second = request(&host, "/machines?after=50", &auth, None).await?;
    assert_eq!(second.matches("<div class=\"cell-main\">").count(), 1);
    assert!(second.contains("<a href=\"/machines/m-50\">m-50</a>"));
    assert!(!second.contains("rel=\"next\""));
    let end = request(&host, "/machines?after=60", &auth, None).await?;
    assert!(
        end.contains("<p class=\"empty__title\">No more machines</p>"),
        "{end}"
    );
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b2, MachineDetail mockup): a
// machine's page has its status beside its name, the model and the last data
// under it, Test connection (the setup's test step), Edit setup and Remove;
// its setup in words; the accepted differences its settings switch on; and its
// newest stored messages first, bounded, with the sample ID only where stored,
// how each was read, how its deliveries stand by a real join, an open Needs
// review item and how it was read by row. Raw IDs only show under "For
// support" or in links; test codes and values never.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: the head, the setup card, the differences, each message row, the raw IDs and the bound"
)]
async fn machine_page_shows_its_setup_differences_and_newest_messages() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), scene()?).await?;
    let seeded = seed(directory.path(), &store).await?;
    let page = request(&host, "/machines/net-1", &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_new_look(&page);
    for part in [
        "<title>net-1 · Analyte</title>",
        "<ol class=\"crumbs\"><li><a href=\"/machines\">Machines</a></li><li>net-1</li></ol>",
        "<div class=\"cluster page-head__title\"><h1>net-1</h1><span class=\"status status--good\" title=\"TCP server: Connected\">",
        "<p class=\"page-head__lede\">Model of net-1 · last data <time datetime=",
        "<a class=\"btn btn--outline\" href=\"/setup/net-1/test\">Test connection</a>",
        "<a class=\"btn btn--outline\" href=\"/setup/net-1\">Edit setup</a>",
        "<a class=\"btn btn--danger-quiet\" href=\"/setup/net-1/remove\">Remove</a>",
        "<dt>Model</dt><dd>Model of net-1</dd>",
        "<dt>Connected by</dt><dd><span class=\"with-icon\"><svg",
        "</svg>Network, <span class=\"mono\">port 5600</span></span></dd>",
        "<dt>Connection</dt><dd><span class=\"status status--good\"",
        "<span class=\"kv__note\">Machine connected on port 5600 since ",
        "<dt>Sends results to</dt><dd><a class=\"link\" href=\"/setup/lab-apps\">main-lab</a></dd>",
        "<dt>In Analyte</dt><dd>Running</dd>",
        "<dt>Settings version</dt><dd><span class=\"mono\" title=\"abababababab",
        ">abababababab</span></dd>",
        "<h2 id=\"md-diff\" class=\"card__title\">Accepted differences from the standard</h2>",
        concat!(
            "<li title=\"Only absent OBX-11 means a final observation; report completion ",
            "is independent.\">Leaves a result&#39;s status empty when the result is final; ",
            "Analyte reads only an empty result status as final.</li>",
        ),
        "Where these differences come from</summary><pre>rule = &#39;hl7_absent_obx11_means_final&#39;",
    ] {
        assert!(page.contains(part), "{part}\n{page}");
    }
    let messages = page
        .split("<tbody>")
        .nth(1)
        .and_then(|body| body.split("</tbody>").next())
        .ok_or("messages")?;
    let rows: Vec<&str> = messages.split("<tr>").skip(1).collect();
    assert_eq!(rows.len(), 4, "{messages}");
    let [waiting, failed, delivered, unread] = rows.as_slice() else {
        return Err("four rows".into());
    };
    let departures = |receipt: ReceiptId, version: u8| {
        format!(
            "<a class=\"link\" href=\"/receipts/{}/departures?version={version}\">How it was read</a>",
            uuid::Uuid::from(receipt)
        )
    };
    assert!(
        waiting.contains("<span title=\"S-0042, s-0042\">S-0042 +1</span>"),
        "{waiting}"
    );
    assert!(waiting.contains("title=\"reading 2\">"));
    assert!(waiting.contains("</svg>Read · 2 results</span>"));
    assert!(waiting.contains("title=\"Done 1, Pending 1\">"));
    assert!(waiting.contains("</svg>Waiting to be sent · 1 of 2 delivered</span>"));
    assert!(waiting.contains(&departures(seeded.waiting, 2)));
    assert!(
        failed.contains("<td class=\"num subtle\">—</td>"),
        "{failed}"
    );
    assert!(failed.contains("status--bad"));
    assert!(failed.contains("</svg>Could not be read</span>"));
    assert!(failed.contains(&format!(
        "<a class=\"link\" href=\"/parked/{}\">In Needs review</a>",
        seeded.item
    )));
    // Owner decision 2026-11-20 (missing values): a message that was read and
    // queued nothing has no delivery, so its cell says it does not apply.
    assert!(failed.contains("<td class=\"subtle\">—</td>"), "{failed}");
    assert!(failed.contains(&departures(seeded.failed, 1)));
    assert!(
        delivered.contains("<span title=\"000123\">000123</span>"),
        "{delivered}"
    );
    assert!(delivered.contains("</svg>2 delivered</span>"));
    assert!(delivered.contains(&departures(seeded.delivered, 1)));
    assert!(unread.contains("</svg>Not read yet</span>"), "{unread}");
    assert!(unread.contains("title=\"no reading stored\""));
    // Not read yet, so nothing was queued yet.
    assert!(
        unread.contains("<td class=\"subtle\">Nothing yet</td>"),
        "{unread}"
    );
    assert_eq!(
        unread.matches("<td class=\"subtle\">—</td>").count(),
        1,
        "{unread}"
    );
    // Raw IDs: only in links and under "Details for support".
    let (shown, support) = page
        .split_once("Details for support</summary><pre>")
        .ok_or("support")?;
    for receipt in [
        seeded.unread,
        seeded.delivered,
        seeded.failed,
        seeded.waiting,
    ] {
        let uuid = uuid::Uuid::from(receipt).to_string();
        assert_eq!(
            shown.matches(&uuid).count(),
            shown
                .matches(&format!("/receipts/{uuid}/departures"))
                .count(),
            "{uuid} outside a link"
        );
        assert!(
            support.contains(&format!("Message {uuid} received ")),
            "{support}"
        );
    }
    assert!(support.contains("Settings version: abababab"));
    assert!(support.contains("Link: TCP server"));
    for word in [CODE, VALUE, "checksum mismatch"] {
        assert!(!page.contains(word), "{word}");
    }
    // Only the newest 20 are listed, and the page says older ones stay stored.
    store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            for n in 0..20 {
                receive(s, "net-1", AT + n)?;
            }
            Ok(())
        })
        .await?;
    let many = request(&host, "/machines/net-1", &auth, None).await?;
    assert_eq!(
        many.split("<tbody>")
            .nth(1)
            .map(|body| body.matches("<tr>").count()),
        Some(20)
    );
    assert!(many.contains("The newest 20 messages are listed; older ones stay stored."));
    assert!(!many.contains(&uuid::Uuid::from(seeded.waiting).to_string()));
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b2): the page says what Analyte
// observes in each connection type's own words, with what to check on this PC
// for a serial port that does not open; an open serial port never reads
// "Connected"; a stopped service, a removed machine (no setup actions), one not
// set up (what to do, Remove only) and one set up since the start say so.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: one page per connection type and per setup state"
)]
async fn each_machine_page_reads_in_its_own_words() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), scene()?).await?;
    let page = |path: &'static str| {
        let (host, auth) = (host.clone(), auth.clone());
        async move { request(&host, path, &auth, None).await }
    };
    let serial = page("/machines/serial-2").await?;
    assert!(serial.contains("</svg>Port not open</span>"), "{serial}");
    assert!(serial.contains("</svg>Serial, <span class=\"mono\">COM3</span></span></dd>"));
    assert!(serial.contains("<span class=\"kv__note\">Port COM3 not open since "));
    assert!(serial.contains(
        "Check that the USB adapter or cable is plugged into this PC, that COM3 is the right port, and that no other program is using it.</span>"
    ));
    assert!(serial.contains("<p class=\"empty__title\">Nothing received yet</p>"));
    assert!(
        serial
            .contains("<p class=\"page-head__lede\">Model of serial-2 · nothing received yet</p>")
    );
    let usb = page("/machines/usb-3").await?;
    assert!(usb.contains("</svg>Port open</span>"), "{usb}");
    assert!(
        !usb.contains("</svg>Connected</span>") && !usb.contains("connected since"),
        "an open serial port is not a connection: {usb}"
    );
    assert!(usb.contains("<span class=\"kv__note\">Found by its USB serial number, so it keeps working if Windows gives the adapter another port name.</span>"));
    assert!(usb.contains("Port of USB adapter A10K3XYZ open since "));
    let waiting = page("/machines/wait-4").await?;
    assert!(
        waiting.contains("Listening on port 5601, nothing connected since "),
        "{waiting}"
    );
    let folder = page("/machines/folder-5").await?;
    assert!(folder.contains("</svg>Watching</span>"), "{folder}");
    assert!(folder.contains("Watching results</span>"));
    assert!(folder.contains("</svg>Watched folder, <span class=\"mono\">results</span>"));
    let client = page("/machines/client-6").await?;
    assert!(
        client.contains("</svg>Not connected (retrying)</span>"),
        "{client}"
    );
    assert!(client.contains("No connection to client-6 since "));
    assert!(
        client.contains("Check the network cable and the machine&#39;s host settings."),
        "{client}"
    );
    let halted = page("/machines/stopped-7").await?;
    assert!(
        halted.contains("</svg>Stopped in Analyte</span>"),
        "{halted}"
    );
    assert!(halted.contains("<dt>In Analyte</dt><dd>Stopped</dd>"));
    let removed = page("/machines/removed-8").await?;
    assert!(
        removed.contains("</svg>Removed: stops at the restart</span>"),
        "{removed}"
    );
    assert!(removed.contains(concat!(
        "<p class=\"alert__title\">Removed from Analyte’s settings</p><p class=\"alert__text\">",
        "After the next restart Analyte stops connecting to it; until then it keeps running. ",
        "Its results, items to review and history are kept and stay visible here.</p>"
    )));
    for action in ["Test connection", "Edit setup", "/setup/removed-8/remove"] {
        assert!(!removed.contains(action), "{action}");
    }
    let new = page("/machines/new-9").await?;
    assert!(new.starts_with("HTTP/1.1 200"), "{new}");
    assert!(new.contains("</svg>Not set up yet</span>"));
    assert!(new.contains(
        "<p class=\"whattodo__text\"><strong>What to do:</strong> set it up: pick its model"
    ));
    assert!(
        new.contains("<a class=\"btn btn--primary btn--sm\" href=\"/setup/new-9\">Set it up</a>")
    );
    assert!(
        new.contains("<a class=\"btn btn--danger-quiet\" href=\"/setup/new-9/remove\">Remove</a>")
    );
    assert!(!new.contains("Test connection"));
    assert!(new.contains("<dt>Model</dt><dd>Not chosen yet</dd>"));
    assert!(new.contains("<dt>In Analyte</dt><dd>Not running yet</dd>"));
    assert!(!new.contains("md-diff"));
    let saved = page("/machines/saved-10").await?;
    assert!(
        saved.contains("</svg>Saved: starts after a restart</span>"),
        "{saved}"
    );
    assert!(saved.contains("href=\"/setup/saved-10/test\">Test connection</a>"));
    assert!(saved.contains("<dt>Model</dt><dd>Shown after Analyte restarts</dd>"));
    let missing = page("/machines/nobody").await?;
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert_new_look(&missing);
    assert!(missing.contains("<p class=\"alert__title\">No machine is called this</p>"));
    let odd = page("/machines/a%20b%2Fc").await?;
    assert!(odd.starts_with("HTTP/1.1 404"), "{odd}");
    std::fs::remove_file(directory.path().join("program.toml"))?;
    let unread = page("/machines/net-1").await?;
    assert!(unread.starts_with("HTTP/1.1 200"), "{unread}");
    assert!(unread.contains("<p class=\"alert__title\">Analyte’s settings could not be read</p>"));
    assert!(!unread.contains("Test connection"));
    let unknown = page("/machines/new-9").await?;
    assert!(unknown.starts_with("HTTP/1.1 503"), "{unknown}");
    assert!(unknown.contains("Analyte could not read its settings just now"));
    stop(state, stopper, owner, store).await
}

// Owner decision 2026-11-20 (admin restyle b2): a machine name the settings
// may hold with characters a URL path cannot carry still links to its page.
#[tokio::test(flavor = "multi_thread")]
async fn odd_machine_names_link_to_their_own_page() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let odd = running(
        "lab bench/2 <x>",
        (Connection::Serial("COM4".into()), Link::Serial),
        (false, true),
        true,
    )?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), vec![odd]).await?;
    let list = request(&host, "/machines", &auth, None).await?;
    let href = "/machines/lab%20bench%2F2%20%3Cx%3E";
    assert!(
        list.contains(&format!("<a href=\"{href}\">lab bench/2 &#60;x&#62;</a>")),
        "{list}"
    );
    let page = request(&host, href, &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<h1>lab bench/2 &#60;x&#62;</h1>"));
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b2): how a message was read shows
// the reading (number, reader, settings label and version), the differences
// switched on and those used with what each means, each result part's status
// as sent and as shown, the earlier and the next reading only when they exist,
// and the stored reasons for support. Older versions that recorded no
// differences, an incomplete record and a failed reading say so.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: two readings, a failed one, differences used, a legacy reading and the refusals"
)]
async fn how_a_message_was_read_shows_its_reading_and_differences() -> Result<(), Error> {
    use analyte_model::{AppliedDeparture, DepartureEvidence, DepartureMeaning};
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), scene()?).await?;
    let seeded = seed(directory.path(), &store).await?;
    let database = directory.path().join("analyte.db");
    let path = |receipt: ReceiptId, version: i64| {
        format!(
            "/receipts/{}/departures?version={version}",
            uuid::Uuid::from(receipt)
        )
    };
    let first = request(&host, &path(seeded.waiting, 1), &auth, None).await?;
    assert!(first.starts_with("HTTP/1.1 200"), "{first}");
    assert_new_look(&first);
    for part in [
        "<ol class=\"crumbs\"><li><a href=\"/machines\">Machines</a></li><li><a href=\"/machines/net-1\">net-1</a></li></ol>",
        "<h1>How this message was read</h1>",
        "<p class=\"page-head__lede\">From net-1 · arrived <time datetime=",
        " · reading 1</p>",
        "<dt>Reading</dt><dd>1</dd>",
        "<dt>Read by</dt><dd>synthetic, version 1</dd>",
        "<dt>Settings label</dt><dd>site-7</dd>",
        "<p class=\"departure__none\">None were switched on.</p>",
        "<p class=\"departure__none\">None were used.</p>",
        "<td class=\"mono\">F</td><td>Final</td>",
        "href=\"/receipts/",
        "\">Next reading</a>",
    ] {
        assert!(first.contains(part), "{part}\n{first}");
    }
    assert!(!first.contains("Earlier reading"));
    for word in [CODE, VALUE, "S-0042"] {
        assert!(!first.contains(word), "{word}");
    }
    let second = request(&host, &path(seeded.waiting, 2), &auth, None).await?;
    assert!(second.contains("\">Earlier reading</a>"), "{second}");
    assert!(!second.contains("Next reading"));
    let failed = request(&host, &path(seeded.failed, 1), &auth, None).await?;
    assert!(
        failed.contains(concat!(
            "<p class=\"alert__title\">This message could not be read</p>",
            "<p class=\"alert__text\">record 4, field 3: UnknownTest</p>"
        )),
        "{failed}"
    );
    assert!(failed.contains("Reason as stored: Position { record: 4, field: 3 }: UnknownTest"));
    assert!(!failed.contains("Result statuses"));
    // A reading with differences switched on and used, and one with an incomplete record.
    let declared = SourceText::new("rule = 'hl7_absent_obx11_means_final'\nscope = '<b>'")?;
    let used = DepartureEvidence {
        enabled: EvidenceList::new(vec![declared.clone()])?,
        applied: EvidenceList::new(vec![AppliedDeparture {
            rule: Identifier::new("hl7_absent_obx11_means_final")?,
            declaration: declared,
            standard: Identifier::new("HL7 2.5.1 §7.4.2.11")?,
            record: 3,
            field: 11,
            source: SourceText::new("absent")?,
            meaning: DepartureMeaning::FinalObservationFromDocumentation,
            coordinates: Identifier::new("syntax_bytes")?,
            spans: EvidenceList::default(),
        }])?,
        incomplete: Some(Reason::new("ledger bound reached")?),
    };
    read_as(
        &database,
        seeded.delivered,
        2,
        &evidence(Reading::Results(vec![]), Some(used))?,
    )?;
    let differences = request(&host, &path(seeded.delivered, 2), &auth, None).await?;
    for part in [
        "<p class=\"alert__title\">Not sent to the lab app: the record of differences used is incomplete</p><p class=\"alert__text\">ledger bound reached</p>",
        "Their declarations</summary><pre>rule = &#39;hl7_absent_obx11_means_final&#39;\nscope = &#39;&#60;b&#62;&#39;",
        "<h3 class=\"departure__rule mono\">hl7_absent_obx11_means_final</h3>",
        "<p class=\"alert__title\">Marked final because the manual says so, not because the machine sent a final status</p>",
        "<dt>As received</dt><dd>absent</dd>",
        "<dt>Read as</dt><dd>FinalObservationFromDocumentation</dd>",
        "<dt>Standard</dt><dd>HL7 2.5.1 §7.4.2.11</dd>",
        "<dt>Where in the message</dt><dd><span class=\"mono\" title=\"Coordinates: syntax_bytes; spans: EvidenceList([])\">Record 3, field 11</span></dd>",
        "Incomplete record, reason as stored: ledger bound reached",
        "<p class=\"empty__title\">No results</p>",
    ] {
        assert!(differences.contains(part), "{part}\n{differences}");
    }
    assert!(!differences.contains("<b>"));
    read_as(
        &database,
        seeded.delivered,
        3,
        &evidence(Reading::Results(vec![]), None)?,
    )?;
    let legacy = request(&host, &path(seeded.delivered, 3), &auth, None).await?;
    assert!(
        legacy.contains("Not recorded by the version that read this message."),
        "{legacy}"
    );
    assert!(legacy.contains(
        "did not record which differences were switched on or used; that does not mean none were."
    ));
    // Refusals keep their status codes in the new look.
    for (version, status, words) in [
        (0, "400", "Readings are numbered from 1"),
        (9, "404", "This message has no reading with this number"),
    ] {
        let refused = request(&host, &path(seeded.waiting, version), &auth, None).await?;
        assert!(
            refused.starts_with(&format!("HTTP/1.1 {status}")),
            "{refused}"
        );
        assert_new_look(&refused);
        assert!(refused.contains(words), "{refused}");
    }
    let unread = request(&host, &path(seeded.unread, 1), &auth, None).await?;
    assert!(unread.starts_with("HTTP/1.1 404"), "{unread}");
    let bad = request(&host, "/receipts/not-an-id/departures", &auth, None).await?;
    assert!(bad.starts_with("HTTP/1.1 400"), "{bad}");
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b2): when the store cannot be read
// or the reads outlast the read budget, every Machines page answers with its
// error page in the new look and the reason for support.
#[tokio::test(flavor = "multi_thread")]
async fn store_failures_and_slow_reads_answer_in_the_new_look() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth) = started(directory.path(), scene()?).await?;
    let seeded = seed(directory.path(), &store).await?;
    let departures = format!(
        "/receipts/{}/departures",
        uuid::Uuid::from(seeded.delivered)
    );
    for path in ["/machines", "/machines/net-1", departures.as_str()] {
        let busy = store.clone();
        let blocker = tokio::spawn(async move {
            busy.request::<_, FixtureError>(fixture_deadline()?, |_| {
                std::thread::sleep(Duration::from_millis(1800));
                Ok(())
            })
            .await
            .map_err(Error::from)
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let slow = request(&host, path, &auth, None).await?;
        assert!(slow.starts_with("HTTP/1.1 504"), "{path}: {slow}");
        assert_new_look(&slow);
        assert!(
            slow.contains("Analyte did not finish reading within 1.5 seconds. Reload the page;")
        );
        assert!(slow.contains("admin Machines deadline of 1500 ms exceeded"));
        blocker.await??;
    }
    stop(state, stopper, owner, store).await?;
    let directory = crate::admin::private_tempdir()?;
    std::fs::write(directory.path().join("program.toml"), PROGRAM)?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let mut closed = Arc::try_unwrap(empty_store(directory.path()).await?).map_err(|_| "shared")?;
    closed.shutdown(fixture_deadline()?).await?;
    let store = Arc::new(closed);
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, scene()?, Vec::new()).await?;
    let key = format!("Authorization: Bearer {bearer}\r\n");
    for path in ["/machines", "/machines/net-1", departures.as_str()] {
        let failed = request(&host, path, &key, None).await?;
        assert!(failed.starts_with("HTTP/1.1 503"), "{path}: {failed}");
        assert_new_look(&failed);
        assert!(failed.contains(
            "<p class=\"alert__title\">Analyte could not read its stored data just now</p>"
        ));
        assert!(failed.contains("Details for support</summary><pre>"));
    }
    drop(state);
    stopper.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    Ok(())
}

// Owner decision 2026-11-20 (admin restyle b2): Remove changes settings, so a
// request with the admin key alone gets "Sign in first" in the new look, and
// nothing is read or changed.
#[tokio::test(flavor = "multi_thread")]
async fn remove_needs_a_signed_in_session() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    std::fs::write(directory.path().join("program.toml"), PROGRAM)?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, scene()?, Vec::new()).await?;
    let key = format!(
        "Authorization: Bearer {bearer}
"
    );
    let refused = request(&host, "/setup/new-9/remove", &key, None).await?;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    assert_new_look(&refused);
    assert!(refused.contains("<h1>Sign in first</h1>"), "{refused}");
    assert!(refused.contains(
        "<p class=\"alert__title\">Removing a machine changes settings, so it needs a signed-in browser session</p>"
    ));
    assert!(
        refused.contains(
            "<a class=\"btn btn--primary\" href=\"/machines/new-9\">Open the machine</a>"
        )
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("program.toml"))?,
        PROGRAM
    );
    stop(state, stopper, owner, store).await
}
