//! Checks supported transport settings before opening machine connections.
use analyte_config::{Serial, Stack, StreamKind, StreamTransport, TcpRole};

pub(super) struct SerialLink<'a> {
    pub(super) settings: &'a Serial,
    pub(super) session: &'a Stack,
    timeout_ms: u32,
}

pub(super) enum Link<'a> {
    Serial(SerialLink<'a>),
    Tcp {
        session: &'a Stack,
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
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => stream(s.transport(), stack),
        Stack::Vidas(s) => stream(s.transport(), stack),
        Stack::Autolyser(s) => stream(s.transport(), stack),
        Stack::Hl7(analyte_config::RecordTransport::Stream(s)) => Link::Tcp {
            session: stack,
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::Law(s) => Link::Tcp {
            session: stack,
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::Poct(s) => Link::Tcp {
            session: stack,
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => Link::Folder {
            timeout_ms: s.operation_timeout_ms(),
            bytes: s.syntax().limits().bytes(),
        },
        Stack::Hl7(analyte_config::RecordTransport::Folder(s)) => Link::Folder {
            timeout_ms: s.operation_timeout_ms(),
            bytes: s.syntax().limits().bytes(),
        },
    }
}

fn stream<'a>(transport: &'a StreamTransport, session: &'a Stack) -> Link<'a> {
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
