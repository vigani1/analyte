//! Tests signing in and out through the real router, and the refusals shown in the plain frame: what each says, its status, and that none names a machine or lab app.
use super::overview::{empty_store, stop};
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
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
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
        state.sessions.issue(true, None)?;
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
