//! Receives durable contract deliveries and serves orders without clinical decisions.
#![forbid(unsafe_code)]
mod alerts;
use analyte_contract::{Delivery, OrderEvent, SigningSecret, content_digest, decode};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};

/// Receiver initialization failures.
#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    /// Database could not be opened or initialized.
    #[error("database initialization failed")]
    Database(#[from] rusqlite::Error),
    /// Storage thread could not start.
    #[error("storage thread unavailable")]
    Thread(#[from] std::io::Error),
    /// Invalid key configuration.
    #[error("invalid bearer key")]
    Key,
    /// Seed exceeded the order count limit.
    #[error("seed exceeds 100 orders")]
    SeedLimit,
    /// Bounded worker queue is full or closed.
    #[error("storage queue unavailable")]
    Queue,
    /// Operation may have committed; retry with the same identity.
    #[error("storage response deadline expired")]
    Deadline,
    /// Worker stopped without returning an answer.
    #[error("storage worker stopped")]
    Worker,
    /// Stored order data could not be encoded.
    #[error("stored order encoding invalid")]
    Encoding,
    /// Order versions must increase; equal versions must have identical fields.
    #[error("conflicting order version")]
    OrderVersion,
}
enum Command {
    Delivery {
        id: String,
        kind: &'static str,
        body: Vec<u8>,
        reply: oneshot::Sender<Result<bool, StoreError>>,
    },
    Seed {
        orders: Vec<analyte_contract::Order>,
        reply: oneshot::Sender<Result<(), StoreError>>,
    },
    Orders {
        cursor: Option<String>,
        limit: u32,
        reply: oneshot::Sender<Result<Value, StoreError>>,
    },
}
#[derive(Debug, thiserror::Error)]
enum StoreError {
    #[error("database failure")]
    Database(#[from] rusqlite::Error),
    #[error("conflicting content")]
    Conflict,
    #[error("cursor no longer valid")]
    Cursor,
    #[error("conflicting order version")]
    OrderVersion,
}
struct Inner {
    key: String,
    secret: SigningSecret,
    sender: mpsc::Sender<Command>,
}
/// Owns the database worker. Dropping all routers closes its bounded queue.
pub struct Receiver {
    state: Arc<Inner>,
    worker: std::thread::JoinHandle<()>,
}
impl Receiver {
    /// Opens durable SQLite storage and starts its dedicated worker.
    ///
    /// # Errors
    /// Returns invalid configuration, database initialization or thread-start failures.
    pub fn open(path: &Path, key: String, secret: SigningSecret) -> Result<Self, ReceiverError> {
        if key.is_empty() || key.len() > 4096 || !key.is_ascii() {
            return Err(ReceiverError::Key);
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS deliveries(id TEXT PRIMARY KEY, kind TEXT NOT NULL, digest BLOB NOT NULL, body BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS changes(seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL, body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cursors(token TEXT PRIMARY KEY, position INTEGER NOT NULL, ceiling INTEGER, snapshot INTEGER NOT NULL, expires INTEGER NOT NULL);")?;
        let (sender, mut receiver) = mpsc::channel(32);
        let worker = std::thread::Builder::new()
            .name("receiver-storage".into())
            .spawn(move || {
                let mut connection = connection;
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Delivery {
                            id,
                            kind,
                            body,
                            reply,
                        } => {
                            let _ = reply.send(store(&mut connection, &id, kind, &body));
                        }
                        Command::Seed { orders, reply } => {
                            let _ = reply.send(seed(&mut connection, &orders));
                        }
                        Command::Orders {
                            cursor,
                            limit,
                            reply,
                        } => {
                            let _ = reply.send(page(&mut connection, cursor.as_deref(), limit));
                        }
                    }
                }
            })?;
        Ok(Self {
            state: Arc::new(Inner {
                key,
                secret,
                sender,
            }),
            worker,
        })
    }

    /// Seeds a bounded set of validated orders for the worked example.
    ///
    /// # Errors
    /// Returns failure if storage is busy, unavailable, or cannot commit.
    pub async fn seed(&self, orders: Vec<analyte_contract::Order>) -> Result<(), ReceiverError> {
        if orders.len() > 100 {
            return Err(ReceiverError::SeedLimit);
        }
        let (reply, response) = oneshot::channel();
        self.state
            .sender
            .try_send(Command::Seed { orders, reply })
            .map_err(|_| ReceiverError::Queue)?;
        tokio::time::timeout(Duration::from_secs(5), response)
            .await
            .map_err(|_| ReceiverError::Deadline)?
            .map_err(|_| ReceiverError::Worker)?
            .map_err(|error| match error {
                StoreError::Database(error) => ReceiverError::Database(error),
                StoreError::Conflict | StoreError::Cursor => ReceiverError::Encoding,
                StoreError::OrderVersion => ReceiverError::OrderVersion,
            })
    }

    /// Serves a local synthetic receiver until its owner requests shutdown.
    /// The returned future owns its router; callers must await it before joining storage.
    /// # Errors
    /// Reports listener or server I/O failures.
    pub fn serve_until(
        &self,
        listener: tokio::net::TcpListener,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static {
        let router = self.router();
        async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    if !*stop.borrow() {
                        let _ = stop.changed().await;
                    }
                })
                .await
        }
    }

    /// Builds bounded HTTP routes; dropping an interrupted handler does not roll back a committed delivery.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/alerts", post(alerts::receive))
            .route("/results", post(results))
            .route("/order-events", post(events))
            .route("/orders", get(orders))
            .layer(DefaultBodyLimit::max(analyte_contract::MAX_DOCUMENT_BYTES))
            .layer(axum::middleware::from_fn(deadline))
            .with_state(Arc::clone(&self.state))
    }
    /// Joins the owned storage worker after the server and all router clones are dropped.
    ///
    /// # Errors
    /// Returns an error if the worker panicked.
    pub fn shutdown(self) -> std::thread::Result<()> {
        drop(self.state);
        self.worker.join()
    }
}
fn store(
    connection: &mut Connection,
    id: &str,
    kind: &str,
    body: &[u8],
) -> Result<bool, StoreError> {
    let digest = content_digest(body);
    let transaction = connection.transaction()?;
    let existing: Option<(String, Vec<u8>)> = transaction
        .query_row(
            "SELECT kind,digest FROM deliveries WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((old_kind, old_digest)) = existing {
        if old_kind != kind || old_digest != digest {
            return Err(StoreError::Conflict);
        }
        return Ok(false);
    }
    transaction.execute(
        "INSERT INTO deliveries VALUES (?1,?2,?3,?4)",
        params![id, kind, digest.as_slice(), body],
    )?;
    transaction.commit()?;
    Ok(true)
}
fn page(
    connection: &mut Connection,
    cursor: Option<&str>,
    limit: u32,
) -> Result<Value, StoreError> {
    let now = jiff::Timestamp::now().as_second();
    let transaction = connection.transaction()?;
    let (position, ceiling, snapshot): (i64, Option<i64>, bool) = if let Some(token) = cursor {
        transaction
            .query_row(
                "SELECT position,ceiling,snapshot FROM cursors WHERE token=?1 AND expires>=?2",
                params![token, now],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or(StoreError::Cursor)?
    } else {
        (
            0,
            Some(
                transaction.query_row("SELECT COALESCE(MAX(seq),0) FROM changes", [], |row| {
                    row.get(0)
                })?,
            ),
            true,
        )
    };
    let high: i64 = match ceiling {
        Some(high) => high,
        None => transaction.query_row("SELECT COALESCE(MAX(seq),0) FROM changes", [], |row| {
            row.get(0)
        })?,
    };
    let rows: Vec<(i64, String)> = {
        let mut statement = transaction.prepare(
            "SELECT seq,body FROM changes WHERE seq>?1 AND seq<=?2 AND (?4=0 OR seq=(SELECT MAX(c.seq) FROM changes c WHERE c.id=changes.id AND c.seq<=?2)) ORDER BY seq LIMIT ?3",
        )?;
        statement
            .query_map(params![position, high, limit, snapshot], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<_, _>>()?
    };
    let last = rows.last().map_or(position, |(seq, _)| *seq);
    let remaining: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM changes WHERE seq>?1 AND seq<=?2 AND (?3=0 OR seq=(SELECT MAX(c.seq) FROM changes c WHERE c.id=changes.id AND c.seq<=?2)))",
        params![last, high, snapshot],
        |row| row.get(0),
    )?;
    let token = uuid::Uuid::now_v7().to_string();
    let expiry = now.checked_add(3600).ok_or(StoreError::Cursor)?;
    transaction.execute(
        "INSERT INTO cursors VALUES (?1,?2,?3,?4,?5)",
        params![
            token,
            if remaining { last } else { high },
            if remaining { Some(high) } else { None },
            remaining && snapshot,
            expiry
        ],
    )?;
    let documents: Vec<Value> = rows
        .iter()
        .map(|(_, body)| serde_json::from_str(body).map_err(|_| StoreError::Cursor))
        .collect::<Result<_, _>>()?;
    transaction.commit()?;
    Ok(json!({"contract_version":1,"orders":documents,"next_cursor":token,"complete":!remaining}))
}
fn failure(status: StatusCode, code: &str) -> Response {
    (
        status,
        Json(json!({"contract_version":1,"error":code,"detail":code})),
    )
        .into_response()
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}
fn authorized(state: &Inner, headers: &HeaderMap) -> bool {
    header(headers, "authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|v| analyte_contract::credential_matches(v.as_bytes(), state.key.as_bytes()))
}
async fn results(
    State(state): State<Arc<Inner>>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    match body {
        Ok(body) => receive(state, headers, body, "results").await,
        Err(_) => failure(StatusCode::PAYLOAD_TOO_LARGE, "body_rejected"),
    }
}
async fn events(
    State(state): State<Arc<Inner>>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    match body {
        Ok(body) => receive(state, headers, body, "order-events").await,
        Err(_) => failure(StatusCode::PAYLOAD_TOO_LARGE, "body_rejected"),
    }
}
async fn receive(
    state: Arc<Inner>,
    headers: HeaderMap,
    body: Bytes,
    kind: &'static str,
) -> Response {
    if !authorized(&state, &headers) {
        return failure(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let (Some(id), Some(timestamp), Some(signature)) = (
        header(&headers, "webhook-id"),
        header(&headers, "webhook-timestamp"),
        header(&headers, "webhook-signature"),
    ) else {
        return failure(StatusCode::FORBIDDEN, "missing_signature");
    };
    if state
        .secret
        .verify(
            id,
            timestamp,
            &body,
            signature,
            jiff::Timestamp::now().as_second(),
        )
        .is_err()
    {
        return failure(StatusCode::FORBIDDEN, "invalid_signature");
    }
    // Contract deserialization validates every evidence part, including decoded
    // text availability, before storage. Never interpret source protocol escapes here. Keep
    // the received body itself: serializing the validated types would lose unknown fields.
    let document_id = if kind == "results" {
        decode::<Delivery>(&body).map(|d| d.fields().delivery_id.as_str().to_owned())
    } else {
        decode::<OrderEvent>(&body).map(|d| d.fields().delivery_id.as_str().to_owned())
    };
    let Ok(document_id) = document_id else {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid_document");
    };
    if id != document_id {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "delivery_id_mismatch");
    }
    let (reply, response) = oneshot::channel();
    if state
        .sender
        .try_send(Command::Delivery {
            id: document_id,
            kind,
            body: body.to_vec(),
            reply,
        })
        .is_err()
    {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "storage_busy");
    }
    match tokio::time::timeout(Duration::from_secs(5), response).await {
        Ok(Ok(Ok(accepted))) => {
            Json(json!({"contract_version":1,"status":if accepted {"accepted"} else {"duplicate"}}))
                .into_response()
        }
        Ok(Ok(Err(StoreError::Conflict))) => failure(StatusCode::CONFLICT, "conflict"),
        _ => failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"),
    }
}
#[derive(Deserialize)]
struct OrderQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}
async fn orders(
    State(state): State<Arc<Inner>>,
    headers: HeaderMap,
    query: Result<Query<OrderQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    if !authorized(&state, &headers) {
        return failure(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let Ok(Query(query)) = query else {
        return failure(StatusCode::BAD_REQUEST, "invalid_query");
    };
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) || query.cursor.as_ref().is_some_and(|s| s.len() > 256) {
        return failure(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let (reply, response) = oneshot::channel();
    if state
        .sender
        .try_send(Command::Orders {
            cursor: query.cursor,
            limit,
            reply,
        })
        .is_err()
    {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "storage_busy");
    }
    match tokio::time::timeout(Duration::from_secs(5), response).await {
        Ok(Ok(Ok(page))) => Json(page).into_response(),
        Ok(Ok(Err(StoreError::Cursor))) => failure(StatusCode::GONE, "cursor_reset"),
        _ => failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"),
    }
}

fn seed(connection: &mut Connection, orders: &[analyte_contract::Order]) -> Result<(), StoreError> {
    let transaction = connection.transaction()?;
    for order in orders {
        let body = serde_json::to_string(order).map_err(|_| StoreError::Cursor)?;
        let previous: Option<String> = transaction
            .query_row(
                "SELECT body FROM changes WHERE id=?1 ORDER BY seq DESC LIMIT 1",
                [order.fields().id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            let old: analyte_contract::Order =
                serde_json::from_str(&previous).map_err(|_| StoreError::Cursor)?;
            if order.fields().version < old.fields().version {
                return Err(StoreError::OrderVersion);
            }
            if order.fields().version == old.fields().version {
                if body != previous {
                    return Err(StoreError::OrderVersion);
                }
                continue;
            }
        }

        transaction.execute(
            "INSERT INTO changes(id,body) VALUES (?1,?2)",
            params![order.fields().id.as_str(), body],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

async fn deadline(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await {
        Ok(response) => response,
        Err(_) => failure(StatusCode::REQUEST_TIMEOUT, "request_deadline"),
    }
}
