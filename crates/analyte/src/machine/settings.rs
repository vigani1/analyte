//! Transport admission from supported stacks; serial settings cannot be absent.
use analyte_config::{Serial, Stack, StreamTransport, TcpRole};

#[derive(Clone, Copy)]
pub(super) enum Session<'a> {
    Astm(&'a analyte_config::AstmStack),
    Hl7(&'a analyte_config::Hl7Stack),
    Law(&'a analyte_config::LawStack),
    Poct(&'a analyte_config::PoctStack),
    Vidas(&'a analyte_config::VidasStack),
    Autolyser(&'a analyte_config::AutolyserStack),
}

pub(super) struct SerialLink<'a> {
    pub(super) settings: &'a Serial,
    pub(super) session: Session<'a>,
    timeout_ms: u32,
}

pub(super) enum Link<'a> {
    Serial(SerialLink<'a>),
    Tcp {
        session: Session<'a>,
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
        Stack::Astm(s) => stream(s.transport(), Session::Astm(s)),
        Stack::VidasLiteral(s) => stream(s.transport(), Session::Vidas(s)),
        Stack::Autolyser(s) => stream(s.transport(), Session::Autolyser(s)),
        Stack::Hl7(s) => Link::Tcp {
            session: Session::Hl7(s),
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::Law(s) => Link::Tcp {
            session: Session::Law(s),
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::Poct(s) => Link::Tcp {
            session: Session::Poct(s),
            role: s.transport().role(),
            timeout_ms: s.transport().operation_timeout_ms(),
        },
        Stack::FileAstm(s) => Link::Folder {
            timeout_ms: s.operation_timeout_ms(),
            bytes: s.syntax().limits().bytes(),
        },
        Stack::FileHl7(s) => Link::Folder {
            timeout_ms: s.operation_timeout_ms(),
            bytes: s.syntax().limits().bytes(),
        },
    }
}

fn stream<'a>(transport: &'a StreamTransport, session: Session<'a>) -> Link<'a> {
    match transport {
        StreamTransport::Serial {
            settings,
            operation_timeout_ms,
        } => Link::Serial(SerialLink {
            settings,
            session,
            timeout_ms: *operation_timeout_ms,
        }),
        StreamTransport::Tcp {
            role,
            operation_timeout_ms,
        } => Link::Tcp {
            session,
            role: *role,
            timeout_ms: *operation_timeout_ms,
        },
    }
}
