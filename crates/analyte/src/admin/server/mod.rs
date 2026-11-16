//! Routes authenticated local HTTP requests with session and request limits.
use super::{
    Token, escape,
    listener::Listener,
    sessions::{Session, Sessions},
};
use crate::{Error, installation::config::Program};
use analyte_model::Identifier;
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, FromRequest, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use std::{
    collections::BTreeMap,
    future::IntoFuture as _,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::{Semaphore, watch};

pub(super) struct Machine {
    pub(super) id: Identifier,
    pub(super) running: Arc<AtomicBool>,
    pub(super) departure_html: String,
}

pub(super) struct Context {
    pub(super) alert_endpoints: Vec<Identifier>,
    pub(super) retention: Vec<(Identifier, Option<u16>)>,
    pub(super) config_path: std::path::PathBuf,
    token: Token,
    pub(super) sessions: Sessions,
    host: String,
    origin: String,
    pub(super) store: Arc<analyte_store::Handle>,
    pub(super) machines: Vec<Machine>,
    pub(super) destinations: Vec<analyte_sink::Destination>,
    pub(super) directory: std::path::PathBuf,
    pub(super) reserve: u64,
    // Struct fields drop in declaration order: notify only after store is released.
    _released: tokio::sync::oneshot::Sender<()>,
}

pub(crate) struct Server {
    listener: Listener,
    app: Router,
    released: tokio::sync::oneshot::Receiver<()>,
    pub(crate) owners: BTreeMap<String, Arc<AtomicBool>>,
}

pub(crate) async fn prepare(
    program: &Program,
    store: Arc<analyte_store::Handle>,
    loopback: bool,
) -> Result<Option<Server>, Error> {
    let Some(address) = program.admin_address() else {
        return Ok(None);
    };
    // All credential and bind failures precede spawning any service owner.
    let token = Token::load(program.data_directory())?;
    let socket = tokio::net::TcpListener::bind(address).await?;
    let host = address.to_string();
    let machines: Vec<_> = program
        .machines()
        .iter()
        .map(|m| {
            Ok(Machine {
                departure_html: super::departures::enabled(&m.snapshot)?,
                id: m.id().clone(),
                running: Arc::new(AtomicBool::new(true)),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let owners = machines
        .iter()
        .map(|m| (m.id.as_str().to_owned(), m.running.clone()))
        .collect();
    let (released, release) = tokio::sync::oneshot::channel();
    let state = Arc::new(Context {
        alert_endpoints: program
            .alert_delivery()
            .endpoints()
            .iter()
            .map(|e| e.id().clone())
            .collect(),
        retention: crate::runtime::retention::policies(program),
        config_path: program.source_path.clone(),
        token,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host,
        store,
        machines,
        destinations: crate::runtime::delivery::destinations(program, loopback)?,
        directory: program.data_directory().to_owned(),
        reserve: program.store_limits.reserve_bytes,
        _released: released,
    });
    Ok(Some(Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state),
        released: release,
        owners,
    }))
}

impl Server {
    /// Shutdown stops admission and drains active requests; the service bounds and
    /// joins this owner. Cancellation can leave an already submitted read running.
    pub(crate) async fn run(self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        drain_server(
            axum::serve(self.listener, self.app)
                .with_graceful_shutdown(async move {
                    crate::machine::stopping(&mut stop).await;
                })
                .into_future(),
            self.released,
        )
        .await
    }
}

// Axum's completion signal precedes destruction of its connection futures.
// The service must not close storage until their router state has also dropped.
async fn drain_server(
    served: impl Future<Output = std::io::Result<()>>,
    released: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Error> {
    served.await?;
    let _ = released.await;
    Ok(())
}

fn router(state: Arc<Context>) -> Router {
    Router::new()
        .route("/", get(super::overview::show))
        .route("/login", get(login_page).post(login))
        .route(
            "/retention",
            get(super::retention::show).post(super::retention::hold),
        )
        .route("/machines", get(super::pages::machines))
        .route("/receipts/{id}/departures", get(super::departures::receipt))
        .route("/parked", get(super::pages::parked))
        .route("/parked/{id}", get(super::pages::detail))
        .route("/parked/{id}/raw", post(super::pages::raw))
        .route("/orders", get(super::pages::orders))
        .route("/deliveries", get(super::actions::deliveries))
        .route(
            "/deliveries/{id}/resend",
            get(super::actions::confirm_resend).post(super::actions::resend),
        )
        .route(
            "/parked/{id}/reprocess",
            get(super::actions::confirm_reprocess).post(super::actions::reprocess),
        )
        .layer(DefaultBodyLimit::max(1024))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

fn unauthorized(state: &Context, existing: Option<&Arc<Session>>) -> Response {
    tracing::warn!("admin authentication refused");
    let session = existing
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| state.sessions.issue(false, None));
    let mut response = match session {
        Ok(session) => {
            let html = format!(
                "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Analyte login</title><h1>Analyte admin</h1><form action=\"/login\" method=\"post\"><input name=\"csrf\" type=\"hidden\" value=\"{}\"><label>Token <input name=\"token\" type=\"password\" required autocomplete=\"off\" maxlength=\"44\"></label><button>Log in</button></form></html>",
                escape(session.csrf.form_value())
            );
            let mut response = (StatusCode::UNAUTHORIZED, Html(html)).into_response();
            if let Ok(value) = header::HeaderValue::from_str(&session.id.cookie(false)) {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
        }
        Err(_) => (StatusCode::UNAUTHORIZED, "admin login session unavailable").into_response(),
    };
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    response
}

async fn protect(State(state): State<Arc<Context>>, mut request: Request, next: Next) -> Response {
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(state.host.as_str());
    let login = request.uri().path() == "/login";
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|v| v.trim().strip_prefix("analyte_session="))
        });
    let session = state.sessions.lookup(cookie);
    let authenticated = bearer.is_some_and(|v| state.token.matches(v))
        || session
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .is_some_and(|s| s.authenticated);
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    let same_origin = origin.is_none_or(|v| v == state.origin);
    let mut response = if session.is_err() {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "admin session lookup failed",
        )
            .into_response()
    } else if !valid_host || !same_origin {
        unauthorized(&state, None)
    } else if login || authenticated {
        let session = session.ok().flatten();
        if let Some(session) = &session {
            request.extensions_mut().insert(session.clone());
        }
        match tokio::time::timeout(Duration::from_secs(2), dispatch(request, next, session)).await {
            Ok(response) => response,
            Err(_) => (
                StatusCode::GATEWAY_TIMEOUT,
                "admin request deadline exceeded",
            )
                .into_response(),
        }
    } else {
        unauthorized(&state, session.ok().flatten().as_ref())
    };
    for (name, value) in [
        (
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'; form-action 'self'",
        ),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
    ] {
        response.headers_mut().insert(
            header::HeaderName::from_static(name),
            header::HeaderValue::from_static(value),
        );
    }
    response
}

#[derive(serde::Deserialize)]
struct Csrf {
    csrf: String,
}

async fn dispatch(request: Request, next: Next, session: Option<Arc<Session>>) -> Response {
    if request.method() != axum::http::Method::POST {
        return next.run(request).await;
    }
    let Some(session) = session else {
        return (StatusCode::FORBIDDEN, "CSRF session required").into_response();
    };
    let (parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 1024).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, "admin form exceeds limit").into_response();
    };
    let copy = Request::from_parts(parts.clone(), axum::body::Body::from(bytes.clone()));
    let form = Form::<Csrf>::from_request(copy, &()).await;
    if !form.is_ok_and(|Form(form)| session.csrf_valid(&form.csrf)) {
        return (StatusCode::FORBIDDEN, "CSRF token invalid or missing").into_response();
    }
    next.run(Request::from_parts(parts, axum::body::Body::from(bytes)))
        .await
}

async fn login_page(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    unauthorized(&state, session.as_ref().map(|Extension(s)| s))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    csrf: String,
    token: String,
}

async fn login(
    State(state): State<Arc<Context>>,
    Extension(previous): Extension<Arc<Session>>,
    form: Result<Form<Login>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(form)) = form else {
        return unauthorized(&state, Some(&previous));
    };
    if !previous.csrf_valid(&form.csrf) || !state.token.matches(&form.token) {
        return unauthorized(&state, Some(&previous));
    }
    // This listener is HTTP loopback only. Forwarded scheme headers are never
    // trusted. HTTPS transport, when supported, must also enable Secure here.
    let session = match state.sessions.issue(true, Some(&previous)) {
        Ok(session) => session,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Html(format!(
                    "admin session creation failed: {}",
                    escape(&error.to_string())
                )),
            )
                .into_response();
        }
    };
    let cookie = session.id.cookie(false);
    let Ok(cookie) = header::HeaderValue::from_str(&cookie) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "admin session header invalid",
        )
            .into_response();
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static("/"));
    response
}

#[cfg(test)]
mod tests;
