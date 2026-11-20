//! Routes authenticated local HTTP requests with session and request limits.
use super::{
    Token,
    listener::Listener,
    sessions::{Session, Sessions},
    setup::Step,
};
use crate::admin::Error;
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
    /// The departures its settings switch on, read once at startup.
    pub(super) rules: Vec<super::machines::Rule>,
    /// The running settings version: the digest of its effective configuration.
    pub(super) digest: [u8; 32],
    /// The model's name for people, read once at startup from its definition
    /// (`setup::model_name`); none when the definition gives none.
    pub(super) model: Option<String>,
    /// How it is attached, as its site file says.
    pub(super) connection: Connection,
    /// The lab app it sends to.
    pub(super) lab_app: Identifier,
}

/// How a set-up machine is attached, as configured: what the overview can name.
/// It never holds a peer's address (safety rule 10 keeps endpoints off pages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Connection {
    /// A serial port named in the site file, such as COM3.
    Serial(String),
    /// The serial port of the USB adapter reporting this serial number.
    UsbAdapter(String),
    /// Analyte listens and the machine connects; the port when the site file
    /// writes a plain socket address.
    Listening(Option<u16>),
    /// Analyte connects to the machine.
    Dialing,
    /// Analyte watches a folder; its last path part.
    Folder(String),
}

impl Connection {
    /// The connection of a machine with `link` at the site's `endpoint`.
    pub(super) fn of(
        link: crate::machine::link::status::Link,
        endpoint: &analyte_config::Endpoint,
    ) -> Self {
        use crate::machine::link::status::Link;
        let fixed = endpoint.fixed().unwrap_or_default();
        match (link, endpoint) {
            (Link::Serial, analyte_config::Endpoint::UsbSerial(serial)) => {
                Self::UsbAdapter(serial.as_str().to_owned())
            }
            (Link::Serial, analyte_config::Endpoint::Fixed(port)) => Self::Serial(port.clone()),
            (Link::TcpServer, _) => Self::Listening(
                fixed
                    .parse::<std::net::SocketAddr>()
                    .ok()
                    .map(|address| address.port()),
            ),
            (Link::TcpClient, _) => Self::Dialing,
            (Link::Folder, _) => Self::Folder(std::path::Path::new(fixed).file_name().map_or_else(
                || fixed.to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )),
        }
    }
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
    /// Where restyled pages load this build's static files from.
    pub(super) assets: super::view::assets::Root,
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
    #[cfg(feature = "test-hooks")]
    let seams = program
        .hooks
        .setup
        .clone()
        .unwrap_or_else(|| super::setup::Seams::system(&program.source_path));
    #[cfg(not(feature = "test-hooks"))]
    let seams = super::setup::Seams::system(&program.source_path);
    let machines: Vec<_> = program
        .machines()
        .iter()
        .map(|m| {
            let effective = m.snapshot().effective();
            Ok(Machine {
                rules: super::machines::rules(&m.snapshot)?,
                digest: m.snapshot().digest(),
                id: m.id().clone(),
                running: Arc::new(AtomicBool::new(true)),
                status: m.status.subscribe(),
                // A small bounded README read per machine, once, before serving.
                model: super::setup::model_name(&m.definition, seams.definitions.as_deref()),
                connection: Connection::of(
                    crate::machine::link::status::Link::of(effective.stack()),
                    &effective.site().endpoint,
                ),
                lab_app: effective.site().destination.name.clone(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let owners = machines
        .iter()
        .map(|m| (m.id.as_str().to_owned(), m.running.clone()))
        .collect();
    let (released, release) = tokio::sync::oneshot::channel();
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
        assets: super::view::assets::Root::of_this_build(),
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

/// How long one request may run before it is answered with "This took too
/// long": shorter than the connection's [`super::listener::LIFETIME`], which
/// starts earlier, at accept, so the answer is still written before the
/// connection closes.
const REQUEST_BUDGET: Duration = Duration::from_millis(1750);

fn router(state: Arc<Context>) -> Router {
    Router::new()
        .route("/", get(super::overview::show))
        .route("/static/{*path}", get(super::view::assets::serve))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route(
            "/retention",
            get(super::pages::retention::show).post(super::pages::retention::hold),
        )
        .route("/machines", get(super::machines::list))
        .route("/machines/{id}", get(super::machines::detail))
        .route(
            "/receipts/{id}/departures",
            get(super::machines::departures::receipt),
        )
        .route("/parked", get(super::parked::list))
        .route("/parked/{id}", get(super::parked::item))
        .route("/parked/{id}/raw", post(super::parked::raw))
        .route("/orders", get(super::pages::orders))
        .route("/analyzer-cancelled", get(super::pages::awos::cancelled))
        .route("/awos/{order}/{item}", get(super::pages::awos::detail))
        .route("/deliveries", get(super::pages::deliveries))
        .route(
            "/deliveries/{id}/resend",
            get(super::actions::confirm_resend).post(super::actions::resend),
        )
        .route(
            "/parked/{id}/reprocess",
            get(super::parked::confirm_reprocess).post(super::parked::reprocess),
        )
        .route(
            "/parked/{id}/resolve",
            get(super::parked::confirm_resolve).post(super::parked::resolve),
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
                .route("/setup/first", get(super::setup::first))
                .route("/setup/listen", get(super::setup::listen_view))
                .route("/setup/listen/stop", post(super::setup::listen_stop))
                .route(
                    "/setup/add",
                    get(super::setup::add_form).post(super::setup::add),
                )
                .route(
                    "/setup/{id}",
                    super::setup::step(Step::Machine).post(super::setup::save),
                )
                .route(
                    "/setup/{id}/connection",
                    super::setup::step(Step::Connection),
                )
                .route("/setup/{id}/test", super::setup::step(Step::Test))
                .route("/setup/{id}/save", super::setup::step(Step::Save))
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
    // Static files hold no data; the login page needs them before any session exists.
    let public = request.uri().path().starts_with("/static/");
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
        match tokio::time::timeout(REQUEST_BUDGET, dispatch(request, next, session)).await {
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
        // Not "no-referrer": with it, Chrome and Edge send `Origin: null` on every
        // form post, which the same-origin check above refuses, so no form could
        // be sent from those browsers. "same-origin" keeps the real origin on our
        // own posts and still sends nothing to any other site.
        ("referrer-policy", "same-origin"),
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
    ] {
        let name = header::HeaderName::from_static(name);
        // A versioned static file keeps its own long cache; everything else is not stored.
        if name == header::CACHE_CONTROL && response.headers().contains_key(&name) {
            continue;
        }
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
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

/// POST /logout: the dispatcher already checked the session's CSRF token. Ends
/// the session, clears its cookie and returns to the sign-in page. Sign-in is
/// not audited, so sign-out is not either.
async fn logout(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
) -> Response {
    if let Err(error) = state.sessions.end(&session) {
        return super::view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "You cannot sign out right now",
            "Analyte cannot check browser sessions any more. Restart Analyte; that signs everyone out.",
            error,
        );
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_static(Token::CLEARED_COOKIE),
    );
    response
        .headers_mut()
        .insert(header::LOCATION, header::HeaderValue::from_static("/login"));
    response
}
