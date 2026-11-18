//! Tests that order-cache trust moves only when a poll chain reaches the app's feed head.
use super::*;
use analyte_contract::document::OrderState;
use tokio::io::AsyncWriteExt as _;

const FRESHNESS: u64 = 30;
const CHANGES: &str = "c1";

fn app() -> Result<Identifier, TextError> {
    Identifier::new("app")
}

fn order(version: u64, state: OrderState) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut fields: analyte_contract::OrderFields = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/order.json"
    ))?;
    fields.version = std::num::NonZeroU64::new(version).ok_or("version")?;
    fields.state = state;
    Ok(serde_json::to_value(analyte_contract::Order::try_from(
        fields,
    )?)?)
}

/// A completed chain that began 20 seconds ago, and the second its trust runs out.
fn previous_chain() -> Result<(i64, i64), Box<dyn std::error::Error>> {
    let started = jiff::Timestamp::now()
        .as_second()
        .checked_sub(20)
        .ok_or("clock underflow")?;
    let expired = started
        .checked_add(i64::try_from(FRESHNESS)?)
        .ok_or("clock overflow")?;
    Ok((started, expired))
}

fn page(orders: &[serde_json::Value], next: &str, complete: bool) -> String {
    serde_json::json!({"contract_version":1,"orders":orders,"next_cursor":next,"complete":complete})
        .to_string()
}

/// A completed snapshot whose chain started `started`, holding version 1 of the
/// vector order as requested; later chains resume from `CHANGES`.
async fn completed_snapshot(store: &Handle, started: i64) -> TestResult {
    let document = serde_json::to_string(&order(1, OrderState::Requested)?)?;
    store
        .request::<_, TestStoreError>(until()?, move |s| {
            Ok(s.order_page(&analyte_store::OrderPage {
                destination: app()?,
                cursor: Identifier::new(CHANGES)?,
                complete: true,
                polled: started,
                orders: vec![analyte_store::CachedOrder {
                    id: Identifier::new("order-1")?,
                    sample: SampleId::new("000Ab12")?,
                    document,
                    tombstone: false,
                    expiry: i64::MAX,
                }],
            })?)
        })
        .await?;
    Ok(())
}

/// Answers one poll request, checking the cursor it resumes from.
async fn answer(
    listener: &tokio::net::TcpListener,
    cursor: &str,
    status: u16,
    body: &str,
) -> TestResult {
    reply(request(listener, cursor).await?, status, body).await
}

/// Accepts one poll request and checks the cursor it resumes from. The poller
/// sends it only after committing everything before it, so the store can be
/// probed here without racing the previous page.
async fn request(
    listener: &tokio::net::TcpListener,
    cursor: &str,
) -> Result<tokio::net::TcpStream, Box<dyn std::error::Error>> {
    let (mut stream, _) = listener.accept().await?;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 8192 {
            return Err("test headers too large".into());
        }
        head.push(stream.read_u8().await?);
    }
    let line = String::from_utf8_lossy(&head);
    let line = line.lines().next().unwrap_or_default();
    assert!(
        line.contains(&format!("cursor={cursor} ")),
        "poll did not resume from {cursor}: {line}"
    );
    Ok(stream)
}

async fn reply(mut stream: tokio::net::TcpStream, status: u16, body: &str) -> TestResult {
    stream
        .write_all(
            format!(
                "HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    Ok(())
}

/// What a machine query and a send-ahead listing would get at `at`, and the
/// trust time health reports.
struct Window {
    query: analyte_store::Lookup,
    send_ahead_trusted: bool,
    last_successful_poll: Option<i64>,
}

async fn window(store: &Handle, at: i64) -> Result<Window, Box<dyn std::error::Error>> {
    Ok(store
        .request::<_, TestStoreError>(until()?, move |s| {
            let destination = app()?;
            let send_ahead = s.pending_orders(
                &destination,
                &Identifier::new("analyzer")?,
                "",
                at,
                FRESHNESS,
            )?;
            Ok(Window {
                query: s.orders(&destination, &SampleId::new("000Ab12")?, at, FRESHNESS)?,
                send_ahead_trusted: !matches!(send_ahead, analyte_store::PendingOrders::NotTrusted),
                last_successful_poll: s.health(&destination, at, FRESHNESS)?.last_successful_poll,
            })
        })
        .await?)
}

/// The previous chain's trust has run out at `expired`; nothing may act on the cache.
async fn assert_untrusted_since(store: &Handle, previous: i64, expired: i64) -> TestResult {
    let seen = window(store, expired).await?;
    assert!(
        matches!(seen.query, analyte_store::Lookup::NotTrusted),
        "a machine query would use orders the unfinished chain may cancel"
    );
    assert!(
        !seen.send_ahead_trusted,
        "send-ahead would list orders the unfinished chain may cancel"
    );
    assert_eq!(seen.last_successful_poll, Some(previous));
    Ok(())
}

async fn listen() -> Result<(tokio::net::TcpListener, Destination), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let destination = destination(&format!("http://{}", listener.local_addr()?))?;
    Ok((listener, destination))
}

// App contract v1 (ADR 0008), GET orders
#[tokio::test]
async fn unfinished_chain_never_refreshes_trust() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let (listener, destination) = listen().await?;
    let secrets = Secrets("key");
    let poller = Poller::new(&store, &destination, &secrets);
    let (previous, expired) = previous_chain()?;
    completed_snapshot(&store, previous).await?;

    // Page one of two commits; page two, carrying the cancellation, fails.
    let peer = async {
        answer(&listener, CHANGES, 200, &page(&[], "c2", false)).await?;
        let second = request(&listener, "c2").await?;
        assert_untrusted_since(&store, previous, expired).await?;
        reply(second, 503, "").await
    };
    let (peer, result) =
        tokio::time::timeout(HANG_GUARD, async { tokio::join!(peer, poller.poll_once()) }).await?;
    peer?;
    assert!(matches!(result, Err(PollError::PollStatus(503))));
    assert_untrusted_since(&store, previous, expired).await?;
    let cursor = store
        .request::<_, TestStoreError>(until()?, |s| Ok(s.cursor(&app()?)?))
        .await?;
    assert_eq!(cursor.as_deref(), Some("c2"), "page one keeps its cursor");

    // The retried chain reaches the head: trust is fresh as of that chain's start.
    let cancel = page(&[order(2, OrderState::CancelRequested)?], "c3", true);
    let started = jiff::Timestamp::now().as_second();
    let (peer, result) = tokio::time::timeout(HANG_GUARD, async {
        tokio::join!(answer(&listener, "c2", 200, &cancel), poller.poll_once())
    })
    .await?;
    peer?;
    result?;
    let finished = jiff::Timestamp::now().as_second();
    let seen = window(&store, finished).await?;
    let last = seen
        .last_successful_poll
        .ok_or("completed chain not recorded")?;
    assert!((started..=finished).contains(&last));
    let analyte_store::Lookup::Orders(orders) = seen.query else {
        return Err("completed chain must be trusted".into());
    };
    let cached: analyte_contract::Order =
        serde_json::from_str(&orders.first().ok_or("order missing")?.document)?;
    assert_eq!(cached.fields().state, OrderState::CancelRequested);
    store.shutdown(until()?).await?;
    Ok(())
}

// App contract v1 (ADR 0008), GET orders
#[tokio::test]
async fn exhausted_page_limit_never_refreshes_trust() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = store(&directory.path().join("store.db")).await?;
    let (listener, mut destination) = listen().await?;
    destination.limits.max_pages = 2;
    let secrets = Secrets("key");
    let poller = Poller::new(&store, &destination, &secrets);
    let (previous, expired) = previous_chain()?;
    completed_snapshot(&store, previous).await?;
    let peer = async {
        answer(&listener, CHANGES, 200, &page(&[], "c2", false)).await?;
        answer(&listener, "c2", 200, &page(&[], "c3", false)).await
    };
    let (peer, result) =
        tokio::time::timeout(HANG_GUARD, async { tokio::join!(peer, poller.poll_once()) }).await?;
    peer?;
    assert!(matches!(result, Err(PollError::FeedProgress)));
    assert_untrusted_since(&store, previous, expired).await?;
    store.shutdown(until()?).await?;
    Ok(())
}
