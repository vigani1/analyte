//! Routes authenticated local HTTP requests with session and request limits.
use super::{
    Token,
    listener::{Lifetime, Listener},
    sessions::{Session, Sessions},
    setup::Step,
};
use crate::admin::Error;
use crate::admin::view::refusal::Refusal;
use crate::installation::config::Program;
use analyte_model::Identifier;
use axum::{
    Form, Router,
    extract::{ConnectInfo, DefaultBodyLimit, FromRequest, Request, State},
    http::{Method, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use std::{
    collections::BTreeMap,
    future::IntoFuture as _,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::{Semaphore, watch};
mod sign_in;
use sign_in::Attempt;
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
    /// Where pages load this build's static files from.
    pub(super) assets: super::view::assets::Root,
    /// What the Settings page shows from the loaded program.
    pub(super) installed: super::pages::Installed,
    /// How long a setup write may run: [`WRITE_BUDGET`]; tests shorten it.
    pub(super) write_budget: Duration,
    // Struct fields drop in declaration order: notify only after store is released.
    _released: tokio::sync::oneshot::Sender<()>,
}

impl Context {
    /// Where the admin pages are served, such as `http://127.0.0.1:4000`.
    pub(super) fn origin(&self) -> &str {
        &self.origin
    }
}

pub(crate) struct Server {
    listener: Listener,
    app: Router,
    /// Set by the setup page's "Restart now".
    restart: watch::Receiver<bool>,
    released: tokio::sync::oneshot::Receiver<()>,
    pub(crate) owners: BTreeMap<String, Arc<AtomicBool>>,
}

/// Prepares the admin server for `program`, or none when it has no admin
/// address; `shutdown` is how long the service may take to stop, which the
/// Settings page shows.
pub(crate) async fn prepare(
    program: &Program,
    store: Arc<analyte_store::Handle>,
    mode: crate::runtime::hooks::ServiceMode,
    retired: &super::Draining,
    shutdown: Duration,
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
        installed: super::pages::Installed::of(program, shutdown),
        write_budget: WRITE_BUDGET,
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
        // Each request can reach its connection's lifetime (setup writes extend it).
        let app = self.app.into_make_service_with_connect_info::<Lifetime>();
        drain_server(
            axum::serve(self.listener, app)
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
/// long": shorter than the connection's [`super::listener::LIFETIME`] by
/// [`ANSWER_TIME`]. The lifetime starts earlier, at accept, so the answer is
/// still written before the connection closes. A setup write has this long to
/// send and check its form, then [`WRITE_BUDGET`] in all.
pub(super) const REQUEST_BUDGET: Duration = Duration::from_millis(1750);

/// How long a setup write (any POST under `/setup/`: save, add, remove, a lab
/// app change, restart, the listen test) may run in all, once its form has
/// passed the session check; its connection stays open that long plus
/// [`ANSWER_TIME`]. It writes and flushes backups and new files, loads the whole
/// program again and writes an audit row. Measured on 2026-11-20 on a Windows
/// developer PC: 20 ms at most at rest and 114 ms at most while a build and
/// the setup tests ran beside it; loaded Windows CI runners went past 1.75 s.
/// The audit row alone may wait the store's own 5 s deadline
/// (`crate::deadline`), so 10 s leaves another 5 s for the files: over 40
/// times the slowest write measured here.
const WRITE_BUDGET: Duration = Duration::from_secs(10);

/// Kept after a budget ends to write its answer before the connection closes.
const ANSWER_TIME: Duration = Duration::from_millis(250);

fn router(state: Arc<Context>) -> Router {
    Router::new()
        .route("/", get(super::overview::show))
        .route("/static/{*path}", get(super::view::assets::serve))
        .route("/status", get(super::status::show))
        .route("/login", get(sign_in::login_page).post(sign_in::login))
        .route("/logout", post(sign_in::logout))
        .route(
            "/retention",
            get(super::pages::retention::show_page).post(super::pages::retention::hold),
        )
        .route("/settings", get(super::pages::settings))
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
            get(super::pages::confirm_resend).post(super::pages::resend),
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
        .fallback(not_found)
        // After every route, merged ones too: it is set on each route there is.
        .method_not_allowed_fallback(wrong_method)
        .layer(DefaultBodyLimit::max(1024))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

/// Any address without a page.
async fn not_found(State(state): State<Arc<Context>>) -> Response {
    Refusal::PageNotFound.page(&state.assets)
}

/// An address that takes other requests, such as opening Sign out as a page.
async fn wrong_method(State(state): State<Arc<Context>>, method: Method) -> Response {
    Refusal::WrongMethod(method).page(&state.assets)
}

async fn protect(State(state): State<Arc<Context>>, mut request: Request, next: Next) -> Response {
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(state.host.as_str());
    let login = request.uri().path() == "/login";
    // Static files hold no data; the login page needs them before any session exists.
    // `/status` says only the overall state in one word, for the desktop app's tray.
    let public = request.uri().path().starts_with("/static/") || request.uri().path() == "/status";
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
        Refusal::NeedsRestart.page(&state.assets)
    } else if !valid_host || !same_origin {
        sign_in::unauthorized(&state, None, Attempt::Open)
    } else if login || public || authenticated {
        let session = session.ok().flatten();
        if let Some(session) = &session {
            request.extensions_mut().insert(session.clone());
        }
        plain_refusal_as_page(&state.assets, answer(&state, request, next, session).await).await
    } else {
        let attempt = if request.method() == Method::POST {
            Attempt::Form
        } else {
            Attempt::Open
        };
        sign_in::unauthorized(&state, session.ok().flatten().as_ref(), attempt)
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

/// A POST under `/setup/`: a setup write. It gets the larger form limit and,
/// once its form has passed the session check, [`WRITE_BUDGET`].
fn setup_write(request: &Request) -> bool {
    request.method() == Method::POST && request.uri().path().starts_with("/setup/")
}

/// Runs an admitted request within its budget. Its form is read and checked
/// within [`REQUEST_BUDGET`], so a slow sender never gets more; a setup write
/// then has until [`Context::write_budget`] and keeps its connection open
/// until its answer is written. Every other request keeps [`REQUEST_BUDGET`].
async fn answer(
    state: &Context,
    request: Request,
    next: Next,
    session: Option<Arc<Session>>,
) -> Response {
    let started = tokio::time::Instant::now();
    let write = setup_write(&request);
    let lifetime = request
        .extensions()
        .get::<ConnectInfo<Lifetime>>()
        .map(|ConnectInfo(lifetime)| lifetime.clone());
    let checked = match tokio::time::timeout(REQUEST_BUDGET, checked(request, session)).await {
        Ok(Ok(request)) => request,
        Ok(Err(refusal)) => return refusal.page(&state.assets),
        Err(_) => return Refusal::TooLong.page(&state.assets),
    };
    let budget = if write {
        state.write_budget
    } else {
        REQUEST_BUDGET
    };
    let left = budget.saturating_sub(started.elapsed());
    if write && let Some(lifetime) = lifetime {
        lifetime.keep_open_for(left.saturating_add(ANSWER_TIME));
    }
    // Past the budget the work it started may still finish, so the page says
    // to look before trying again.
    tokio::time::timeout(left, next.run(checked))
        .await
        .unwrap_or_else(|_| Refusal::TooLong.page(&state.assets))
}

/// The most a plain-text refusal's words are read to show them for support.
const PLAIN_LIMIT: usize = 4096;

/// The web framework's own refusals of an address, a query or a form it could
/// not read are plain text; they leave as the refusal page with the same
/// status, keeping the framework's words for support. Every other answer
/// passes as it is.
async fn plain_refusal_as_page(assets: &super::view::assets::Root, response: Response) -> Response {
    let plain = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|kind| kind.as_bytes().starts_with(b"text/plain"));
    let status = response.status();
    if !plain || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let detail = match axum::body::to_bytes(response.into_body(), PLAIN_LIMIT).await {
        Ok(words) => String::from_utf8_lossy(&words).into_owned(),
        Err(error) => format!("its words could not be read within {PLAIN_LIMIT} bytes: {error}"),
    };
    Refusal::Unreadable { status, detail }.page(assets)
}

/// The request with its posted form read and the form's CSRF value checked
/// against the session, or the refusal; a request that posts nothing passes as it is.
async fn checked(request: Request, session: Option<Arc<Session>>) -> Result<Request, Refusal> {
    if request.method() != Method::POST {
        return Ok(request);
    }
    let Some(session) = session else {
        return Err(Refusal::FormExpired("CSRF session required"));
    };
    let limit = if setup_write(&request) {
        super::setup::FORM_LIMIT
    } else {
        1024
    };
    let (parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, limit).await else {
        return Err(Refusal::TooMuchText);
    };
    let copy = Request::from_parts(parts.clone(), axum::body::Body::from(bytes.clone()));
    let form = Form::<Csrf>::from_request(copy, &()).await;
    if !form.is_ok_and(|Form(form)| session.csrf_valid(&form.csrf)) {
        // Usually an old page or a new sign-in.
        return Err(Refusal::FormExpired("CSRF token invalid or missing"));
    }
    Ok(Request::from_parts(parts, axum::body::Body::from(bytes)))
}
