//! Tests the Lab apps pages through the real handlers: the list by the Overview's lab app rules, the form with where its keys are found, the Remove dialog, and what a save or a removal says.
use super::super::overview::{empty_store, queue, stop};
use super::*;

/// The settings of the list scene: machines that pick lab apps, a lab app
/// kept from a removed machine, and one still written in a machine's own file.
const PROGRAM: &str = r"
[[machines]]
id = 'chem-1'
definition = 'machines/a/machine.toml'
site = 'chem-1.site.toml'
[[machines]]
id = 'hema-2'
definition = 'machines/a/machine.toml'
site = 'hema-2.site.toml'
[[machines]]
id = 'immuno-3'
definition = 'machines/a/machine.toml'
site = 'immuno-3.site.toml'
[[machines]]
id = 'old-4'
definition = 'machines/a/machine.toml'
site = 'old-4.site.toml'
[[retired_destinations]]
name = 'kept-lab'
base_url = 'https://kept-lab.example.invalid'
api_key_name = 'kept-lab-api'
signing_key_name = 'kept-lab-signing'
";

/// One `[[lab_apps]]` table, or a site file's `[destination]` when `table` says so.
fn lab_app(table: &str, name: &str) -> String {
    format!(
        "[{table}]\nname = '{name}'\nbase_url = 'https://{name}.example.invalid'\napi_key_name = '{name}-api'\nsigning_key_name = '{name}-signing'\n"
    )
}

/// The lab apps file listing `names`.
fn lab_apps_file(names: &[&str]) -> String {
    let mut text = "schema_version = 1\n".to_owned();
    for name in names {
        text.push_str(&lab_app("[lab_apps]", name));
    }
    text
}

/// A lab apps page server over the files in `root`, with `destinations`
/// running and `kept` lab apps (name, keys missing at startup) draining.
async fn serve(
    root: &std::path::Path,
    store: &Arc<analyte_store::Handle>,
    destinations: &[&str],
    kept: &[(&str, bool)],
) -> Result<(Started, String), Error> {
    let bearer = crate::admin::rotate_token(root)?;
    let retired = kept
        .iter()
        .map(|&(name, keys_missing)| {
            Ok((
                name.to_owned(),
                crate::admin::Kept {
                    delivering: Arc::new(AtomicBool::new(true)),
                    keys_missing,
                    api_key: Identifier::new(&format!("{name}-api"))?,
                    signing_key: Identifier::new(&format!("{name}-signing"))?,
                },
            ))
        })
        .collect::<Result<crate::admin::Draining, Error>>()?;
    let running = destinations
        .iter()
        .map(|name| loopback_destination(name))
        .collect::<Result<Vec<_>, Error>>()?;
    let started =
        super::super::start_with(root, store, (Vec::new(), running), retired, root).await?;
    let cookie = login(&started.3, &bearer).await?;
    Ok((started, format!("Cookie: {cookie}\r\n")))
}

/// Stores both keys of each lab app in `names` in the locked file at `set_at`.
fn store_keys(root: &std::path::Path, names: &[&str], set_at: i64) -> Result<(), Error> {
    let mut updates = Vec::new();
    for name in names {
        for (key, value) in [("api", API_KEY), ("signing", SIGNING_KEY)] {
            let id = Identifier::new(&format!("{name}-{key}"))?;
            let secret = crate::installation::credentials::Secret::new(&id, value.to_owned())?;
            updates.push((id, secret));
        }
    }
    crate::installation::credentials::Locked::new(root).replace(updates, set_at)?;
    Ok(())
}

/// Writes the list scene's settings into `root`.
fn write_scene(root: &std::path::Path) -> Result<(), Error> {
    std::fs::write(root.join("program.toml"), PROGRAM)?;
    for (machine, lab) in [
        ("chem-1", "main-lab"),
        ("hema-2", "main-lab"),
        ("immuno-3", "up-lab"),
    ] {
        std::fs::write(
            root.join(format!("{machine}.site.toml")),
            format!("lab_app = '{lab}'\n"),
        )?;
    }
    std::fs::write(
        root.join("old-4.site.toml"),
        lab_app("destination", "site-lab"),
    )?;
    std::fs::write(
        root.join("lab-apps.toml"),
        lab_apps_file(&[
            "up-lab",
            "queued-lab",
            "failing-lab",
            "paused-lab",
            "main-lab",
            "new-lab",
        ]),
    )?;
    Ok(())
}

/// Waiting, delivered, failing and paused items for the list scene's lab apps.
fn seed(store: &mut analyte_store::Store, now: i64) -> Result<(), FixtureError> {
    queue(store, "up-lab", "order-up", true, now.saturating_sub(120))?;
    queue(
        store,
        "queued-lab",
        "order-queued-1",
        false,
        now.saturating_sub(900),
    )?;
    queue(
        store,
        "queued-lab",
        "order-queued-2",
        false,
        now.saturating_sub(60),
    )?;
    queue(
        store,
        "main-lab",
        "order-main",
        false,
        now.saturating_sub(600),
    )?;
    queue(
        store,
        "failing-lab",
        "order-failing",
        false,
        now.saturating_sub(2400),
    )?;
    let failing = Identifier::new("failing-lab")?;
    for (tried, next) in [
        (now.saturating_sub(2400), now.saturating_sub(2340)),
        (now.saturating_sub(60), now.saturating_add(600)),
    ] {
        let claim = store
            .claim_destination(tried, &failing)?
            .ok_or(FixtureError::UnknownDelivery)?;
        store.attempted(claim.id, &analyte_store::Attempt::RetryAt(next), tried)?;
    }
    queue(
        store,
        "paused-lab",
        "order-paused",
        false,
        now.saturating_sub(300),
    )?;
    let paused = Identifier::new("paused-lab")?;
    let claim = store
        .claim_destination(now.saturating_sub(300), &paused)?
        .ok_or(FixtureError::UnknownDelivery)?;
    store.attempted(
        claim.id,
        &analyte_store::Attempt::Paused(Reason::new("synthetic pause")?),
        now.saturating_sub(300),
    )?;
    // Results for a lab app the settings no longer write.
    queue(
        store,
        "gone-lab",
        "order-gone",
        false,
        now.saturating_sub(60),
    )?;
    Ok(())
}

/// The card of lab app `name` on the list.
fn card<'a>(page: &'a str, name: &str) -> &'a str {
    page.split("<article class=\"card")
        .find(|card| card.contains(&format!("class=\"card__title\">{name}</h2>")))
        .and_then(|card| card.split("</article>").next())
        .unwrap_or_default()
}

/// Every Lab apps page is in the new look, Lab apps marked, with the strict headers.
fn assert_new_look(page: &str) {
    headers_present(page);
    assert!(
        page.contains("<nav class=\"sidebar\" aria-label=\"Main\">"),
        "{page}"
    );
    assert!(page.contains("app.css") && !page.contains("admin.css"));
    assert!(page.contains("<a href=\"/setup/lab-apps\" aria-current=\"page\">"));
    assert!(!page.contains("style=") && !other_script(page));
    assert!(
        !page.contains("Receiving"),
        "Analyte cannot see the lab app receive"
    );
}

// Owner decisions 2026-11-20 (admin restyle b3): each lab app is a card whose
// status follows the Overview's lab node rules (Up to date, Queued, Not
// delivering, Sending paused, Cannot deliver) with the same last delivered,
// waiting and failing-try lines; one Analyte does not run yet says so. Keys
// show only where they are found; who uses it links to each machine's page.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scene: every lab app state on one list, each card checked in turn"
)]
async fn lab_apps_list_reads_each_lab_app_by_the_overview_rules() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    write_scene(root)?;
    let now = jiff::Timestamp::now().as_second();
    store_keys(
        root,
        &[
            "up-lab",
            "queued-lab",
            "failing-lab",
            "paused-lab",
            "new-lab",
            "kept-lab",
            "site-lab",
        ],
        now - 86_400,
    )?;
    let store = empty_store(root).await?;
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| seed(s, now))
        .await?;
    let ((state, stopper, owner, host), auth) = serve(
        root,
        &store,
        &["up-lab", "queued-lab", "failing-lab", "paused-lab"],
        &[("main-lab", true), ("kept-lab", false), ("site-lab", false)],
    )
    .await?;
    let page = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_new_look(&page);
    assert!(page.contains("<h1>Lab apps</h1>"));
    assert!(page.contains("href=\"/setup/lab-apps/new\">"));
    for (name, tone, word) in [
        ("up-lab", "good", "Up to date"),
        ("queued-lab", "off", "Queued"),
        ("failing-lab", "bad", "Not delivering"),
        ("paused-lab", "warn", "Sending paused"),
        ("main-lab", "bad", "Cannot deliver"),
        ("new-lab", "warn", "Starts after a restart"),
        ("kept-lab", "good", "Up to date"),
        ("site-lab", "good", "Up to date"),
    ] {
        let card = card(&page, name);
        assert!(
            card.contains(&format!("<span class=\"status status--{tone}\">")),
            "{name}: {card}"
        );
        assert!(
            card.contains(&format!("</svg>{word}</span>")),
            "{name}: {card}"
        );
        assert!(card.contains(&format!(
            "<dt>Address</dt><dd class=\"mono\">https://{name}.example.invalid</dd>"
        )));
        assert!(card.contains(&format!("href=\"/setup/lab-apps/{name}\">Edit</a>")));
        assert!(card.contains(&format!(
            "href=\"/setup/lab-apps/{name}/remove\">Remove</a>"
        )));
        assert_eq!(
            card.contains(" card--bad\""),
            tone == "bad",
            "{name}: {card}"
        );
    }
    let up = card(&page, "up-lab");
    for part in [
        "<span class=\"mono subtle lab__aside\">Last delivered <time ",
        "<dt>Used by</dt><dd><a class=\"link\" href=\"/machines/immuno-3\">immuno-3</a></dd>",
        "<dt>API key</dt><dd><span class=\"status status--good\" title=\"up-lab-api, stored ",
        "</svg>Set on ",
        "<dt>Waiting to be sent</dt><dd>nothing waiting</dd>",
        "<dt>Order list</dt><dd>Not checked yet</dd>",
    ] {
        assert!(up.contains(part), "{part}\n{up}");
    }
    assert!(!up.contains("whattodo") && !up.contains("<dt>Tries</dt>"));
    let queued = card(&page, "queued-lab");
    assert!(queued.contains("<dt>Waiting to be sent</dt><dd>2 items waiting · oldest <time "));
    assert!(queued.contains("<dt>Used by</dt><dd><span class=\"subtle\">No machine</span></dd>"));
    let failing = card(&page, "failing-lab");
    for part in [
        "<dt>Tries</dt><dd>Last try <time ",
        "</time> failed · 2 failed tries since <time ",
        "<strong>What to do:</strong> Check that the lab app is running and that this PC can reach it at its address.",
        "href=\"/deliveries\">Deliveries</a>",
    ] {
        assert!(failing.contains(part), "{part}\n{failing}");
    }
    let main = card(&page, "main-lab");
    for part in [
        "Nothing delivered yet</span>",
        "<a class=\"link\" href=\"/machines/chem-1\">chem-1</a>, <a class=\"link\" href=\"/machines/hema-2\">hema-2</a>",
        "<dt>Signing key</dt><dd><span class=\"status status--bad\" title=\"main-lab-signing\">",
        "</svg>Missing</span>",
        "<strong>What to do:</strong> add the missing keys on the Lab apps page or in the service environment. Waiting items go out after Analyte restarts with the keys.",
        "<a class=\"btn btn--primary btn--sm\" href=\"/setup/lab-apps/main-lab\">Add keys</a>",
        "<dt>Waiting to be sent</dt><dd>1 item waiting · oldest <time ",
    ] {
        assert!(main.contains(part), "{part}\n{main}");
    }
    assert!(!main.contains("Key missing"), "the key rows already say it");
    let new = card(&page, "new-lab");
    assert!(new.contains("Nothing delivered yet</span>"), "{new}");
    assert!(!new.contains("<dt>Order list</dt>"));
    assert!(card(&page, "kept-lab").contains(
        "<dt>Written in</dt><dd><span title=\"[[retired_destinations]] in the program file; saving it here moves it to lab-apps.toml\">Kept from a removed machine</span></dd>"
    ));
    assert!(card(&page, "site-lab").contains(">Still kept in the settings of old-4</span></dd>"));
    // Results for a lab app the settings no longer write stay visible under the list.
    assert!(page.contains(
        "<p class=\"alert__title\">Cannot send to lab app gone-lab: it is not set up any more."
    ));
    for hidden in [
        API_KEY,
        SIGNING_KEY,
        "order-up",
        "order-queued-1",
        "synthetic-order-message",
    ] {
        assert!(!page.contains(hidden), "{hidden} shown");
    }
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b3): the list keeps every notice
// the old page had: no lab app yet, a name written differently in two places,
// a lab apps file that cannot be read (503, nothing to add), and keys that
// cannot be checked with the reason for support.
#[tokio::test(flavor = "multi_thread")]
async fn lab_apps_list_keeps_its_notices_and_empty_state() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    std::fs::write(root.join("program.toml"), "")?;
    let store = empty_store(root).await?;
    let ((state, stopper, owner, host), auth) = serve(root, &store, &[], &[]).await?;
    let empty = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(empty.starts_with("HTTP/1.1 200"), "{empty}");
    assert_new_look(&empty);
    assert!(empty.contains("<p class=\"empty__title\">No lab app is set up yet</p>"));
    assert!(empty.contains(
        "<a class=\"btn btn--outline btn--sm\" href=\"/setup/lab-apps/new\">Add a lab app</a>"
    ));
    // One name, two different settings.
    std::fs::write(
        root.join("program.toml"),
        "[[machines]]\nid = 'old-4'\nsite = 'old-4.site.toml'\n",
    )?;
    std::fs::write(root.join("lab-apps.toml"), lab_apps_file(&["twice"]))?;
    std::fs::write(
        root.join("old-4.site.toml"),
        lab_app("destination", "twice").replace("twice.example", "other.example"),
    )?;
    let conflict = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(conflict.contains(
        "<p class=\"alert__title\">Lab app twice has different settings in two places (one is machine old-4’s own settings). Analyte cannot start until they are the same.</p>"
    ) || conflict.contains(
        "<p class=\"alert__title\">Lab app twice has different settings in two places (one is machine old-4&#39;s own settings). Analyte cannot start until they are the same.</p>"
    ), "{conflict}");
    // Keys that cannot be checked: the locked file is not readable as keys.
    std::fs::create_dir_all(root.join("credentials"))?;
    std::fs::write(root.join("credentials/credentials.toml"), "not keys")?;
    let unchecked = request(&host, "/setup/lab-apps", &auth, None).await?;
    let twice = card(&unchecked, "twice");
    assert!(
        twice.contains("<span class=\"status status--warn\" title=\"twice-api\">"),
        "{twice}"
    );
    assert!(twice.contains("</svg>Cannot be checked</span>"));
    assert!(unchecked.contains("Details for support</summary><pre>Keys file: "));
    // The lab apps file cannot be read: nothing is listed or offered.
    std::fs::write(
        root.join("lab-apps.toml"),
        "schema_version = 1\n[[lab_apps]]\nname = 'broken'\nunknown_setting = 1\n",
    )?;
    let unreadable = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(unreadable.starts_with("HTTP/1.1 503"), "{unreadable}");
    assert_new_look(&unreadable);
    assert!(
        unreadable.contains("<p class=\"alert__title\">The lab apps file could not be read</p>")
    );
    assert!(unreadable.contains("unknown_setting"));
    assert!(!unreadable.contains("href=\"/setup/lab-apps/new\""));
    assert!(!unreadable.contains("No lab app is set up yet"));
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b3): a new lab app's form asks for
// its name and both keys; an existing one's name is fixed, the machines that
// send to it link to their pages, and each key says only where it is found:
// missing with why it is needed, or set with Replace, never the value.
#[tokio::test(flavor = "multi_thread")]
async fn lab_app_form_says_where_keys_are_and_never_their_values() -> Result<(), Error> {
    let fixture = start().await?;
    let new = fixture.get("/setup/lab-apps/new").await?;
    assert!(new.starts_with("HTTP/1.1 200"), "{new}");
    assert_new_look(&new);
    for part in [
        "<h1>Add a lab app</h1>",
        "<form class=\"stack stack--lg narrow\" method=\"post\" action=\"/setup/lab-apps/new\">",
        "<input id=\"la-name\" class=\"input\" name=\"name\" required maxlength=\"64\" value=\"lab-app\">",
        "<input id=\"la-api\" class=\"input mono\" type=\"password\" name=\"api_key\" required autocomplete=\"off\" maxlength=\"4096\">",
        "<input id=\"la-signing\" class=\"input mono\" type=\"password\" name=\"signing_key\" required autocomplete=\"off\" maxlength=\"4096\">",
        "value=\"30\"",
        "value=\"10\"",
        "The defaults suit most labs: 30 and 10 seconds.",
        "None yet. Each machine picks its lab app in setup step 2.",
        "<a class=\"btn btn--outline\" href=\"/setup/lab-apps\">Cancel</a><button type=\"submit\" class=\"btn btn--primary\">Save</button>",
    ] {
        assert!(new.contains(part), "{part}\n{new}");
    }
    assert!(!new.contains("Remove this lab app"));
    let edit = fixture.get("/setup/lab-apps/demo").await?;
    assert!(edit.starts_with("HTTP/1.1 200"), "{edit}");
    for part in [
        "<li><a href=\"/setup/lab-apps\">Lab apps</a></li><li>demo</li>",
        "<h1>Edit demo</h1>",
        "<input id=\"la-name\" class=\"input\" value=\"demo\" readonly>",
        "<p class=\"alert__title\">Still kept in the settings of synthetic</p>",
        "<li><a class=\"link\" href=\"/machines/synthetic\">synthetic</a></li>",
        "<input id=\"la-api\" class=\"input mono\" type=\"password\" name=\"api_key\" autocomplete=\"off\" maxlength=\"4096\" placeholder=\"Paste the api key\"",
        "</svg>Needed before Analyte can send results.</span>",
        "href=\"/setup/lab-apps/demo/remove\">Remove this lab app</a>",
    ] {
        assert!(edit.contains(part), "{part}\n{edit}");
    }
    assert!(!edit.contains("name=\"name\""), "the name is fixed");
    let saved = fixture
        .submit(
            "/setup/lab-apps/demo",
            "/setup/lab-apps/demo",
            &[
                ("base_url", "https://lab.example.invalid"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    let set = fixture.get("/setup/lab-apps/demo").await?;
    assert_eq!(
        set.matches("<details class=\"replace\"><summary class=\"secret\">")
            .count(),
        2,
        "{set}"
    );
    assert!(set.contains("<span class=\"btn btn--outline btn--sm\">Replace</span></summary>"));
    assert!(set.contains("<label class=\"field__label\" for=\"la-api\">New api key</label>"));
    assert!(!set.contains("Needed before Analyte can send results."));
    let unknown = fixture.get("/setup/lab-apps/nowhere").await?;
    assert!(unknown.starts_with("HTTP/1.1 409"), "{unknown}");
    assert!(unknown.contains("There is no lab app called nowhere; add it on the Lab apps page."));
    for page in [&new, &edit, &saved, &set, &unknown] {
        assert!(!page.contains(API_KEY) && !page.contains(SIGNING_KEY));
    }
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b3): Remove is a dialog over the
// lab app's form, a real form with the session's token and Cancel back to the
// form; while a machine uses the lab app it lists why and offers no Remove.
#[tokio::test(flavor = "multi_thread")]
async fn remove_dialog_draws_over_the_lab_app_form() -> Result<(), Error> {
    let fixture = start().await?;
    let blocked = fixture.get("/setup/lab-apps/demo/remove").await?;
    assert!(blocked.starts_with("HTTP/1.1 200"), "{blocked}");
    assert_new_look(&blocked);
    for part in [
        "<h1>Edit demo</h1>",
        "<div class=\"scrim\">",
        "<form class=\"dialog\" role=\"dialog\" aria-modal=\"true\" aria-labelledby=\"dlg-title\" method=\"post\" action=\"/setup/lab-apps/demo/remove\">",
        "<h2 id=\"dlg-title\" class=\"dialog__title\">Remove demo?</h2>",
        "<p class=\"dialog__text\">Results already sent and their history stay.</p>",
        "<p class=\"dialog__text\">No key is stored for it, so none is kept.</p>",
        "<p class=\"dialog__label\">It cannot be removed now</p>",
        "<span>Machines send results to it: synthetic. Pick another lab app on their setup pages, or remove them, first.</span>",
        "<a class=\"btn btn--outline\" href=\"/setup/lab-apps/demo\">Cancel</a></div>",
    ] {
        assert!(blocked.contains(part), "{part}\n{blocked}");
    }
    assert!(!blocked.contains("btn--danger\">Remove</button>"));
    let token = csrf(&blocked)?;
    assert!(blocked.contains(&format!(
        "<input type=\"hidden\" name=\"csrf\" value=\"{token}\">"
    )));
    let added = fixture
        .submit(
            "/setup/lab-apps/new",
            "/setup/lab-apps/new",
            &[
                ("name", "spare"),
                ("base_url", "https://spare.example.invalid"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    let free = fixture.get("/setup/lab-apps/spare/remove").await?;
    for part in [
        "<p class=\"dialog__text\">Its keys stay stored, locked.</p>",
        "<p class=\"dialog__label\">Before you remove it</p>",
        "<span>No machine uses it and nothing waits for it.</span>",
        "<span>Analyte keeps a dated copy of the old settings.</span>",
        "<a class=\"btn btn--outline\" href=\"/setup/lab-apps/spare\">Cancel</a><button type=\"submit\" class=\"btn btn--danger\">Remove</button>",
    ] {
        assert!(free.contains(part), "{part}\n{free}");
    }
    assert!(!free.contains("name=\"anyway\""));
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b3): a save says which keys were
// stored (never their values), that waiting items go out after a restart with
// the keys, and offers Restart now; a removal says the keys stay; the dated
// copies go under Details for support; an audit that cannot be written shows.
#[tokio::test(flavor = "multi_thread")]
async fn saving_says_which_keys_were_stored_and_when_they_apply() -> Result<(), Error> {
    let fixture = start().await?;
    let fields = [
        ("name", "spare"),
        ("base_url", "https://spare.example.invalid"),
        ("api_key", API_KEY),
        ("signing_key", SIGNING_KEY),
    ];
    let added = fixture
        .submit("/setup/lab-apps/new", "/setup/lab-apps/new", &fields)
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    assert_new_look(&added);
    for part in [
        "<li><a href=\"/setup/lab-apps\">Lab apps</a></li><li><a href=\"/setup/lab-apps/spare\">spare</a></li>",
        "<h1>Lab app added</h1>",
        "<p class=\"alert__title\">spare is added</p><p class=\"alert__text\">Analyte uses these settings after it restarts.</p>",
        "<p class=\"text\">Address: https://spare.example.invalid</p>",
        "<p class=\"text\">Used by no machine yet: pick it on a machine’s setup page.</p>",
        "<p class=\"text\">Key stored: spare-api</p>",
        "<p class=\"text\">Key stored: spare-signing</p>",
        "<p class=\"text\">Waiting items go out after Analyte restarts with the keys.</p>",
        "<button type=\"submit\" class=\"btn btn--primary\">Restart now</button>",
        "<a class=\"btn btn--primary\" href=\"/setup/lab-apps/spare\">Open the lab app</a>",
    ] {
        assert!(added.contains(part), "{part}\n{added}");
    }
    // The lab apps file is new, so there is no old copy to keep.
    assert!(!added.contains("A dated copy") && !added.contains("Details for support"));
    let same = fixture
        .submit(
            "/setup/lab-apps/spare",
            "/setup/lab-apps/spare",
            &[("base_url", "https://spare.example.invalid")],
        )
        .await?;
    assert!(same.starts_with("HTTP/1.1 200"), "{same}");
    assert!(same.contains("<h1>Lab app saved</h1>"));
    assert!(same.contains(
        "<p class=\"text\">The settings were already the same; no file was written.</p>"
    ));
    assert!(!same.contains("Key stored") && !same.contains("Waiting items go out"));
    assert!(!same.contains("A dated copy"));
    let removed = fixture
        .submit(
            "/setup/lab-apps/spare/remove",
            "/setup/lab-apps/spare/remove",
            &[],
        )
        .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    for part in [
        "<h1>Lab app removed</h1>",
        "<p class=\"alert__title\">spare is no longer listed</p><p class=\"alert__text\">Its keys stay stored, locked.</p>",
        "<p class=\"text\">A dated copy of the old settings was kept.</p>",
        "Details for support</summary><pre>Copy of the old settings: ",
        "Restart now",
        "<a class=\"btn btn--primary\" href=\"/setup/lab-apps\">Back to Lab apps</a>",
    ] {
        assert!(removed.contains(part), "{part}\n{removed}");
    }
    assert!(
        !removed.contains("href=\"/setup/lab-apps/spare\""),
        "it has no page any more"
    );
    // The audit refuses: the lab app is added, and the page says the record is missing.
    fixture.refuse_audits()?;
    let unaudited = fixture
        .submit("/setup/lab-apps/new", "/setup/lab-apps/new", &fields)
        .await?;
    assert!(unaudited.starts_with("HTTP/1.1 200"), "{unaudited}");
    assert!(
        unaudited.contains(
            "<p class=\"alert__title\">The record of who made this change could not be written: "
        ),
        "{unaudited}"
    );
    assert!(unaudited.contains("Restart now"));
    for page in [&added, &same, &removed, &unaudited] {
        assert!(!page.contains(API_KEY) && !page.contains(SIGNING_KEY));
    }
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b3): lab app pages hold
// addresses and keys, so a request with the admin key alone gets "Sign in
// first" in the new look, and nothing is read or changed.
#[tokio::test(flavor = "multi_thread")]
async fn lab_app_pages_need_a_signed_in_session() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    write_scene(root)?;
    let bearer = crate::admin::rotate_token(root)?;
    let store = empty_store(root).await?;
    let (state, stopper, owner, host) =
        start_overview_server(root, &store, Vec::new(), Vec::new()).await?;
    let key = format!("Authorization: Bearer {bearer}\r\n");
    for path in [
        "/setup/lab-apps",
        "/setup/lab-apps/new",
        "/setup/lab-apps/up-lab",
        "/setup/lab-apps/up-lab/remove",
    ] {
        let page = request(&host, path, &key, None).await?;
        assert!(page.starts_with("HTTP/1.1 403"), "{path}: {page}");
        assert_new_look(&page);
        assert!(page.contains("<p class=\"alert__title\">Lab apps hold addresses and keys, so their pages need a signed-in browser session</p>"));
        assert!(!page.contains("up-lab.example.invalid"));
    }
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-20 (admin restyle b3), as for every restyled page:
// a slow store gets the error page in the new look after 1.5 s (504), and a
// store that cannot be read gets it at once (503), with the reason for support.
#[tokio::test(flavor = "multi_thread")]
async fn lab_apps_list_answers_store_failures_in_the_new_look() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    write_scene(root)?;
    let store = empty_store(root).await?;
    let ((state, stopper, owner, host), auth) = serve(root, &store, &[], &[]).await?;
    let blocker = hold_store(&store, PAST_READ_BUDGET).await?;
    let slow = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow}");
    assert_new_look(&slow);
    assert!(slow.contains("Analyte did not finish reading within 1.5 seconds. Reload the page;"));
    assert!(slow.contains("admin Lab apps deadline of 1500 ms exceeded"));
    blocker.await??;
    stop(state, stopper, owner, store).await?;
    let mut closed = Arc::try_unwrap(empty_store(root).await?).map_err(|_| "shared")?;
    closed.shutdown(fixture_deadline()?).await?;
    let store = Arc::new(closed);
    let ((state, stopper, owner, host), auth) = serve(root, &store, &[], &[]).await?;
    let failed = request(&host, "/setup/lab-apps", &auth, None).await?;
    assert!(failed.starts_with("HTTP/1.1 503"), "{failed}");
    assert_new_look(&failed);
    assert!(
        failed.contains(
            "<p class=\"alert__title\">Analyte could not read its stored data just now</p>"
        )
    );
    assert!(failed.contains("Details for support</summary><pre>"));
    drop(state);
    stopper.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    Ok(())
}
