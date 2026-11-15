//! Transport admission from supported stacks; serial settings cannot be absent.
use analyte_config::{Serial, Stack, StreamTransport, TcpRole};

pub(super) enum Link<'a> {
    Serial {
        settings: &'a Serial,
        timeout_ms: u32,
    },
    Tcp {
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
            Self::Serial { timeout_ms, .. }
            | Self::Tcp { timeout_ms, .. }
            | Self::Folder { timeout_ms, .. } => *timeout_ms,
        }
    }
}

pub(super) fn transport(stack: &Stack) -> Link<'_> {
    match stack {
        Stack::Astm(s) => stream(&s.transport),
        Stack::VidasLiteral(s) => stream(&s.transport),
        Stack::Autolyser(s) => stream(&s.transport),
        Stack::Hl7(s) => Link::Tcp {
            role: s.transport.role,
            timeout_ms: s.transport.operation_timeout_ms,
        },
        Stack::Law(s) => Link::Tcp {
            role: s.transport.role,
            timeout_ms: s.transport.operation_timeout_ms,
        },
        Stack::Poct(s) => Link::Tcp {
            role: s.transport.role,
            timeout_ms: s.transport.operation_timeout_ms,
        },
        Stack::FileAstm(s) => Link::Folder {
            timeout_ms: s.operation_timeout_ms,
            bytes: s.syntax.limits.bytes,
        },
        Stack::FileHl7(s) => Link::Folder {
            timeout_ms: s.operation_timeout_ms,
            bytes: s.syntax.limits.bytes,
        },
    }
}

fn stream(transport: &StreamTransport) -> Link<'_> {
    match transport {
        StreamTransport::Serial {
            settings,
            operation_timeout_ms,
        } => Link::Serial {
            settings,
            timeout_ms: *operation_timeout_ms,
        },
        StreamTransport::Tcp {
            role,
            operation_timeout_ms,
        } => Link::Tcp {
            role: *role,
            timeout_ms: *operation_timeout_ms,
        },
    }
}
