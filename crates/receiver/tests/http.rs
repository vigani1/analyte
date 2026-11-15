//! Synthetic ADR 0008 contract regression cases.
#![forbid(unsafe_code)]
#[path = "../../analyte/tests/support/guard.rs"]
mod guard;
use analyte_contract::{Order, SigningSecret};
use analyte_receiver::Receiver;
use guard::HANG_GUARD;
use serde_json::{Value, json};
const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
async fn post(
    client: &reqwest::Client,
    url: &str,
    path: &str,
    body: &[u8],
    id: &str,
    key: &str,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let timestamp = jiff::Timestamp::now().as_second().to_string();
    let signature = SigningSecret::parse(SECRET)?.sign(id, &timestamp, body)?;
    Ok(client
        .post(format!("{url}/{path}"))
        .bearer_auth(key)
        .header("webhook-id", id)
        .header("webhook-timestamp", timestamp)
        .header("webhook-signature", signature)
        .body(body.to_vec())
        .send()
        .await?)
}
#[tokio::test]
async fn adr0008_durable_retry_conflict_auth_snapshot_tombstone_and_reset()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("receiver.sqlite");
    let receiver = Receiver::open(&database, "key".into(), SigningSecret::parse(SECRET)?)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let router = receiver.router();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let client = reqwest::Client::builder().timeout(HANG_GUARD).build()?;
    let body = include_bytes!("../../../docs/contract/vectors/decoded-text.json");
    let id = "019ab000-0000-7000-8000-000000000001";
    let mut unsupported: Value = serde_json::from_slice(include_bytes!(
        "../../../docs/contract/vectors/unsupported-formatting.json"
    ))?;
    unsupported["results"][0]["parts"][0]["values"][0]["decoded_text"] =
        json!("partially interpreted");
    assert_eq!(
        post(
            &client,
            &url,
            "results",
            &serde_json::to_vec(&unsupported)?,
            id,
            "key"
        )
        .await?
        .status(),
        422
    );
    // The contract owns evidence validation; invalid nested input must not reach storage.
    let mut invalid: Value = serde_json::from_slice(body)?;
    invalid["results"][0]["parts"] = json!([]);
    assert_eq!(
        post(
            &client,
            &url,
            "results",
            &serde_json::to_vec(&invalid)?,
            id,
            "key"
        )
        .await?
        .status(),
        422
    );
    // Discard the first response: retry must observe the durable ID.
    assert_eq!(
        post(&client, &url, "results", body, id, "key")
            .await?
            .status(),
        200
    );
    let response = post(&client, &url, "results", body, id, "key").await?;
    assert_eq!(response.status(), 200);
    let value: Value = serde_json::from_slice(&response.bytes().await?)?;
    assert_eq!(value.get("status"), Some(&json!("duplicate")));
    let mut changed = body.to_vec();
    changed.push(b' ');
    assert_eq!(
        post(&client, &url, "results", &changed, id, "key")
            .await?
            .status(),
        409
    );
    assert_eq!(
        post(&client, &url, "results", body, id, "wrong")
            .await?
            .status(),
        401
    );
    assert_eq!(
        post(&client, &url, "results", b"{}", id, "key")
            .await?
            .status(),
        422
    );
    assert_eq!(
        client
            .post(format!("{url}/results"))
            .bearer_auth("key")
            .body(body.to_vec())
            .send()
            .await?
            .status(),
        403
    );

    assert_eq!(
        client
            .post(format!("{url}/results"))
            .bearer_auth("key")
            .header("webhook-id", id)
            .header(
                "webhook-timestamp",
                jiff::Timestamp::now().as_second().to_string()
            )
            .header(
                "webhook-signature",
                "v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            )
            .body(body.to_vec())
            .send()
            .await?
            .status(),
        403
    );
    assert_eq!(
        post(
            &client,
            &url,
            "results",
            include_bytes!("../../../docs/contract/vectors/invalid-version.json"),
            id,
            "key"
        )
        .await?
        .status(),
        422
    );
    assert_eq!(
        client
            .post(format!("{url}/results"))
            .bearer_auth("key")
            .body(vec![b' '; 1_048_577])
            .send()
            .await?
            .status(),
        413
    );
    assert_eq!(
        client.get(format!("{url}/orders")).send().await?.status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{url}/orders?limit=oops"))
            .bearer_auth("key")
            .send()
            .await?
            .status(),
        400
    );
    let event = include_bytes!("../../../docs/contract/vectors/event.json");
    assert_eq!(
        post(
            &client,
            &url,
            "order-events",
            event,
            "019ab000-0000-7000-8000-000000000002",
            "key"
        )
        .await?
        .status(),
        200
    );
    let order: Value =
        serde_json::from_slice(include_bytes!("../../../docs/contract/vectors/order.json"))?;
    receiver
        .seed(vec![serde_json::from_value::<Order>(order.clone())?])
        .await?;
    let response = client
        .get(format!("{url}/orders?limit=1"))
        .bearer_auth("key")
        .send()
        .await?;
    let page: Value = serde_json::from_slice(&response.bytes().await?)?;
    assert_eq!(page.get("complete"), Some(&json!(true)));
    let cursor = page
        .get("next_cursor")
        .and_then(Value::as_str)
        .ok_or("cursor")?;
    let mut tombstone = order;
    tombstone["version"] = json!(2);
    tombstone["tombstone"] = json!(true);
    tombstone["state"] = json!("cancel_confirmed");
    receiver
        .seed(vec![serde_json::from_value(tombstone)?])
        .await?;
    let response = client
        .get(format!("{url}/orders?cursor={cursor}"))
        .bearer_auth("key")
        .send()
        .await?;
    let changes: Value = serde_json::from_slice(&response.bytes().await?)?;
    assert_eq!(changes.pointer("/orders/0/tombstone"), Some(&json!(true)));
    assert_eq!(
        client
            .get(format!("{url}/orders?cursor=unknown"))
            .bearer_auth("key")
            .send()
            .await?
            .status(),
        410
    );
    assert_eq!(
        client
            .get(format!("{url}/orders?limit=101"))
            .bearer_auth("key")
            .send()
            .await?
            .status(),
        400
    );

    // A paged snapshot must not absorb a newly changed order mid-traversal.
    let base: Value =
        serde_json::from_slice(include_bytes!("../../../docs/contract/vectors/order.json"))?;
    let mut second = base.clone();
    second["id"] = json!("order-2");
    let mut third = base.clone();
    third["id"] = json!("order-3");
    receiver
        .seed(vec![
            serde_json::from_value(second)?,
            serde_json::from_value(third)?,
        ])
        .await?;
    let read_page = |cursor: String| {
        client
            .get(format!("{url}/orders?limit=1{cursor}"))
            .bearer_auth("key")
    };
    let first: Value =
        serde_json::from_slice(&read_page(String::new()).send().await?.bytes().await?)?;
    assert_eq!(first.get("complete"), Some(&json!(false)));
    let mut fourth = base.clone();
    fourth["id"] = json!("order-4");
    receiver.seed(vec![serde_json::from_value(fourth)?]).await?;
    let mut cursor = first
        .get("next_cursor")
        .and_then(Value::as_str)
        .ok_or("cursor")?
        .to_owned();
    let mut seen = 1_u32;
    loop {
        let page: Value = serde_json::from_slice(
            &read_page(format!("&cursor={cursor}"))
                .send()
                .await?
                .bytes()
                .await?,
        )?;
        seen = seen
            .checked_add(u32::try_from(
                page.get("orders")
                    .and_then(Value::as_array)
                    .ok_or("orders")?
                    .len(),
            )?)
            .ok_or("count")?;
        cursor = page
            .get("next_cursor")
            .and_then(Value::as_str)
            .ok_or("cursor")?
            .to_owned();
        if page.get("complete") == Some(&json!(true)) {
            break;
        }
        if seen > 3 {
            return Err("snapshot failed to finish".into());
        }
    }
    assert_eq!(seen, 3);
    let changes: Value = serde_json::from_slice(
        &read_page(format!("&cursor={cursor}"))
            .send()
            .await?
            .bytes()
            .await?,
    )?;
    assert_eq!(changes.pointer("/orders/0/id"), Some(&json!("order-4")));
    let connection = rusqlite::Connection::open(&database)?;
    connection.execute("UPDATE cursors SET expires=0", [])?;
    assert_eq!(
        read_page(format!("&cursor={cursor}"))
            .send()
            .await?
            .status(),
        410
    );
    let (stored, digest): (Vec<u8>, Vec<u8>) = connection.query_row(
        "SELECT body,digest FROM deliveries WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(stored, body);
    assert_eq!(digest, analyte_contract::content_digest(body));
    drop(connection);
    // Seed retries do not add feed entries; regressing versions are rejected.
    let old: Order = serde_json::from_value(base)?;
    assert!(receiver.seed(vec![old]).await.is_err());
    stop.send(()).map_err(|()| "server stopped")?;
    server.await??;
    receiver.shutdown().map_err(|_| "worker panicked")?;
    let receiver = Receiver::open(&database, "key".into(), SigningSecret::parse(SECRET)?)?;
    let connection = rusqlite::Connection::open(&database)?;
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM deliveries", [], |row| row.get(0))?;
    assert_eq!(count, 2);
    receiver.shutdown().map_err(|_| "worker panicked")?;
    Ok(())
}
