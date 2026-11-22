//! Runs the checker against the reference receiver, whole and with one rule broken at a time.
#![forbid(unsafe_code)]
use analyte_contract::{MAX_DOCUMENT_BYTES, SigningSecret};
use analyte_contract_check::{Outcome, Report, SEED_FILE, Settings, parse_seed, run};
use analyte_fixtures::{HANG_GUARD, SECRET};
use analyte_receiver::Receiver;
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KEY: &str = "synthetic-app-key-7f3a";
const SEED: &[u8] = include_bytes!("../../../../docs/contract/vectors/seed-orders.json");

/// A reference receiver on a loopback port, stopped and joined by `stop`.
struct App {
    url: String,
    stop: tokio::sync::oneshot::Sender<()>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    receiver: Receiver,
    _directory: tempfile::TempDir,
}

impl App {
    async fn start(
        seeded: bool,
        wrap: impl FnOnce(Router) -> Router,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let receiver = Receiver::open(
            &directory.path().join("receiver.sqlite"),
            KEY.into(),
            SigningSecret::parse(SECRET)?,
        )?;
        if seeded {
            receiver.seed(parse_seed(SEED)?).await?;
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let router = wrap(receiver.router());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        Ok(Self {
            url,
            stop,
            server,
            receiver,
            _directory: directory,
        })
    }

    async fn check(&self, seeded: bool) -> Result<Report, Box<dyn std::error::Error>> {
        let settings = Settings::from_lookup(|name| match name {
            "ANALYTE_APP_URL" => Some(self.url.clone()),
            "ANALYTE_API_KEY" => Some(KEY.into()),
            "ANALYTE_SIGNING_SECRET" => Some(SECRET.into()),
            _ => None,
        })?;
        let seed = if seeded {
            Some(parse_seed(SEED)?)
        } else {
            None
        };
        Ok(tokio::time::timeout(HANG_GUARD, run(settings, seed.as_deref())).await??)
    }

    async fn stop(mut self) -> TestResult {
        self.stop.send(()).map_err(|()| "server already stopped")?;
        self.server.await??;
        self.receiver.shutdown().await?;
        Ok(())
    }
}

/// Runs a seeded check against a receiver wrapped by `wrap`; returns the failed case names.
async fn failures(
    wrap: impl FnOnce(Router) -> Router,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let app = App::start(true, wrap).await?;
    let report = app.check(true).await?;
    app.stop().await?;
    Ok(report.failed().into_iter().map(str::to_owned).collect())
}

fn skipped(report: &Report) -> Vec<&str> {
    report
        .cases
        .iter()
        .filter(|case| matches!(case.outcome, Outcome::Skip(_)))
        .map(|case| case.name.as_str())
        .collect()
}

// Contract v1 (docs/contract/v1.md), every section; the reference receiver implements it all
#[tokio::test]
async fn reference_receiver_passes() -> TestResult {
    let app = App::start(true, |router| router).await?;
    let report = app.check(true).await?;
    app.stop().await?;
    assert!(report.passed(), "{}", report.render());
    // The reference receiver never asks for a later retry, so no Retry-After is seen.
    assert_eq!(
        skipped(&report),
        ["retry-after.format"],
        "{}",
        report.render()
    );
    for name in [
        "idempotency.first",
        "idempotency.retry",
        "idempotency.changed-content",
        "idempotency.other-endpoint",
        "authentication.invalid-signature",
        "invalid-documents.missing-occurrence",
        "invalid-documents.missing-model",
        "invalid-documents.model-without-vendor",
        "limits.max-document-bytes",
        "orders.unknown-cursor",
        "orders.versions-only-increase",
        "seed.tombstones",
        "seed.paging",
    ] {
        assert!(
            report.cases.iter().any(|case| case.name == name),
            "{name} did not run"
        );
    }
    Ok(())
}

// Contract v1 "GET /orders": seeded-state cases need the developer to load the seed first
#[tokio::test]
async fn unseeded_run_says_what_to_seed() -> TestResult {
    let app = App::start(false, |router| router).await?;
    let report = app.check(false).await?;
    app.stop().await?;
    assert!(report.passed(), "{}", report.render());
    let seed_cases: Vec<_> = report
        .cases
        .iter()
        .filter(|case| case.name.starts_with("seed."))
        .collect();
    assert_eq!(seed_cases.len(), 4);
    for case in seed_cases {
        assert!(
            matches!(&case.outcome, Outcome::Skip(reason) if reason.contains(SEED_FILE) && reason.contains("--seed-file")),
            "{}",
            case.name
        );
    }
    Ok(())
}

// Contract v1 "HTTP and authentication": Standard Webhooks signature and 300-second window
#[tokio::test]
async fn ignored_signature_fails_signature_cases() -> TestResult {
    let failed = failures(|router| router.layer(from_fn(resign))).await?;
    assert_eq!(
        failed,
        [
            "authentication.invalid-signature",
            "authentication.missing-signature",
            "authentication.stale-timestamp",
            "authentication.future-timestamp",
        ]
    );
    Ok(())
}

// Contract v1 "Delivery responses and retries": the same delivery ID retried is a duplicate
#[tokio::test]
async fn retry_counted_as_new_fails_idempotency() -> TestResult {
    let failed = failures(|router| router.layer(from_fn(duplicate_as_accepted))).await?;
    assert_eq!(failed, ["idempotency.retry"]);
    Ok(())
}

// Contract v1 "HTTP and authentication": every request carries the bearer key
#[tokio::test]
async fn ignored_api_key_fails_key_cases() -> TestResult {
    let failed = failures(|router| router.layer(from_fn(any_key))).await?;
    assert_eq!(
        failed,
        [
            "authentication.wrong-api-key",
            "authentication.missing-api-key",
            "authentication.orders-wrong-api-key",
            "authentication.orders-missing-api-key",
        ]
    );
    Ok(())
}

// Contract v1 "Delivery responses and retries": 413 below the document limit pauses Analyte
#[tokio::test]
async fn small_body_limit_fails_size_case() -> TestResult {
    let failed = failures(|router| router.layer(from_fn(small_body_limit))).await?;
    assert_eq!(failed, ["limits.max-document-bytes"]);
    Ok(())
}

// Contract v1 "GET /orders": an unknown or expired cursor returns 410
#[tokio::test]
async fn endless_cursor_fails_cursor_case() -> TestResult {
    let failed = failures(|router| router.layer(from_fn(never_gone))).await?;
    assert_eq!(failed, ["orders.unknown-cursor"]);
    Ok(())
}

// Contract v1 "HTTP and authentication": credentials from the environment, never printed
#[tokio::test]
async fn command_reads_environment_and_hides_credentials() -> TestResult {
    let app = App::start(true, |router| router).await?;
    let seed = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/contract/vectors/seed-orders.json"
    );
    let command = |key: &str| {
        let mut command =
            tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte-contract-check"));
        command
            .args(["--seed-file", seed])
            .env("ANALYTE_APP_URL", &app.url)
            .env("ANALYTE_API_KEY", key)
            .env("ANALYTE_SIGNING_SECRET", SECRET)
            .env_remove("ANALYTE_REQUEST_TIMEOUT_SECONDS")
            .kill_on_drop(true);
        command
    };
    let passing = tokio::time::timeout(HANG_GUARD, command(KEY).output()).await??;
    let text = String::from_utf8(passing.stdout)?;
    assert_eq!(passing.status.code(), Some(0), "{text}");
    assert!(text.contains(" 0 failed"), "{text}");
    for secret in [KEY, SECRET, SECRET.trim_start_matches("whsec_")] {
        assert!(!text.contains(secret));
        assert!(!String::from_utf8(passing.stderr.clone())?.contains(secret));
    }
    let wrong =
        tokio::time::timeout(HANG_GUARD, command("another-synthetic-key").output()).await??;
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8(wrong.stdout)?.contains("FAIL    idempotency.first"));
    let mut missing = command(KEY);
    missing.env_remove("ANALYTE_APP_URL");
    let missing = tokio::time::timeout(HANG_GUARD, missing.output()).await??;
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8(missing.stderr)?.contains("ANALYTE_APP_URL is not set"));
    app.stop().await
}

/// Broken app: replaces whatever signature arrived with a valid one, so it never refuses.
async fn resign(request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_DOCUMENT_BYTES.saturating_add(1)).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let id = parts
        .headers
        .get("webhook-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if let (Some(id), Ok(secret)) = (id, SigningSecret::parse(SECRET)) {
        let timestamp = jiff::Timestamp::now().as_second().to_string();
        if let Ok(signature) = secret.sign(&id, &timestamp, &bytes)
            && let (Ok(timestamp), Ok(signature)) = (
                HeaderValue::from_str(&timestamp),
                HeaderValue::from_str(&signature),
            )
        {
            parts.headers.insert("webhook-timestamp", timestamp);
            parts.headers.insert("webhook-signature", signature);
        }
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

/// Broken app: reports a retried delivery as newly accepted.
async fn duplicate_as_accepted(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_DOCUMENT_BYTES).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let bytes = if bytes.as_ref() == br#"{"contract_version":1,"status":"duplicate"}"# {
        br#"{"contract_version":1,"status":"accepted"}"#.to_vec()
    } else {
        bytes.to_vec()
    };
    let mut parts = parts;
    parts.headers.remove("content-length");
    Response::from_parts(parts, Body::from(bytes))
}

/// Broken app: treats every request as carrying the right key.
async fn any_key(mut request: Request, next: Next) -> Response {
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {KEY}")) {
        request.headers_mut().insert("authorization", value);
    }
    next.run(request).await
}

/// Broken app: refuses bodies over 64 KiB, below the contract's document limit.
async fn small_body_limit(request: Request, next: Next) -> Response {
    let length = request
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok()?.parse::<u64>().ok());
    if length.is_some_and(|length| length > 65_536) {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    next.run(request).await
}

/// Broken app: answers an unknown cursor with an empty complete page instead of 410.
async fn never_gone(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    if response.status() == StatusCode::GONE {
        return axum::Json(serde_json::json!({
            "contract_version": 1, "orders": [], "next_cursor": "synthetic", "complete": true
        }))
        .into_response();
    }
    response
}
