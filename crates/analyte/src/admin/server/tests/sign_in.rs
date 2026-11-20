//! Tests signing out through the real router: CSRF-checked, POST only, and the session ends.
use super::pages::{encode, login};
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
