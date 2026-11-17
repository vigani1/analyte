//! Receives synthetic operational alerts through the reference receiver's durable worker.
use super::{
    Arc, Bytes, Command, Duration, HeaderMap, Inner, Response, State, StatusCode, StoreError,
    Value, authorized, failure, oneshot,
};
use axum::response::IntoResponse as _;

pub(super) async fn receive(
    State(state): State<Arc<Inner>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !authorized(&state, &headers) {
        return failure(StatusCode::UNAUTHORIZED, "authentication");
    }
    if body.len() > 16384 {
        return failure(StatusCode::PAYLOAD_TOO_LARGE, "alert_size");
    }
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return failure(StatusCode::BAD_REQUEST, "alert_json");
    };
    let Some(id) = value.get("event_id").and_then(Value::as_str) else {
        return failure(StatusCode::BAD_REQUEST, "alert_identity");
    };
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return failure(StatusCode::BAD_REQUEST, "alert_identity");
    }
    let (reply, response) = oneshot::channel();
    if state
        .try_send(Command::Delivery {
            id: id.into(),
            kind: "alerts",
            body: body.to_vec(),
            reply,
        })
        .is_err()
    {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "storage_queue");
    }
    match tokio::time::timeout(Duration::from_secs(5), response).await {
        Ok(Ok(Ok(_))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(StoreError::Conflict))) => failure(StatusCode::CONFLICT, "conflict"),
        Ok(Ok(Err(StoreError::Database(_) | StoreError::Cursor | StoreError::OrderVersion)))
        | Ok(Err(_))
        | Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "storage_failure"),
    }
}
