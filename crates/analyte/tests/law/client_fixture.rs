//! Starts a LAW TCP client against an injected loopback analyzer listener.
use super::*;

pub(super) async fn client(
    settings: &str,
) -> Result<
    (
        tempfile::TempDir,
        Fixture,
        Reference,
        Running,
        TcpStream,
        TcpListener,
    ),
    Error,
> {
    client_with_setup(settings, |_| Ok(())).await
}
pub(super) async fn client_with_setup(
    settings: &str,
    configure: impl FnOnce(&mut Program) -> Result<(), Error>,
) -> Result<
    (
        tempfile::TempDir,
        Fixture,
        Reference,
        Running,
        TcpStream,
        TcpListener,
    ),
    Error,
> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "law",
        &settings.rewrite("kind = \"tcp_server\"", "kind = \"tcp_client\""),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let loaded = Program::load(&path)?;
    let old = loaded
        .machines()
        .first()
        .ok_or("machine")?
        .snapshot()
        .effective()
        .site()
        .endpoint
        .fixed()?;
    let site = temp.path().join("hl7-site.toml");
    std::fs::write(
        &site,
        std::fs::read_to_string(&site)?.rewrite(old, &listener.local_addr()?.to_string()),
    )?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![order("order-1", "000Ab12", Some("law"), 2)?])
        .await?;
    let mut loaded = Program::load(&path)?;
    loaded.set_loopback_destination_for_test("demo", reference.url.clone())?;
    configure(&mut loaded)?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let (peer, _) = tokio::time::timeout(HANG_GUARD, listener.accept())
        .await
        .map_err(|e| format!("reconnect after disconnect: {e}"))??;
    wait_feed(&path).await?;
    Ok((temp, path, reference, running, peer, listener))
}

/// Waits for custody of this frame and its linked parking reason, not other receipts.
pub(super) async fn wait_parked_frame(path: &Path, raw: &[u8], reason: &str) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM receipts r JOIN parked p ON json_extract(r.id,'$')=json_extract(p.evidence,'$.targets[0]') WHERE r.raw=?1 AND json_extract(p.evidence,'$.reason')=?2",
        (raw, reason),
        1,
    ).await
}
