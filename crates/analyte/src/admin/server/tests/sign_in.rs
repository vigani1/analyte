//! Tests signing in and out through the real router, and the refusals shown in the plain frame: what each says, its status, and that none names a machine or lab app.
use super::overview::{assert_recorded, confirm_needs_csrf, empty_store, review_items, stop};
use super::pages::{cookie, csrf, encode, login};
use super::*;

// Owner request 2026-11-19 (admin restyle, sign out) and plan §12: sign-out is a
// POST with the session's CSRF token, GET is not allowed, and afterwards the old
// cookie no longer opens any page or form.
#[tokio::test(flavor = "multi_thread")]
async fn sign_out_needs_csrf_and_ends_the_session() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = Arc::new(
        analyte_store::Handle::start(
            directory.path().join("analyte.db"),
            analyte_fixtures::store_limits(),
            Box::new(Space),
            4,
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("logout fixture migration: {error}"))?,
    );
    let (state, stop, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let cookie = login(&host, &bearer).await?;
    let token = state
        .sessions
        .lookup(cookie.strip_prefix("analyte_session="))?
        .ok_or("session missing")?
        .csrf
        .form_value()
        .to_owned();
    let auth = format!("Cookie: {cookie}\r\n");
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    for refused in [String::new(), "csrf=wrong".to_owned()] {
        let response = request(&host, "/logout", &post, Some(&refused)).await?;
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        headers_present(&response);
    }
    let still = request(&host, "/", &auth, None).await?;
    assert!(
        still.starts_with("HTTP/1.1 200"),
        "refused sign-out kept the session"
    );
    let get = request(&host, "/logout", &auth, None).await?;
    assert!(get.starts_with("HTTP/1.1 405"), "{get}");
    let form = format!("csrf={}", encode(&token));
    let done = request(&host, "/logout", &post, Some(&form)).await?;
    assert!(done.starts_with("HTTP/1.1 303"), "{done}");
    let lower = done.to_ascii_lowercase();
    assert!(lower.contains("location: /login\r\n"), "{done}");
    assert!(
        lower
            .contains("set-cookie: analyte_session=; path=/; max-age=0; httponly; samesite=strict"),
        "{done}"
    );
    headers_present(&done);
    let after = request(&host, "/", &auth, None).await?;
    assert!(after.starts_with("HTTP/1.1 401"), "{after}");
    let again = request(&host, "/logout", &post, Some(&form)).await?;
    assert!(again.starts_with("HTTP/1.1 401"), "{again}");
    assert!(
        state
            .sessions
            .lookup(cookie.strip_prefix("analyte_session="))?
            .is_none()
    );
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retained store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}

/// Machine and lab app names a page shown before sign-in must not hold.
const SECRET_NAMES: [&str; 2] = ["chemistry-analyzer-b6", "main-lab-b6"];

/// A server with one machine sending to one lab app, so a page can be checked
/// for naming them; its store, its parts and the admin key.
async fn started(
    root: &std::path::Path,
) -> Result<(Arc<analyte_store::Handle>, Started, String), Error> {
    let bearer = crate::admin::rotate_token(root)?;
    let store = empty_store(root).await?;
    let link =
        crate::machine::link::status::Reporter::new(crate::machine::link::status::Link::Serial);
    let machines = vec![machine(
        SECRET_NAMES[0],
        true,
        link.subscribe(),
        SECRET_NAMES[1],
    )?];
    let started = start_overview_server(root, &store, machines, Vec::new()).await?;
    Ok((store, started, bearer))
}

/// The plain frame (the `SignIn` mockup): one centred panel, no sidebar, no
/// count and no machine or lab app name.
fn assert_plain(page: &str) {
    assert!(page.contains("<main class=\"center\">"), "{page}");
    assert!(!page.contains("class=\"sidebar\""), "{page}");
    assert!(!page.contains("nav__count"), "{page}");
    for name in SECRET_NAMES {
        assert!(!page.contains(name), "{name} shown: {page}");
    }
}

/// A refusal page: its status, its title, the link onward and the exact
/// reason under "Details for support".
fn assert_refusal(page: &str, status: u16, title: &str, link: Option<(&str, &str)>, detail: &str) {
    assert!(page.starts_with(&format!("HTTP/1.1 {status}")), "{page}");
    headers_present(page);
    assert_plain(page);
    assert!(
        page.contains(&format!(
            "<h1 id=\"rf-title\" class=\"panel__title\">{title}</h1>"
        )),
        "{title}: {page}"
    );
    assert!(
        page.contains(&format!("<title>{title} · Analyte</title>")),
        "{page}"
    );
    match link {
        Some((href, label)) => assert!(
            page.contains(&format!(
                "<a class=\"btn btn--primary btn--lg panel__action\" href=\"{href}\">{label}</a>"
            )),
            "{page}"
        ),
        None => assert!(!page.contains("panel__action"), "{page}"),
    }
    assert!(
        page.contains("Details for support</summary><pre>"),
        "{page}"
    );
    assert!(page.contains(detail), "{detail}: {page}");
}

// Owner request 2026-11-20 (admin restyle b6, SignIn mockup) and plan §12: sign
// in is the plain frame with the admin key's field, the session's form token,
// its cookie, 401 and the Bearer challenge. A refused try says why under the
// field and never shows the key typed; a form sent without a signed-in session
// says nothing was changed; the right key still signs in.
#[tokio::test(flavor = "multi_thread")]
async fn sign_in_says_why_a_try_was_refused_and_never_shows_the_key() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), bearer) = started(directory.path()).await?;
    let page = request(&host, "/login", "", None).await?;
    assert!(page.starts_with("HTTP/1.1 401"), "{page}");
    headers_present(&page);
    assert_plain(&page);
    let lower = page.to_ascii_lowercase();
    assert!(lower.contains("www-authenticate: bearer\r\n"), "{page}");
    assert!(lower.contains("set-cookie: analyte_session="), "{page}");
    for expected in [
        "<title>Sign in · Analyte</title>",
        "<form class=\"panel anim-in\" aria-labelledby=\"si-title\" method=\"post\" action=\"/login\">",
        "<input type=\"hidden\" name=\"csrf\" value=\"",
        "<h1 id=\"si-title\" class=\"panel__title\">Sign in to Analyte</h1>",
        "<label class=\"field__label\" for=\"si-key\">Admin key</label>",
        concat!(
            "<input id=\"si-key\" class=\"input\" type=\"password\" name=\"token\" ",
            "required autocomplete=\"off\" maxlength=\"44\" autofocus>"
        ),
        "<button type=\"submit\" class=\"btn btn--primary btn--lg panel__action\">Sign in</button>",
        "The key is in the file <span class=\"mono\">admin-token</span> in Analyte’s data folder on this PC.",
        "Engine running · it keeps running when you sign out",
    ] {
        assert!(page.contains(expected), "{expected}: {page}");
    }
    assert!(
        !page.contains("field__error") && !page.contains("alert"),
        "{page}"
    );
    // A page opened before sign-in is the same sign-in page, and names nothing.
    let machines = request(&host, "/machines", "", None).await?;
    assert!(machines.starts_with("HTTP/1.1 401"), "{machines}");
    assert_plain(&machines);
    assert!(machines.contains("Sign in to Analyte"));
    let challenge = cookie(&page)?;
    let token = csrf(&page)?;
    let post =
        format!("Cookie: {challenge}\r\nContent-Type: application/x-www-form-urlencoded\r\n");
    let typed = "typed-wrong-key-b6";
    let wrong = request(
        &host,
        "/login",
        &post,
        Some(&format!("csrf={}&token={typed}", encode(&token))),
    )
    .await?;
    assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
    headers_present(&wrong);
    assert_plain(&wrong);
    assert!(wrong.contains(concat!(
        "autofocus aria-invalid=\"true\" aria-describedby=\"si-error\">",
        "<span id=\"si-error\" class=\"field__error\">"
    )));
    assert!(wrong.contains("This is not the admin key. Check it and try again.</span>"));
    assert!(!wrong.contains(typed), "the typed key is shown");
    assert!(!wrong.contains(&bearer), "the admin key is shown");
    // The same session's form token still works for the next try.
    assert_eq!(csrf(&wrong)?, token);
    let unreadable = request(
        &host,
        "/login",
        &post,
        Some(&format!("csrf={}&token={typed}&extra=1", encode(&token))),
    )
    .await?;
    assert!(unreadable.starts_with("HTTP/1.1 401"), "{unreadable}");
    assert!(unreadable.contains("Analyte could not read the form. Enter the key again.</span>"));
    assert!(!unreadable.contains(typed));
    // A form sent with a session that is not signed in is not acted on.
    let refused = request(
        &host,
        "/parked/1/resolve",
        &post,
        Some(&format!("csrf={}&confirm=yes&reason=x", encode(&token))),
    )
    .await?;
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
    assert_plain(&refused);
    assert!(refused.contains("<div class=\"alert alert--warn\" role=\"note\">"));
    assert!(refused.contains(concat!(
        "<p class=\"alert__title\">Nothing was changed</p><p class=\"alert__text\">",
        "The form was sent without a signed-in session. Sign in, then open the page again ",
        "and repeat what you did.</p>"
    )));
    let signed = request(
        &host,
        "/login",
        &post,
        Some(&format!(
            "csrf={}&token={}",
            encode(&token),
            encode(&bearer)
        )),
    )
    .await?;
    assert!(signed.starts_with("HTTP/1.1 303"), "{signed}");
    assert!(signed.to_ascii_lowercase().contains("location: /\r\n"));
    headers_present(&signed);
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-20 (admin restyle b6): with every browser session in
// use, sign in answers "You cannot sign in right now" in the plain frame, still
// as the 401 challenge, with a way to try again.
#[tokio::test(flavor = "multi_thread")]
async fn sign_in_says_when_no_session_is_left() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), _) = started(directory.path()).await?;
    for _ in 0..128 {
        state
            .sessions
            .issue(sessions::SignedIn::admin_key()?, None)?;
    }
    let page = request(&host, "/login", "", None).await?;
    assert_refusal(
        &page,
        401,
        "You cannot sign in right now",
        Some(("/login", "Try again")),
        "admin session capacity reached; restart or wait for expiry",
    );
    assert!(page.contains("Too many browser sessions are open."));
    assert!(
        page.to_ascii_lowercase()
            .contains("www-authenticate: bearer\r\n")
    );
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-20 (admin restyle b6): every refusal without a page of
// its own is the plain frame with its unchanged status, what it means, a link
// onward and the exact reason: an address with no page (404) or no file (404),
// a page opened the wrong way (405), an address or query that could not be
// read (400, the framework's words kept), an expired form (403) and too much
// text (413). Before sign-in an unknown address still asks to sign in.
#[tokio::test(flavor = "multi_thread")]
async fn refusals_use_the_plain_frame_and_keep_their_status() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), bearer) = started(directory.path()).await?;
    let unsigned = request(&host, "/missing", "", None).await?;
    assert!(unsigned.starts_with("HTTP/1.1 401"), "{unsigned}");
    assert!(unsigned.contains("Sign in to Analyte"));
    let overview = Some(("/", "Go to Overview"));
    let auth = format!("Authorization: Bearer {bearer}\r\n");
    let missing = request(&host, "/missing", &auth, None).await?;
    assert_refusal(
        &missing,
        404,
        "Page not found",
        overview,
        "no admin page at this address",
    );
    let file = request(&host, "/static/nothing.css", "", None).await?;
    assert_refusal(
        &file,
        404,
        "File not found",
        overview,
        "static asset not found",
    );
    let cookie = login(&host, &bearer).await?;
    let signed = format!("Cookie: {cookie}\r\n");
    let method = request(&host, "/logout", &signed, None).await?;
    assert_refusal(
        &method,
        405,
        "This cannot be done here",
        overview,
        "GET is not allowed at this address",
    );
    assert!(
        method.to_ascii_lowercase().contains("allow: post\r\n"),
        "{method}"
    );
    for (path, words) in [
        ("/parked/abc", "Cannot parse"),
        ("/orders?after=-1", "Failed to deserialize query string"),
    ] {
        let page = request(&host, path, &signed, None).await?;
        assert_refusal(&page, 400, "This could not be read", overview, words);
        assert!(page.contains("Nothing was changed. Go back and try again."));
    }
    let post = format!("{signed}Content-Type: application/x-www-form-urlencoded\r\n");
    let expired = request(
        &host,
        "/parked/1/resolve",
        &post,
        Some("csrf=wrong&confirm=yes&reason=x"),
    )
    .await?;
    assert_refusal(
        &expired,
        403,
        "This form has expired",
        overview,
        "CSRF token invalid or missing",
    );
    assert!(expired.contains("Go back, reload the page and try again. Nothing was changed."));
    let long = format!("csrf=x&reason={}", "a".repeat(2048));
    let large = request(&host, "/parked/1/resolve", &post, Some(&long)).await?;
    assert_refusal(
        &large,
        413,
        "Too much text",
        overview,
        "admin form exceeds limit",
    );
    assert!(!large.contains(&"a".repeat(64)), "the text is echoed");
    stop(state, stopper, owner, store).await
}

/// What the Analyte app's pages view sends: any browser's words, then the mark.
const APP: &str = "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/131.0.0.0 AnalyteApp\r\n";
/// What a browser sends.
const BROWSER: &str =
    "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/131.0.0.0\r\n";
/// The SID of the test account.
const SID: &str = "S-1-5-21-1-2-3-1001";

/// A Windows account as the pipe verified it, with or without its name.
fn account(name: Option<&str>) -> Result<windows::WindowsAccount, Error> {
    Ok(windows::WindowsAccount::new(
        windows::Sid::new(SID.into())?,
        name.map(|name| windows::AccountName::new(name.into()))
            .transpose()?,
    )?)
}

/// Hands out a code for `account` and `next` as the pipe does; its text.
fn code(state: &Context, account: windows::WindowsAccount, next: &str) -> Result<String, Error> {
    let code = state
        .windows
        .codes
        .keep(account, windows::line::Next::new(next))?;
    Ok(code.text().to_owned())
}

/// The refresh the challenge to the app carries, back to `next` encoded.
fn refresh(next: &str) -> String {
    format!("<meta http-equiv=\"refresh\" content=\"0;url=/login/windows?next={next}\">")
}

/// The key form with an alert above it, as a 401 challenge.
fn assert_key_form_with(page: &str, title: &str, text: &str) {
    assert!(page.starts_with("HTTP/1.1 401"), "{page}");
    headers_present(page);
    assert_plain(page);
    assert!(page.contains("<label class=\"field__label\" for=\"si-key\">Admin key</label>"));
    assert!(
        page.contains(&format!(
            "<p class=\"alert__title\">{title}</p><p class=\"alert__text\">{text}</p>"
        )),
        "{title}: {page}"
    );
    assert!(!page.contains("http-equiv=\"refresh\""), "{page}");
}

// ADR 0021 §4 and the design study's HTTP tests: a fresh code starts a Windows
// session, sets the cookie and goes to the code's page, not stored; the sidebar
// names the account. The same code again, a wrong one and an expired one get
// the key form with one fixed sentence, as a 401.
#[tokio::test(flavor = "multi_thread")]
async fn windows_code_signs_in_once_and_the_sidebar_names_the_account() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), _) = started(directory.path()).await?;
    let used = code(&state, account(Some(r"LAB-PC\maria"))?, "/parked/12")?;
    let path = format!("/login/windows?code={}", encode(&used));
    let signed = request(&host, &path, APP, None).await?;
    assert!(signed.starts_with("HTTP/1.1 303"), "{signed}");
    headers_present(&signed);
    let lower = signed.to_ascii_lowercase();
    assert!(lower.contains("location: /parked/12\r\n"), "{signed}");
    assert!(lower.contains("set-cookie: analyte_session="), "{signed}");
    let session = cookie(&signed)?;
    let signed_in = state
        .sessions
        .lookup(session.strip_prefix("analyte_session="))?
        .ok_or("session missing")?;
    assert_eq!(
        signed_in.actor().map(Identifier::as_str),
        Some(r"windows:S-1-5-21-1-2-3-1001:LAB-PC\maria")
    );
    let page = request(&host, "/", &format!("Cookie: {session}\r\n"), None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains(concat!(
        "<span class=\"sidebar__account\">Signed in as <span class=\"mono\">LAB-PC\\maria</span></span>\n",
        "<form class=\"sidebar__signout\" method=\"post\" action=\"/logout\">"
    )));
    let spent = "A Windows sign-in works once, within 20 seconds. Open the Analyte app again, or sign in with the admin key.";
    let again = request(&host, &path, APP, None).await?;
    assert_key_form_with(&again, "This sign-in has expired", spent);
    for wrong in [String::new(), "A".repeat(44), "%3Cb%3E".into()] {
        let page = request(&host, &format!("/login/windows?code={wrong}"), APP, None).await?;
        assert_key_form_with(&page, "This sign-in has expired", spent);
        assert!(!page.contains("&#60;b&#62;"), "the code is echoed");
    }
    let old = code(&state, account(None)?, "/")?;
    state.windows.codes.age(Duration::from_secs(20))?;
    let expired = request(
        &host,
        &format!("/login/windows?code={}", encode(&old)),
        APP,
        None,
    )
    .await?;
    assert_key_form_with(&expired, "This sign-in has expired", spent);
    stop(state, stopper, owner, store).await
}

// ADR 0021 §5: Windows gave no name, so the sidebar shows the SID, verified
// and never a guess.
#[tokio::test(flavor = "multi_thread")]
async fn sidebar_shows_the_sid_without_a_name() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), _) = started(directory.path()).await?;
    let text = code(&state, account(None)?, "/settings")?;
    let signed = request(
        &host,
        &format!("/login/windows?code={}", encode(&text)),
        APP,
        None,
    )
    .await?;
    assert!(
        signed
            .to_ascii_lowercase()
            .contains("location: /settings\r\n")
    );
    let page = request(
        &host,
        "/settings",
        &format!("Cookie: {}\r\n", cookie(&signed)?),
        None,
    )
    .await?;
    assert!(page.contains(&format!(
        "<span class=\"sidebar__account\">Signed in as <span class=\"mono\">{SID}</span></span>"
    )));
    stop(state, stopper, owner, store).await
}

// ADR 0021 §6 and the design study (§3 Sessions): a browser at
// /login/windows gets the key form saying Windows sign-in works in the app,
// without a refresh, so a faked app mark cannot loop; /login links to it.
#[tokio::test(flavor = "multi_thread")]
async fn browser_at_windows_sign_in_gets_the_key_form() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), _) = started(directory.path()).await?;
    for agent in [BROWSER, APP] {
        let page = request(&host, "/login/windows?next=%2Fparked", agent, None).await?;
        assert!(page.starts_with("HTTP/1.1 401"), "{page}");
        headers_present(&page);
        assert_plain(&page);
        assert!(page.contains("<div class=\"alert alert--info\" role=\"note\">"));
        assert!(page.contains(concat!(
            "<p class=\"alert__title\">Windows sign-in works in the Analyte app</p>",
            "<p class=\"alert__text\">In a browser, sign in with the admin key.</p>"
        )));
        assert!(page.contains("for=\"si-key\">Admin key</label>"));
        assert!(!page.contains("http-equiv=\"refresh\""), "{page}");
        assert!(!page.contains("Sign in with Windows"), "{page}");
    }
    let login = request(&host, "/login", BROWSER, None).await?;
    assert!(login.contains(
        "<a class=\"btn btn--outline btn--lg panel__action\" href=\"/login/windows\">Sign in with Windows</a>"
    ));
    stop(state, stopper, owner, store).await
}

// ADR 0021 §6 and the design study (§3 Refusal): the word the app sends after
// a refused Windows sign-in shows one fixed sentence above the key form; any
// other text shows nothing and is never echoed.
#[tokio::test(flavor = "multi_thread")]
async fn refusal_words_show_fixed_sentences() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), _) = started(directory.path()).await?;
    let title = "Windows sign-in did not work";
    for (word, text) in [
        (
            "not-allowed",
            "Your Windows account is not allowed to use Analyte. Ask an administrator to add it to the Analyte users group, or sign in with the admin key.",
        ),
        (
            "bad-request",
            "The Analyte app asked in a way this engine does not understand. Sign in with the admin key, and ask IT to update the app.",
        ),
        (
            "unidentified",
            "Windows could not say which account is asking. Sign in with the admin key.",
        ),
        (
            "unavailable",
            "Windows sign-in is not available right now. Try again in a moment, or sign in with the admin key.",
        ),
    ] {
        let page = request(&host, &format!("/login?windows={word}"), APP, None).await?;
        assert_key_form_with(&page, title, text);
        assert!(page.contains("<div class=\"alert alert--warn\" role=\"note\">"));
    }
    for other in [
        "%3Cscript%3Ehostile%3C%2Fscript%3E",
        "Not-Allowed",
        "",
        "%ZZ",
    ] {
        let page = request(&host, &format!("/login?windows={other}"), APP, None).await?;
        assert!(page.starts_with("HTTP/1.1 401"), "{page}");
        assert!(!page.contains("alert__title"), "{other}: {page}");
        assert!(!page.contains("hostile"), "the address is echoed");
        assert!(!page.contains("http-equiv=\"refresh\""), "{page}");
    }
    stop(state, stopper, owner, store).await
}

// ADR 0021 §6: a challenge to the app (its user agent ends in AnalyteApp)
// refreshes to its Windows sign-in with the page to return to; a browser's
// does not, sign-in pages never do (so sign-out stays signed out), a form is
// not followed, and a return path that is not a local page becomes "/". After
// a restart the old cookie is unknown and the next challenge refreshes again.
#[tokio::test(flavor = "multi_thread")]
async fn challenge_refreshes_only_the_app_and_never_on_sign_in() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), bearer) = started(directory.path()).await?;
    for (path, next) in [
        ("/machines", "%2Fmachines"),
        (
            "/parked?after=5&show=resolved",
            "%2Fparked%3Fafter%3D5%26show%3Dresolved",
        ),
        ("//evil.example/", "%2F"),
    ] {
        let page = request(&host, path, APP, None).await?;
        assert!(page.starts_with("HTTP/1.1 401"), "{page}");
        headers_present(&page);
        assert!(page.contains(&refresh(next)), "{path}: {page}");
        let browser = request(&host, path, BROWSER, None).await?;
        assert!(!browser.contains("http-equiv=\"refresh\""), "{browser}");
    }
    let forgotten =
        format!("{APP}Cookie: analyte_session=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\r\n");
    let restarted = request(&host, "/activity", &forgotten, None).await?;
    assert!(restarted.contains(&refresh("%2Factivity")), "{restarted}");
    let marked_elsewhere = "User-Agent: AnalyteApp/2 Mozilla/5.0\r\n";
    let page = request(&host, "/machines", marked_elsewhere, None).await?;
    assert!(!page.contains("http-equiv=\"refresh\""), "{page}");
    let cookie = login(&host, &bearer).await?;
    let token = state
        .sessions
        .lookup(cookie.strip_prefix("analyte_session="))?
        .ok_or("session missing")?
        .csrf
        .form_value()
        .to_owned();
    let post =
        format!("{APP}Cookie: {cookie}\r\nContent-Type: application/x-www-form-urlencoded\r\n");
    let out = request(
        &host,
        "/logout",
        &post,
        Some(&format!("csrf={}", encode(&token))),
    )
    .await?;
    assert!(
        out.to_ascii_lowercase().contains("location: /login\r\n"),
        "{out}"
    );
    for path in ["/login", "/login?windows=not-allowed", "/login/windows"] {
        let page = request(&host, path, &format!("{APP}Cookie: {cookie}\r\n"), None).await?;
        assert!(page.starts_with("HTTP/1.1 401"), "{page}");
        assert!(!page.contains("http-equiv=\"refresh\""), "{path}: {page}");
    }
    let form = request(
        &host,
        "/parked/1/resolve",
        &format!("{APP}Content-Type: application/x-www-form-urlencoded\r\n"),
        Some("csrf=x&confirm=yes&reason=x"),
    )
    .await?;
    assert!(form.starts_with("HTTP/1.1 401"), "{form}");
    assert!(form.contains("<p class=\"alert__title\">Nothing was changed</p>"));
    assert!(!form.contains("http-equiv=\"refresh\""), "{form}");
    stop(state, stopper, owner, store).await
}

// ADR 0021 §5 and the design study's HTTP tests: a Windows session's Mark
// resolved records windows:<SID>:<DOMAIN>\<name> as the actor, and the
// history shows the account's name.
#[tokio::test(flavor = "multi_thread")]
async fn windows_session_is_the_audit_actor_and_the_history_names_it() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let items: Vec<analyte_store::Parked> =
        review_items("unused", &uuid::Uuid::now_v7().to_string())?
            .into_iter()
            .skip(5)
            .take(1)
            .collect();
    let ids = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            items
                .iter()
                .map(|item| s.park(item).map_err(FixtureError::from))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
    let id = *ids.first().ok_or("nothing parked")?;
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let text = code(&state, account(Some(r"LAB-PC\maria"))?, "/")?;
    let signed = request(
        &host,
        &format!("/login/windows?code={}", encode(&text)),
        APP,
        None,
    )
    .await?;
    let session = cookie(&signed)?;
    let auth = format!("Cookie: {session}\r\n");
    let path = format!("/parked/{id}/resolve");
    let form = confirm_needs_csrf(&host, &auth, &path).await?;
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let done = request(&host, &path, &post, Some(&form)).await?;
    assert!(done.starts_with("HTTP/1.1 200"), "{done}");
    assert_recorded(&store, &state, &session, id).await?;
    let recorded = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.audits(0)?))
        .await?;
    assert_eq!(
        recorded.first().map(|(_, audit)| audit.actor.as_str()),
        Some(r"windows:S-1-5-21-1-2-3-1001:LAB-PC\maria")
    );
    let detail = request(&host, &format!("/parked/{id}"), &auth, None).await?;
    assert!(
        detail.contains(concat!(
            "<span class=\"timeline__text\">Marked resolved<span class=\"timeline__who\">",
            "LAB-PC\\maria · “settled with the lab app”</span>"
        )),
        "{detail}"
    );
    stop(state, stopper, owner, store).await
}

// ADR 0021 §1 and §3: Settings says whether Windows sign-in is on, and why
// not: the pipe's name was taken, or the Analyte users group is missing.
#[tokio::test(flavor = "multi_thread")]
async fn settings_say_whether_windows_sign_in_is_on() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), bearer) = started(directory.path()).await?;
    let auth = format!("Authorization: Bearer {bearer}\r\n");
    for (status, line) in [
        (windows::Status::On, "On".to_owned()),
        (
            windows::Status::Off(r"another program already holds the sign-in pipe \\.\pipe\analyte-sign-in-4000".into()),
            r"Off because another program already holds the sign-in pipe \\.\pipe\analyte-sign-in-4000".to_owned(),
        ),
        (
            windows::Status::AdministratorsOnly,
            "Off for other users because the Analyte users group is missing, so only administrators can sign in with Windows".to_owned(),
        ),
    ] {
        state.windows.set(status);
        let page = request(&host, "/settings", &auth, None).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        assert!(
            page.contains(&format!("<dt>Windows sign-in</dt><dd>{line}</dd>")),
            "{line}: {page}"
        );
    }
    stop(state, stopper, owner, store).await
}
