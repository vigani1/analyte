//! Tests the static file route: every listed file, its media type, no login and the cache rule.
use super::*;

// ADR 0018: every file of the table is served without a session, with its media
// type and the usual security headers. Under this build's version a browser may
// keep it for a year; asked for without it, it is not stored.
// An unknown file is refused and the host check still applies.
#[tokio::test(flavor = "multi_thread")]
async fn every_static_file_is_public_typed_and_cached_by_version() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    crate::admin::rotate_token(directory.path())?;
    let store = Arc::new(
        analyte_store::Handle::start(
            directory.path().join("analyte.db"),
            analyte_fixtures::store_limits(),
            Box::new(Space),
            4,
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("asset fixture migration: {error}"))?,
    );
    let (state, stop, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let root = crate::admin::view::assets::Root::of_this_build();
    let listed = crate::admin::view::assets::tests::listed();
    assert_eq!(listed.len(), 25);
    for (name, kind, bytes) in listed {
        for (path, kept) in [
            (format!("{}{name}", root.as_str()), true),
            (format!("/static/{name}"), false),
        ] {
            let (response, body) = fetch(&host, &path).await?;
            assert!(response.starts_with("HTTP/1.1 200"), "{path}: {response}");
            assert_eq!(body, bytes, "{path} serves other bytes");
            let lower = response.to_ascii_lowercase();
            assert!(
                lower.contains(&format!("content-type: {kind}\r\n")),
                "{path}"
            );
            assert!(lower.contains("x-content-type-options: nosniff"), "{path}");
            assert!(
                lower.contains("content-security-policy: default-src 'self'"),
                "{path}"
            );
            let cache = if kept {
                "cache-control: public, max-age=31536000, immutable\r\n"
            } else {
                "cache-control: no-store\r\n"
            };
            assert!(lower.contains(cache), "{path}: {response}");
        }
    }
    for missing in [
        "/static/nothing.css",
        "/static/../Cargo.toml",
        "/static/0000/app.css",
    ] {
        let response = request(&host, missing, "", None).await?;
        assert!(
            response.starts_with("HTTP/1.1 404"),
            "{missing}: {response}"
        );
    }
    let mut stream = tokio::net::TcpStream::connect(&host).await?;
    stream
        .write_all(
            format!(
                "GET {}analyte.css HTTP/1.1\r\nHost: attacker.invalid\r\nConnection: close\r\n\r\n",
                root.as_str()
            )
            .as_bytes(),
        )
        .await?;
    let mut refused = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(6),
        stream.take(1_048_576).read_to_end(&mut refused),
    )
    .await??;
    assert!(String::from_utf8(refused)?.starts_with("HTTP/1.1 401"));
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retained store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}

/// GET `path` without credentials; the head as text and the body as exact bytes.
async fn fetch(host: &str, path: &str) -> Result<(String, Vec<u8>), Error> {
    let mut stream = tokio::net::TcpStream::connect(host).await?;
    let message = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(message.as_bytes()).await?;
    let mut bytes = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(6),
        stream.take(1_048_576).read_to_end(&mut bytes),
    )
    .await??;
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| format!("{path}: response without a head"))?;
    let (head, body) = bytes.split_at(end);
    Ok((
        String::from_utf8(head.to_vec())?,
        body.get(4..).unwrap_or_default().to_vec(),
    ))
}
