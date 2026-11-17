//! Checks supported transport settings before opening machine connections.
use analyte_config::{Serial, Stack, StreamKind, StreamTransport, TcpRole};

/// Only stream protocols can be passed to a stream session.
#[derive(Clone, Copy)]
pub(super) enum StreamSession<'a> {
    Astm(&'a analyte_config::AstmStack),
    Hl7(&'a analyte_config::Hl7Stack),
    Law(&'a analyte_config::LawStack),
    Poct(&'a analyte_config::PoctStack),
    Vidas(&'a analyte_config::VidasStack),
    Autolyser(&'a analyte_config::AutolyserStack),
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

pub(super) fn transport(stack: &Stack) -> Link<'_> {
    match stack {
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => {
            stream(s.settings().transport(), StreamSession::Astm(s))
        }
        Stack::Vidas(s) => stream(s.settings().transport(), StreamSession::Vidas(s)),
        Stack::Autolyser(s) => stream(s.settings().transport(), StreamSession::Autolyser(s)),
        Stack::Hl7(analyte_config::RecordTransport::Stream(s)) => Link::Tcp {
            session: StreamSession::Hl7(s),
            role: s.settings().transport().role(),
            timeout_ms: s.settings().transport().operation_timeout_ms(),
        },
        Stack::Law(s) => Link::Tcp {
            session: StreamSession::Law(s),
            role: s.settings().transport().role(),
            timeout_ms: s.settings().transport().operation_timeout_ms(),
        },
        Stack::Poct(s) => Link::Tcp {
            session: StreamSession::Poct(s),
            role: s.settings().transport().role(),
            timeout_ms: s.settings().transport().operation_timeout_ms(),
        },
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => Link::Folder {
            timeout_ms: *s.settings().transport(),
            bytes: s.settings().syntax().limits().bytes(),
        },
        Stack::Hl7(analyte_config::RecordTransport::Folder(s)) => Link::Folder {
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
