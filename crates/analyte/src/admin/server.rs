use super::{Token, listener::Listener};
use crate::{Error, config::Program};
use analyte_model::Identifier;
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::{Semaphore, watch};

pub(super) struct Machine {
    pub(super) id: Identifier,
    pub(super) running: Arc<AtomicBool>,
}

pub(super) struct Context {
    token: Token,
    session: Token,
    host: String,
    origin: String,
    pub(super) store: Arc<analyte_store::Handle>,
    pub(super) machines: Vec<Machine>,
    pub(super) destinations: Vec<analyte_sink::Destination>,
    pub(super) directory: std::path::PathBuf,
    pub(super) reserve: u64,
}

pub(crate) struct Server {
    listener: Listener,
    app: Router,
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
    let session = Token::session()?;
    let socket = tokio::net::TcpListener::bind(address).await?;
    let host = address.to_string();
    let machines: Vec<_> = program
        .machines()
        .iter()
        .map(|m| Machine {
            id: m.id().clone(),
            running: Arc::new(AtomicBool::new(true)),
        })
        .collect();
    let owners = machines
        .iter()
        .map(|m| (m.id.as_str().to_owned(), m.running.clone()))
        .collect();
    let state = Arc::new(Context {
        token,
        session,
        origin: format!("http://{host}"),
        host,
        store,
        machines,
        destinations: crate::delivery::destinations(program, loopback)?,
        directory: program.data_directory().to_owned(),
        reserve: program.store_limits.reserve_bytes,
    });
    Ok(Some(Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state),
        owners,
    }))
}

impl Server {
    /// Shutdown stops admission and drains active requests; the service bounds and
    /// joins this owner. Cancellation can leave an already submitted read running.
    pub(crate) async fn run(self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(async move {
                crate::machine::stopping(&mut stop).await;
            })
            .await?;
        Ok(())
    }
}

fn router(state: Arc<Context>) -> Router {
    Router::new()
        .route("/", get(super::overview::show))
        .route("/login", post(login))
        .layer(DefaultBodyLimit::max(1024))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

const LOGIN: &str = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Analyte login</title><h1>Analyte admin</h1><form action=\"/login\" method=\"post\"><label>Token <input name=\"token\" type=\"password\" required autocomplete=\"off\" maxlength=\"44\"></label><button>Log in</button></form></html>";

fn unauthorized() -> Response {
    tracing::warn!("admin authentication refused");
    let mut response = (StatusCode::UNAUTHORIZED, Html(LOGIN)).into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    response
}

async fn protect(State(state): State<Arc<Context>>, request: Request, next: Next) -> Response {
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(state.host.as_str());
    let login = request.uri().path() == "/login" && request.method() == axum::http::Method::POST;
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
    let authenticated = bearer.is_some_and(|v| state.token.matches(v))
        || cookie.is_some_and(|v| state.session.matches(v));
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    let same_origin = origin.is_none_or(|v| v == state.origin);
    let mut response = if !valid_host || !same_origin {
        unauthorized()
    } else if login || authenticated {
        match tokio::time::timeout(Duration::from_secs(2), next.run(request)).await {
            Ok(response) => response,
            Err(_) => (
                StatusCode::GATEWAY_TIMEOUT,
                "admin request deadline exceeded",
            )
                .into_response(),
        }
    } else {
        unauthorized()
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
#[serde(deny_unknown_fields)]
struct Login {
    token: String,
}

async fn login(
    State(state): State<Arc<Context>>,
    form: Result<Form<Login>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(form)) = form else {
        return unauthorized();
    };
    if !state.token.matches(&form.token) {
        return unauthorized();
    }
    // This listener is HTTP loopback only. Forwarded scheme headers are never
    // trusted. HTTPS transport, when supported, must also enable Secure here.
    let cookie = state.session.cookie(false);
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
