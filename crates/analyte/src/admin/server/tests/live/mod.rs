//! Tests the live stream (PR c2) through the real router and listener: signed-in only, an event after a stored change, the heartbeat, its lifetime, the cap that leaves pages answering, sign-out and shutdown.
use super::overview::{empty_store, stop};
use super::pages::{csrf, encode, login};
use super::*;
use crate::admin::live::Bounds;
mod script;

/// A running admin server over an empty store with the live streams' bounds
/// given, and a signed-in session.
struct Site {
    _directory: tempfile::TempDir,
    store: Arc<analyte_store::Handle>,
    started: Started,
    bearer: String,
    cookie: String,
}

/// Stream bounds in milliseconds, and the cap.
fn bounds(lifetime: u64, heartbeat: u64, check: u64, streams: usize) -> Bounds {
    Bounds {
        lifetime: Duration::from_millis(lifetime),
        heartbeat: Duration::from_millis(heartbeat),
        check: Duration::from_millis(check),
        streams: Arc::new(Semaphore::new(streams)),
    }
}

impl Site {
    async fn start(bounds: Bounds) -> Result<Self, Error> {
        let directory = crate::admin::private_tempdir()?;
        let root = directory.path();
        let bearer = crate::admin::rotate_token(root)?;
        let store = empty_store(root).await?;
        let started = start_bounded(
            root,
            &store,
            (
                (Vec::new(), Vec::new()),
                (crate::admin::Draining::new(), root),
                (
                    Vec::new(),
                    crate::admin::pages::Installed::for_test(Vec::new()),
                ),
            ),
            bounds,
        )
        .await?;
        let cookie = login(&started.3, &bearer).await?;
        Ok(Self {
            _directory: directory,
            store,
            started,
            bearer,
            cookie,
        })
    }

    fn auth(&self) -> String {
        format!("Cookie: {}\r\n", self.cookie)
    }

    /// Opens GET /events with `auth` and returns the connection.
    async fn open(&self, auth: &str) -> Result<tokio::net::TcpStream, Error> {
        let host = &self.started.3;
        let mut stream = tokio::net::TcpStream::connect(host).await?;
        stream
            .write_all(
                format!("GET /events HTTP/1.1\r\nHost: {host}\r\nAccept: text/event-stream\r\n{auth}\r\n")
                    .as_bytes(),
            )
            .await?;
        Ok(stream)
    }

    async fn stop(self) -> Result<(), Error> {
        let (state, stopper, owner, _) = self.started;
        stop(state, stopper, owner, self.store).await
    }
}

/// Reads `stream` into `seen` until it holds `count` copies of `text`;
/// fails after `wait` or when the stream ends first.
async fn read_until(
    stream: &mut tokio::net::TcpStream,
    seen: &mut String,
    text: &str,
    count: usize,
    wait: Duration,
) -> Result<(), Error> {
    let deadline = tokio::time::Instant::now()
        .checked_add(wait)
        .ok_or("wait past the clock")?;
    while seen.matches(text).count() < count {
        let mut buffer = [0_u8; 4096];
        let read = tokio::time::timeout_at(deadline, stream.read(&mut buffer))
            .await
            .map_err(|_| format!("waited for {count} × {text:?}; read {seen:?}"))??;
        if read == 0 {
            return Err(format!("the stream ended before {text:?}: {seen:?}").into());
        }
        seen.push_str(&String::from_utf8_lossy(
            buffer.get(..read).unwrap_or_default(),
        ));
    }
    Ok(())
}

/// The end of a chunked body: the stream ended.
const ENDED: &str = "\r\n0\r\n\r\n";

/// An audited change to store, which Activity lists.
fn audit() -> Result<analyte_store::Audit, Error> {
    Ok(analyte_store::Audit {
        actor: Identifier::new("admin-session-1")?,
        action: Identifier::new("setup-save")?,
        targets: vec![Identifier::new("synthetic")?],
        time: 1,
        reason: Reason::new("synthetic")?,
    })
}

// Owner decisions 2026-11-20 (PR c2): the stream is for signed-in pages only;
// it answers at once, then sends an event naming the areas a stored change
// touched, never what changed.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_change_reaches_a_signed_in_stream() -> Result<(), Error> {
    let site = Site::start(bounds(60_000, 60_000, 100, 4)).await?;
    let host = site.started.3.clone();
    let refused = request(&host, "/events", "", None).await?;
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
    let forged = request(&host, "/events", "Cookie: analyte_session=forged\r\n", None).await?;
    assert!(forged.starts_with("HTTP/1.1 401"), "{forged}");
    let mut stream = site.open(&site.auth()).await?;
    let mut seen = String::new();
    read_until(
        &mut stream,
        &mut seen,
        "\"changed\":[]",
        1,
        Duration::from_secs(3),
    )
    .await?;
    let head = seen.to_ascii_lowercase();
    assert!(head.starts_with("http/1.1 200"), "{seen}");
    assert!(head.contains("content-type: text/event-stream"), "{seen}");
    assert!(head.contains("cache-control: no-store"), "{seen}");
    assert!(seen.contains("event: live\ndata: {\"at\":\""), "{seen}");
    let audit = audit()?;
    site.store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| Ok(s.audit(&audit)?))
        .await?;
    read_until(
        &mut stream,
        &mut seen,
        "\"changed\":[\"overview\",\"activity\"]",
        1,
        Duration::from_secs(3),
    )
    .await?;
    site.store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            s.park(&analyte_store::Parked {
                kind: Identifier::new("mapping_finding")?,
                reason: Reason::new("synthetic")?,
                targets: vec![Identifier::new("synthetic")?],
            })?;
            Ok(())
        })
        .await?;
    read_until(
        &mut stream,
        &mut seen,
        "\"changed\":[\"review\",\"overview\",\"activity\",\"count\"]",
        1,
        Duration::from_secs(3),
    )
    .await?;
    drop(stream);
    site.stop().await
}

// Owner decisions 2026-11-20 (PR c2): with nothing changed the stream still
// says it is there every heartbeat, and it ends at its lifetime with a
// goodbye, so the browser opens a new one.
#[tokio::test(flavor = "multi_thread")]
async fn the_stream_beats_and_ends_at_its_lifetime() -> Result<(), Error> {
    let site = Site::start(bounds(1_200, 300, 100, 4)).await?;
    let mut stream = site.open(&site.auth()).await?;
    let mut seen = String::new();
    read_until(
        &mut stream,
        &mut seen,
        "\"changed\":[]",
        3,
        Duration::from_secs(3),
    )
    .await?;
    read_until(
        &mut stream,
        &mut seen,
        "event: bye",
        1,
        Duration::from_secs(3),
    )
    .await?;
    read_until(&mut stream, &mut seen, ENDED, 1, Duration::from_secs(2)).await?;
    site.stop().await
}

// Owner decisions 2026-11-20 (PR c2): streams are capped so they never take
// the slots pages need; one past the cap is refused and pages still answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_past_the_cap_is_refused_and_pages_still_answer() -> Result<(), Error> {
    let site = Site::start(bounds(60_000, 60_000, 100, 1)).await?;
    let mut first = site.open(&site.auth()).await?;
    let mut seen = String::new();
    read_until(
        &mut first,
        &mut seen,
        "event: live",
        1,
        Duration::from_secs(3),
    )
    .await?;
    let mut second = site.open(&site.auth()).await?;
    let mut refused = String::new();
    read_until(
        &mut second,
        &mut refused,
        "</html>",
        1,
        Duration::from_secs(3),
    )
    .await?;
    assert!(refused.starts_with("HTTP/1.1 503"), "{refused}");
    assert!(refused.contains("Too many live pages"));
    assert!(refused.contains("every admin live stream is in use"));
    let page = request(&site.started.3, "/", &site.auth(), None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    drop(first);
    site.stop().await
}

// Owner decisions 2026-11-20 (PR c2): signing out ends the session's stream,
// and shutdown ends open streams at once, so draining stays within its bound.
#[tokio::test(flavor = "multi_thread")]
async fn sign_out_and_shutdown_end_open_streams() -> Result<(), Error> {
    let site = Site::start(bounds(300_000, 60_000, 100, 4)).await?;
    let host = site.started.3.clone();
    let mut signed = site.open(&site.auth()).await?;
    let mut seen = String::new();
    read_until(
        &mut signed,
        &mut seen,
        "event: live",
        1,
        Duration::from_secs(3),
    )
    .await?;
    let token = encode(&csrf(&request(&host, "/", &site.auth(), None).await?)?);
    let out = request(
        &host,
        "/logout",
        &format!(
            "{}Content-Type: application/x-www-form-urlencoded\r\n",
            site.auth()
        ),
        Some(&format!("csrf={token}")),
    )
    .await?;
    assert!(out.starts_with("HTTP/1.1 303"), "{out}");
    read_until(&mut signed, &mut seen, ENDED, 1, Duration::from_secs(3)).await?;
    // A new sign-in's stream stays open until shutdown ends it.
    let cookie = login(&host, &site.bearer).await?;
    let mut open = site.open(&format!("Cookie: {cookie}\r\n")).await?;
    let mut held = String::new();
    read_until(
        &mut open,
        &mut held,
        "event: live",
        1,
        Duration::from_secs(3),
    )
    .await?;
    let started = std::time::Instant::now();
    site.stop().await?;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "draining waited for the stream: {:?}",
        started.elapsed()
    );
    read_until(&mut open, &mut held, ENDED, 1, Duration::from_secs(1)).await?;
    Ok(())
}
