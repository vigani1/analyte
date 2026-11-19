//! Routes authenticated local HTTP requests with session and request limits.
use super::{
    Token,
    listener::Listener,
    sessions::{Session, Sessions},
};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::installation::config::Program;
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
#[cfg(all(test, feature = "test-hooks"))]
mod tests;

pub(super) struct Machine {
    pub(super) id: Identifier,
    pub(super) running: Arc<AtomicBool>,
    /// The machine runtime's link light; the admin page only reads it.
    pub(super) status: watch::Receiver<crate::machine::link::status::Status>,
    pub(super) departure_html: Markup,
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
    /// Kept destinations no set-up machine uses, with their delivery-only worker's flag.
    pub(super) retired: super::Draining,
    pub(super) directory: std::path::PathBuf,
    pub(super) reserve: u64,
    pub(super) setup: super::setup::Setup,
    // Struct fields drop in declaration order: notify only after store is released.
    _released: tokio::sync::oneshot::Sender<()>,
}

pub(crate) struct Server {
    listener: Listener,
    app: Router,
    /// Set by the setup page's "Restart now".
    restart: watch::Receiver<bool>,
    released: tokio::sync::oneshot::Receiver<()>,
    pub(crate) owners: BTreeMap<String, Arc<AtomicBool>>,
}

pub(crate) async fn prepare(
    program: &Program,
    store: Arc<analyte_store::Handle>,
    mode: crate::runtime::hooks::ServiceMode,
    retired: &super::Draining,
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
                status: m.status.subscribe(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let owners = machines
        .iter()
        .map(|m| (m.id.as_str().to_owned(), m.running.clone()))
        .collect();
    let (released, release) = tokio::sync::oneshot::channel();
    #[cfg(feature = "test-hooks")]
    let seams = program
        .hooks
        .setup
        .clone()
        .unwrap_or_else(|| super::setup::Seams::system(&program.source_path));
    #[cfg(not(feature = "test-hooks"))]
    let seams = super::setup::Seams::system(&program.source_path);
    let setup = super::setup::Setup::new(program, seams);
    let restart = setup.restart_requested();
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
        destinations: crate::runtime::delivery::destinations(program, mode)?,
        retired: retired.clone(),
        directory: program.data_directory().to_owned(),
        reserve: program.store_limits.reserve_bytes,
        setup,
        _released: released,
    });
    Ok(Some(Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        restart,
        app: router(state),
        released: release,
        owners,
    }))
}

impl Server {
    /// Shutdown stops admission and drains active requests; the service bounds and
    /// joins this owner. Cancellation can leave an already submitted read running.
    /// A restart requested on the setup page drains the same way and then
    /// returns [`Error::RestartRequested`], so the service stops for its manager to
    /// start it again.
    pub(crate) async fn run(self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        let mut restart = self.restart.clone();
        drain_server(
            axum::serve(self.listener, self.app)
                .with_graceful_shutdown(async move {
                    let stopped = async {
                        let _ = stop.wait_for(|&stopped| stopped).await.map(|_| ());
                    };
                    tokio::pin!(stopped);
                    tokio::select! {
                        () = &mut stopped => {}
                        requested = async {
                            restart.wait_for(|&requested| requested).await.map(|_| ())
                        } => {
                            // A closed channel is not a request; keep serving until stop.
                            if requested.is_err() {
                                stopped.await;
                            }
                        }
                    }
                })
                .into_future(),
            self.released,
        )
        .await?;
        if *self.restart.borrow() {
            return Err(Error::RestartRequested);
        }
        Ok(())
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
        .route(super::view::STYLESHEET, get(super::view::stylesheet))
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
        .route("/analyzer-cancelled", get(super::awos::cancelled))
        .route("/awos/{order}/{item}", get(super::awos::detail))
        .route("/deliveries", get(super::actions::deliveries))
        .route(
            "/deliveries/{id}/resend",
            get(super::actions::confirm_resend).post(super::actions::resend),
        )
        .route(
            "/parked/{id}/reprocess",
            get(super::actions::confirm_reprocess).post(super::actions::reprocess),
        )
        .route(
            "/parked/{id}/resolve",
            get(super::actions::confirm_resolve).post(super::actions::resolve),
        )
        .route("/setup", get(super::setup::index))
        .merge(
            Router::new()
                .route("/setup/restart", post(super::setup::restart))
                .route("/setup/lab-apps", get(super::setup::lab_apps))
                .route(
                    "/setup/lab-apps/new",
                    get(super::setup::add_lab_app_form).post(super::setup::add_lab_app),
                )
                .route(
                    "/setup/lab-apps/{name}",
                    get(super::setup::edit_lab_app).post(super::setup::save_lab_app),
                )
                .route(
                    "/setup/lab-apps/{name}/remove",
                    get(super::setup::confirm_remove_lab_app).post(super::setup::remove_lab_app),
                )
                .route("/setup/listen", get(super::setup::listen_view))
                .route("/setup/listen/stop", post(super::setup::listen_stop))
                .route(
                    "/setup/add",
                    get(super::setup::add_form).post(super::setup::add),
                )
                .route(
                    "/setup/{id}",
                    get(super::setup::machine).post(super::setup::save),
                )
                .route("/setup/{id}/connection", get(super::setup::connection))
                .route("/setup/{id}/test", get(super::setup::test))
                .route("/setup/{id}/save", get(super::setup::review))
                .route("/setup/{id}/listen", post(super::setup::listen_start))
                .route(
                    "/setup/{id}/remove",
                    get(super::setup::confirm_remove).post(super::setup::remove),
                )
                // Runs after the outer limit below, so setup forms get the larger bound.
                .layer(DefaultBodyLimit::max(super::setup::FORM_LIMIT)),
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
            let html = super::view::login("Sign in to Analyte")
                .markup(concat!(
                    "<section><p class=\"lead\">Enter the admin key. It is in the file ",
                    "<code>admin-token</code> in Analyte's data folder on this PC; ask IT if you ",
                    "do not have it.</p><form ",
                    "action=\"/login\" method=\"post\"><input name=\"csrf\" ",
                ))
                .markup("type=\"hidden\" value=\"")
                .text(session.csrf.form_value())
                .markup(concat!(
                    "\"><label class=\"field\"><span title=\"Admin token\">Admin key</span><input name=\"token\" ",
                    "type=\"password\" required autocomplete=\"off\" ",
                ))
                .markup("maxlength=\"44\"></label><button>Sign in</button></form></section>")
                .markup(super::view::END);
            let mut response = (StatusCode::UNAUTHORIZED, Html(html)).into_response();
            if let Ok(value) = header::HeaderValue::from_str(&session.id.cookie(false)) {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => super::view::refusal(
            StatusCode::UNAUTHORIZED,
            "You cannot sign in right now",
            "Too many browser sessions are open. Wait a few minutes and try again, or restart Analyte.",
            error,
        ),
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
    // The stylesheet holds no data; the login page needs it before any session exists.
    let public = request.uri().path() == super::view::STYLESHEET;
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
        super::view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "Analyte needs a restart",
            "Analyte cannot check browser sessions any more. Restart Analyte, then sign in again.",
            "admin session lookup failed",
        )
    } else if !valid_host || !same_origin {
        unauthorized(&state, None)
    } else if login || public || authenticated {
        let session = session.ok().flatten();
        if let Some(session) = &session {
            request.extensions_mut().insert(session.clone());
        }
        match tokio::time::timeout(Duration::from_secs(2), dispatch(request, next, session)).await {
            Ok(response) => response,
            Err(_) => super::view::refusal(
                StatusCode::GATEWAY_TIMEOUT,
                "This took too long",
                concat!(
                    "Analyte did not finish in time. Check the page before trying again: ",
                    "an action may already have been saved.",
                ),
                "admin request deadline exceeded",
            ),
        }
    } else {
        unauthorized(&state, session.ok().flatten().as_ref())
    };
    for (name, value) in [
        (
            "content-security-policy",
            concat!(
                "default-src 'self'; frame-ancestors 'none'; ",
                "form-action 'self'",
            ),
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
        return form_expired("CSRF session required");
    };
    let limit = if request.uri().path().starts_with("/setup/") {
        super::setup::FORM_LIMIT
    } else {
        1024
    };
    let (parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, limit).await else {
        return super::view::refusal(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Too much text",
            "The form holds more text than Analyte accepts. Shorten what you typed and try again.",
            "admin form exceeds limit",
        );
    };
    let copy = Request::from_parts(parts.clone(), axum::body::Body::from(bytes.clone()));
    let form = Form::<Csrf>::from_request(copy, &()).await;
    if !form.is_ok_and(|Form(form)| session.csrf_valid(&form.csrf)) {
        return form_expired("CSRF token invalid or missing");
    }
    next.run(Request::from_parts(parts, axum::body::Body::from(bytes)))
        .await
}

/// The refusal for a form whose session check failed, usually an old page or a new sign-in.
fn form_expired(detail: &'static str) -> Response {
    super::view::refusal(
        StatusCode::FORBIDDEN,
        "This form has expired",
        "Go back, reload the page and try again. Nothing was changed.",
        detail,
    )
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
            return super::view::refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "You cannot sign in right now",
                "Too many browser sessions are open. Wait a few minutes and try again, or restart Analyte.",
                error,
            );
        }
    };
    let cookie = session.id.cookie(false);
    let Ok(cookie) = header::HeaderValue::from_str(&cookie) else {
        return super::view::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            "You cannot sign in right now",
            "Analyte could not start a browser session. Try again; if this keeps happening, give the details below to support.",
            "admin session header invalid",
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static("/"));
    response
}
