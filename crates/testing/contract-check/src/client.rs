//! Sends bounded, signed requests to the app the way Analyte does: no redirects, verified TLS, one deadline.
use crate::settings::Settings;
use analyte_contract::{MAX_DOCUMENT_BYTES, ResponseAction, classify_response};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{KeyInit as _, Mac as _};
use std::sync::Mutex;

/// A request that never got a complete answer.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RequestError {
    /// The whole request, body included, ran past the deadline.
    #[error("no complete answer within the request deadline")]
    Timeout,
    /// Connection, TLS or HTTP failure, with its causes on the same line.
    #[error("request failed: {}", causes(.0))]
    Http(#[from] reqwest::Error),
    /// The answer body exceeded the contract document limit.
    #[error("answer body exceeds the 1,048,576-byte document limit")]
    TooLarge,
    /// The request could not be signed or addressed.
    #[error("request could not be built: {0}")]
    Build(&'static str),
}

/// One complete answer from the app.
pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) retry_after: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl Answer {
    /// What Analyte would do with this answer, in operator words.
    pub(crate) fn action(&self) -> String {
        match classify_response(self.status, &self.body, self.retry_after.as_deref()) {
            ResponseAction::Done(status) => format!("done, {status:?}").to_lowercase(),
            ResponseAction::Conflict => "parked as a conflict".into(),
            ResponseAction::Parked { reason } => format!("parked: {}", one_line(&reason)),
            ResponseAction::PauseForSetup { reason } => format!("destination paused: {reason}"),
            ResponseAction::Retry { .. } => "retried later".into(),
        }
    }

    /// Status and Analyte's action, for a one-line reason.
    pub(crate) fn describe(&self) -> String {
        format!("got HTTP {} (Analyte: {})", self.status, self.action())
    }
}

/// Which bearer key a request carries.
#[derive(Clone, Copy)]
pub(crate) enum Key<'a> {
    Correct,
    Other(&'a str),
    Missing,
}

/// How a POST request is signed.
#[derive(Clone, Copy)]
pub(crate) enum Signature<'a> {
    /// Signed now with the configured secret.
    Correct,
    /// Signed with the configured secret over a shifted timestamp.
    Shifted(i64),
    /// This exact header value instead of a signature.
    Fixed(&'a str),
    /// A candidate from another secret first, then the correct one.
    Rotated(&'a analyte_contract::SigningSecret),
    /// No webhook-signature header.
    Missing,
}

/// The app's endpoints with the run's credentials, deadline and Retry-After log.
pub(crate) struct App {
    client: reqwest::Client,
    settings: Settings,
    retry_after: Mutex<Vec<String>>,
}

impl App {
    pub(crate) fn new(settings: Settings) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .timeout(settings.timeout)
            .build()?;
        Ok(Self {
            client,
            settings,
            retry_after: Mutex::new(Vec::new()),
        })
    }

    /// POSTs `body` to `path` with `id` as webhook-id.
    pub(crate) async fn post(
        &self,
        path: &str,
        id: &str,
        body: &[u8],
        key: Key<'_>,
        signature: Signature<'_>,
    ) -> Result<Answer, RequestError> {
        let now = jiff::Timestamp::now().as_second();
        let timestamp = match signature {
            Signature::Shifted(offset) => now.saturating_add(offset),
            Signature::Correct
            | Signature::Fixed(_)
            | Signature::Rotated(_)
            | Signature::Missing => now,
        }
        .to_string();
        let sign = |secret: &analyte_contract::SigningSecret| {
            secret
                .sign(id, &timestamp, body)
                .map_err(|_| RequestError::Build("webhook metadata or body outside signing bounds"))
        };
        let header = match signature {
            Signature::Correct | Signature::Shifted(_) => Some(sign(&self.settings.secret)?),
            Signature::Fixed(value) => Some(value.to_owned()),
            Signature::Rotated(other) => {
                Some(format!("{} {}", sign(other)?, sign(&self.settings.secret)?))
            }
            Signature::Missing => None,
        };
        let mut request = self
            .client
            .post(self.url(path)?)
            .header("content-type", "application/json")
            .header("webhook-id", id)
            .header("webhook-timestamp", &timestamp)
            .body(body.to_vec());
        if let Some(header) = header {
            request = request.header("webhook-signature", header);
        }
        self.send(self.authorize(request, key)).await
    }

    /// POSTs a body above the signing bound, signed with the same HMAC rule.
    pub(crate) async fn post_oversized(
        &self,
        path: &str,
        id: &str,
        body: &[u8],
    ) -> Result<Answer, RequestError> {
        let timestamp = jiff::Timestamp::now().as_second().to_string();
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(&self.settings.oversize_key)
            .map_err(|_| RequestError::Build("signing key"))?;
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(body);
        let signature = format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()));
        let request = self
            .client
            .post(self.url(path)?)
            .header("content-type", "application/json")
            .header("webhook-id", id)
            .header("webhook-timestamp", timestamp)
            .header("webhook-signature", signature)
            .body(body.to_vec());
        self.send(self.authorize(request, Key::Correct)).await
    }

    /// GETs `/orders` with the given query pairs.
    pub(crate) async fn orders(
        &self,
        query: &[(&str, &str)],
        key: Key<'_>,
    ) -> Result<Answer, RequestError> {
        let mut url = self.url("orders")?;
        url.query_pairs_mut().extend_pairs(query);
        self.send(self.authorize(self.client.get(url), key)).await
    }

    /// Retry-After values seen so far, in arrival order.
    pub(crate) fn retry_after_seen(&self) -> Vec<String> {
        self.retry_after
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }

    fn url(&self, path: &str) -> Result<url::Url, RequestError> {
        self.settings
            .base
            .join(path)
            .map_err(|_| RequestError::Build("endpoint path"))
    }

    fn authorize(&self, request: reqwest::RequestBuilder, key: Key<'_>) -> reqwest::RequestBuilder {
        match key {
            Key::Correct => request.bearer_auth(&self.settings.api_key),
            Key::Other(other) => request.bearer_auth(other),
            Key::Missing => request,
        }
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Answer, RequestError> {
        tokio::time::timeout(self.settings.timeout, async {
            let mut response = request.send().await?;
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("retry-after")
                .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
            if let Some(value) = &retry_after
                && let Ok(mut seen) = self.retry_after.lock()
            {
                seen.push(value.chars().take(80).collect());
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > MAX_DOCUMENT_BYTES.saturating_sub(body.len()) {
                    return Err(RequestError::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(Answer {
                status,
                retry_after,
                body,
            })
        })
        .await
        .map_err(|_| RequestError::Timeout)?
    }
}

/// An error and each of its sources, joined on one line.
fn causes(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// App text shown on one bounded line: control characters escaped, at most 300 characters.
pub(crate) fn one_line(text: &str) -> String {
    const LIMIT: usize = 300;
    let mut escaped = text.chars().flat_map(|c| {
        let token: Vec<char> = if c.is_control() {
            c.escape_default().collect()
        } else {
            vec![c]
        };
        token
    });
    let mut line: String = escaped.by_ref().take(LIMIT).collect();
    if escaped.next().is_some() {
        line.push('…');
    }
    line
}
