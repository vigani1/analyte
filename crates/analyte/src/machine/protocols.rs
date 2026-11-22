//! Lists every protocol host: its link and session, worker error, receipt reader and recovery rule.
//!
//! Each concern is one exhaustive match, so a new protocol adds one arm to each
//! and the compiler names any it misses. Watched-folder stacks have no stream
//! session: they select `Link::Folder` here and are read like any receipt.
use super::{astm, autolyser, driver, hl7, law, poct, vidas};
use crate::intake::ConnectionEvidence;
use crate::machine::receive::custody::{Cleanup, CleanupFailure};
use crate::reading::{self, Error as ReadError};
use analyte_config::{
    RecordLink, RecordTransport, Serial, Stack, StreamKind, StreamTransport, TcpRole,
};
use analyte_config::{TcpTransport, Validated};
use analyte_mapping::{Input, Output};
use analyte_store::Handle;
use std::borrow::Cow;
use tokio::sync::watch;

/// Only stream protocols can be passed to a stream session.
#[derive(Clone, Copy)]
pub(super) enum StreamSession<'a> {
    Astm(&'a analyte_config::AstmStack),
    Hl7(&'a analyte_config::Hl7Stack),
    Law(&'a analyte_config::LawStack),
    Poct(&'a analyte_config::PoctStack),
    Vidas(&'a analyte_config::VidasStack),
    Autolyser(&'a analyte_config::AutolyserStack),
    /// Record only (ADR 0034): the protocol's link rules, nothing read or sent.
    Record(&'a analyte_config::RecordStack),
}

impl StreamSession<'_> {
    /// POCT, VIDAS and Autolyser journal every input chunk as a receipt that is never
    /// interpreted, and send their positive step only after interpretation (ADR 0010).
    /// An uninterpreted receipt there was never acknowledged, so the machine still
    /// holds the data and sends it again; startup recovery leaves it alone.
    const fn journals_chunks(self) -> bool {
        match self {
            Self::Astm(_) | Self::Hl7(_) | Self::Law(_) => false,
            // Record only journals every chunk and never interprets anything.
            Self::Poct(_) | Self::Vidas(_) | Self::Autolyser(_) | Self::Record(_) => true,
        }
    }
}

pub(super) struct SerialLink<'a> {
    pub(super) settings: &'a Serial,
    pub(super) session: StreamSession<'a>,
    timeout_ms: u32,
}

pub(super) enum Link<'a> {
    Serial(SerialLink<'a>),
    Tcp {
        session: StreamSession<'a>,
        role: TcpRole,
        timeout_ms: u32,
    },
    Folder {
        timeout_ms: u32,
        bytes: usize,
    },
}

impl Link<'_> {
    pub(super) fn timeout_ms(&self) -> u32 {
        match self {
            Self::Serial(link) => link.timeout_ms,
            Self::Tcp { timeout_ms, .. } | Self::Folder { timeout_ms, .. } => *timeout_ms,
        }
    }
}

/// Selects the link and protocol host of a validated stack.
pub(super) fn transport(stack: &Stack) -> Link<'_> {
    match stack {
        Stack::Astm(RecordTransport::Stream(s)) => {
            stream(s.settings().transport(), StreamSession::Astm(s))
        }
        Stack::Vidas(s) => stream(s.settings().transport(), StreamSession::Vidas(s)),
        Stack::Autolyser(s) => stream(s.settings().transport(), StreamSession::Autolyser(s)),
        Stack::Hl7(RecordTransport::Stream(s)) => {
            tcp(s.settings().transport(), StreamSession::Hl7(s))
        }
        Stack::Law(s) => tcp(s.settings().transport(), StreamSession::Law(s)),
        Stack::Poct(s) => tcp(s.settings().transport(), StreamSession::Poct(s)),
        Stack::Record(s) => stream(s.transport(), StreamSession::Record(s)),
        Stack::Astm(RecordTransport::Folder(s)) => Link::Folder {
            timeout_ms: *s.settings().transport(),
            bytes: s.settings().syntax().limits().bytes(),
        },
        Stack::Hl7(RecordTransport::Folder(s)) => Link::Folder {
            timeout_ms: *s.settings().transport(),
            bytes: s.settings().syntax().limits().bytes(),
        },
    }
}

fn stream<'a>(transport: &'a StreamTransport, session: StreamSession<'a>) -> Link<'a> {
    match transport.kind() {
        StreamKind::Serial(settings) => Link::Serial(SerialLink {
            settings,
            session,
            timeout_ms: transport.operation_timeout_ms(),
        }),
        StreamKind::Tcp(role) => Link::Tcp {
            session,
            role: *role,
            timeout_ms: transport.operation_timeout_ms(),
        },
    }
}

fn tcp<'a>(transport: &TcpTransport, session: StreamSession<'a>) -> Link<'a> {
    Link::Tcp {
        session,
        role: transport.role(),
        timeout_ms: transport.operation_timeout_ms(),
    }
}

/// Whether startup recovery must leave this stack's uninterpreted receipts alone.
pub(crate) fn journals_chunks(stack: &Stack) -> bool {
    match transport(stack) {
        Link::Serial(SerialLink { session, .. }) | Link::Tcp { session, .. } => {
            session.journals_chunks()
        }
        Link::Folder { .. } => false,
    }
}

/// Runs one stream session with the protocol host its stack selected.
pub(super) async fn session(
    machine: &crate::installation::config::Machine,
    selected: StreamSession<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let machine = &ConnectionEvidence::new(machine)?;
    let host = driver::Context { conn, stop };
    match selected {
        StreamSession::Astm(settings) => {
            Ok(driver::run(|| astm::Host::open(machine, settings, store), host).await?)
        }
        StreamSession::Hl7(settings) => {
            Ok(driver::run(|| hl7::Host::open(machine, settings, store), host).await?)
        }
        StreamSession::Law(settings) => {
            Ok(law::session(machine, settings, store, host.conn, host.stop).await?)
        }
        StreamSession::Poct(settings) => {
            let session = driver::Session::new(machine, settings, store, host.conn, host.stop);
            Ok(driver::serve(async { poct::Host::open(settings) }, session).await?)
        }
        StreamSession::Vidas(settings) => {
            let session = driver::Session::new(machine, settings, store, host.conn, host.stop);
            Ok(driver::serve(async { vidas::Host::open(settings) }, session).await?)
        }
        StreamSession::Autolyser(settings) => {
            let session = driver::Session::new(machine, settings, store, host.conn, host.stop);
            let open = autolyser::Host::open(machine, settings, store);
            Ok(driver::serve(open, session).await?)
        }
        StreamSession::Record(settings) => {
            let owner = machine.machine();
            let (conn, stop) = (host.conn, host.stop);
            match settings.link() {
                RecordLink::Astm(link) => {
                    let session = driver::Session::new(machine, settings, store, conn, stop);
                    let open = async { astm::recording::Host::open(owner, link) };
                    Ok(driver::serve(open, session).await?)
                }
                RecordLink::Hl7(link) => {
                    let session = driver::Session::new(machine, settings, store, conn, stop);
                    let open = async { hl7::recording::Host::open(link) };
                    Ok(driver::serve(open, session).await?)
                }
                RecordLink::Vidas(link) => {
                    let session = driver::Session::new(machine, settings, store, conn, stop);
                    let open = async { vidas::recording::Host::open(link) };
                    Ok(driver::serve(open, session).await?)
                }
                RecordLink::Autolyser(link) => {
                    let session = driver::Session::new(machine, settings, store, conn, stop);
                    let open = autolyser::recording::Host::open(owner, link, store);
                    Ok(driver::serve(open, session).await?)
                }
            }
        }
    }
}

/// Combines protocol and link failures at the machine worker orchestration boundary.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The ASTM session refused further processing.
    #[error(transparent)]
    Astm(Box<astm::Error>),
    /// The HL7 session refused further processing.
    #[error(transparent)]
    Hl7(Box<hl7::Error>),
    /// The LAW session refused further processing.
    #[error(transparent)]
    Law(Box<law::Error>),
    /// The POCT session refused further processing.
    #[error(transparent)]
    Poct(Box<poct::Error>),
    /// The VIDAS session refused further processing.
    #[error(transparent)]
    Vidas(Box<vidas::Error>),
    /// The AUTOLYSER session refused further processing.
    #[error(transparent)]
    Autolyser(Box<autolyser::Error>),
    /// Connection custody or retained evidence failed.
    #[error(transparent)]
    Evidence(super::receive::error::Error),
    /// A bounded synthetic gate stopped or could not resume the connection.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// Retains both the triggering failure and an independent cleanup failure.
    #[error(transparent)]
    Cleanup(Box<Cleanup<Self>>),
}

impl CleanupFailure for Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup(Box::new(Cleanup { original, effect }))
    }
}

/// Boxes each host's own failure into its worker variant.
macro_rules! host_errors {
    ($($variant:ident => $key:ident),+ $(,)?) => {$(
        impl From<$key::Error> for Error {
            fn from(error: $key::Error) -> Self {
                Self::$variant(Box::new(error))
            }
        }
    )+};
}
host_errors!(Astm => astm, Hl7 => hl7, Law => law, Poct => poct, Vidas => vidas, Autolyser => autolyser);

impl<C> From<C> for Error
where
    super::receive::error::Error: From<C>,
{
    fn from(cause: C) -> Self {
        Self::Evidence(super::receive::error::Error::from(cause))
    }
}

/// Returns exact syntax bytes from the configured stored receipt envelope,
/// selected from installed configuration without guessing from content.
/// # Errors
/// Returns framing and assembly defects without parsing a partial message.
pub(crate) fn payload<'a>(config: &Validated, raw: &'a [u8]) -> Result<Cow<'a, [u8]>, ReadError> {
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Folder(_))
        | Stack::Hl7(RecordTransport::Folder(_))
        | Stack::Poct(_)
        | Stack::Vidas(_)
        | Stack::Autolyser(_) => Ok(raw.into()),
        // A record-only receipt is a journaled chunk, kept and never read (ADR 0034).
        Stack::Record(_) => Err(ReadError::RecordedOnly),
        Stack::Astm(RecordTransport::Stream(s)) => Ok(analyte_framing::astm::decode(
            raw,
            astm::settings::frame_limit(s)?,
            astm::settings::checksum_policy(s),
            astm::settings::record_boundaries(s),
            s.settings().syntax().limits().bytes(),
        )?
        .into()),
        Stack::Hl7(RecordTransport::Stream(s)) => {
            Ok(analyte_framing::hl7::decode(raw, hl7::settings::frame_limit(s)?)?.into())
        }
        Stack::Law(s) => {
            Ok(analyte_framing::hl7::decode(raw, law::settings::frame_limit(s)?)?.into())
        }
    }
}

/// Reads retained original bytes through the same framing, syntax and mapping
/// policies as live intake, without I/O, orders, acknowledgments or storage
/// mutations. `hello` is the original durable POCT connection Hello, when available.
/// # Errors
/// Returns the original layer failure; never supplies a partial mapping on failure.
pub(crate) fn read(
    config: &Validated,
    input: Input<'_>,
    raw: &[u8],
    hello: Option<&[u8]>,
) -> Result<Output, ReadError> {
    let payload = payload(config, raw)?;
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Stream(s)) => {
            reading::astm(input, &payload, s.settings().syntax(), s.settings().codes())
        }
        Stack::Astm(RecordTransport::Folder(s)) => {
            reading::astm(input, &payload, s.settings().syntax(), s.settings().codes())
        }
        Stack::Hl7(RecordTransport::Stream(s)) => {
            let settings = s.settings();
            reading::hl7(config, input, &payload, settings.syntax(), settings.codes())
        }
        Stack::Hl7(RecordTransport::Folder(s)) => {
            let settings = s.settings();
            reading::hl7(config, input, &payload, settings.syntax(), settings.codes())
        }
        Stack::Law(s) => reading::law(input, &payload, s, config.effective().site()),
        Stack::Poct(s) => reading::poct(input, &payload, s, hello),
        Stack::Vidas(s) => reading::vidas(input, &payload, s),
        Stack::Autolyser(s) => reading::autolyser(input, &payload, s),
        Stack::Record(_) => Err(ReadError::RecordedOnly),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_errors_remain_distinct_at_worker_boundary() {
        let astm = Error::from(astm::Error::MessageBoundExceeded);
        assert!(
            matches!(astm, Error::Astm(error) if matches!(*error, astm::Error::MessageBoundExceeded))
        );
        let hl7 = Error::from(hl7::Error::MessageRefused);
        assert!(matches!(hl7, Error::Hl7(error) if matches!(*error, hl7::Error::MessageRefused)));
        let law = Error::from(law::Error::ConnectionDeadlineOverflow);
        assert!(
            matches!(law, Error::Law(error) if matches!(*error, law::Error::ConnectionDeadlineOverflow))
        );
        let poct = Error::from(poct::Error::MessageLimitExceeded);
        assert!(
            matches!(poct, Error::Poct(error) if matches!(*error, poct::Error::MessageLimitExceeded))
        );
        let vidas = Error::from(vidas::Error::ControlSuffixMismatch);
        assert!(
            matches!(vidas, Error::Vidas(error) if matches!(*error, vidas::Error::ControlSuffixMismatch))
        );
        let autolyser = Error::from(autolyser::Error::ReportReceiptMissing);
        assert!(
            matches!(autolyser, Error::Autolyser(error) if matches!(*error, autolyser::Error::ReportReceiptMissing))
        );
    }
    #[test]
    fn shared_link_and_custody_keep_concrete_recovery_causes() {
        let link = super::super::link::error::Error::from(analyte_transport::Error::Deadline(
            analyte_transport::DeadlineKind::WriteAll,
        ));
        assert!(matches!(
            link,
            super::super::link::error::Error::Transport(analyte_transport::Error::Deadline(
                analyte_transport::DeadlineKind::WriteAll
            ))
        ));
        let custody = super::super::receive::error::Error::from(
            analyte_store::IntakeError::Storage(analyte_store::StorageError::Capacity),
        );
        assert!(matches!(
            custody,
            super::super::receive::error::Error::Store(analyte_store::IntakeError::Storage(
                analyte_store::StorageError::Capacity
            ))
        ));
        let source = Error::from(std::io::Error::other("synthetic owner failure"));
        assert_eq!(source.to_string(), "synthetic owner failure");
    }
}
