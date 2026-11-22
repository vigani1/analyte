//! Answers protocol capability questions from the validated stack.
use super::{AstmOrders, Hl7Orders, LawStack, RecordTransport, Stack};

impl Stack {
    /// ASTM settings when this stack supports ASTM; otherwise absent.
    #[must_use]
    pub fn astm_orders(&self) -> Option<&AstmOrders> {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => s.orders(),
            Stack::Astm(RecordTransport::Folder(s)) => s.orders(),
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Record(_)
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }
    /// HL7 settings when this stack supports HL7; otherwise absent.
    #[must_use]
    pub fn hl7_orders(&self) -> Option<&Hl7Orders> {
        match self {
            Stack::Hl7(RecordTransport::Stream(s)) => s.orders(),
            Stack::Hl7(RecordTransport::Folder(s)) => s.orders(),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Record(_)
            | Stack::Astm(RecordTransport::Folder(_)) => None,
        }
    }
    /// LAW settings when this stack supports LAW; otherwise absent.
    #[must_use]
    pub fn law(&self) -> Option<&LawStack> {
        match self {
            Stack::Law(s) => Some(s),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Record(_)
            | Stack::Astm(RecordTransport::Folder(_))
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }
}

impl Stack {
    /// Returns the stable receipt driver namespace.
    #[must_use]
    pub fn driver_namespace(&self) -> &'static str {
        match self {
            Stack::Astm(RecordTransport::Stream(_)) | Stack::Astm(RecordTransport::Folder(_)) => {
                "analyte-astm"
            }
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Folder(_))
            | Stack::Law(_) => "analyte-hl7",
            Stack::Poct(_) => "analyte-poct",
            Stack::Vidas(_) => "analyte-vidas",
            Stack::Autolyser(_) => "analyte-autolyser",
            // A distinct namespace marks every record-only receipt: it was
            // kept, never read, and reprocessing refuses it (ADR 0034).
            Stack::Record(_) => crate::recording::RECORD_DRIVER,
        }
    }
}

/// Record grammar of a watched-folder stack's files.
#[derive(Clone, Copy)]
pub(crate) enum RecordKind {
    Astm,
    Hl7,
}

// Each row owns all static capability facts for one stack choice.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent protocol facts form one explicit capability row."
)]
struct Capabilities {
    has_astm_records: bool,
    has_hl7_records: bool,
    has_field_mappings: bool,
    allows_law_section: bool,
    has_hl7_acknowledgment: bool,
    has_astm_link: bool,
    has_vidas_assay_codes: bool,
    has_hl7_departures: bool,
    connection_context: bool,
    has_autolyser_orders: bool,
    file_record_kind: Option<RecordKind>,
    transports: TransportSupport,
}
enum TransportSupport {
    Stream,
    Tcp,
    Folder,
}
const ASTM_LIS02: Capabilities = Capabilities {
    has_astm_records: true,
    has_hl7_records: false,
    has_field_mappings: true,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: true,
    has_vidas_assay_codes: false,
    has_hl7_departures: false,
    connection_context: false,
    file_record_kind: None,
    transports: TransportSupport::Stream,
    has_autolyser_orders: false,
};
const HL7_V2: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: true,
    has_field_mappings: true,
    allows_law_section: false,
    has_hl7_acknowledgment: true,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: true,
    connection_context: false,
    file_record_kind: None,
    transports: TransportSupport::Tcp,
    has_autolyser_orders: false,
};
const FILE_ASTM: Capabilities = Capabilities {
    has_astm_records: true,
    has_hl7_records: false,
    has_field_mappings: true,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: false,
    connection_context: false,
    file_record_kind: Some(RecordKind::Astm),
    transports: TransportSupport::Folder,
    has_autolyser_orders: false,
};
const FILE_HL7: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: true,
    has_field_mappings: true,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: true,
    connection_context: false,
    file_record_kind: Some(RecordKind::Hl7),
    transports: TransportSupport::Folder,
    has_autolyser_orders: false,
};
const LAW_MANAGER: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: true,
    has_field_mappings: true,
    allows_law_section: true,
    has_hl7_acknowledgment: true,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: false,
    connection_context: false,
    file_record_kind: None,
    transports: TransportSupport::Tcp,
    has_autolyser_orders: false,
};
const POCT: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: false,
    has_field_mappings: false,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: false,
    connection_context: true,
    file_record_kind: None,
    transports: TransportSupport::Tcp,
    has_autolyser_orders: false,
};
const VIDAS_LITERAL: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: false,
    has_field_mappings: false,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: false,
    has_vidas_assay_codes: true,
    has_hl7_departures: false,
    connection_context: false,
    file_record_kind: None,
    transports: TransportSupport::Stream,
    has_autolyser_orders: false,
};
const AUTOLYSER: Capabilities = Capabilities {
    has_astm_records: false,
    has_hl7_records: false,
    has_field_mappings: false,
    allows_law_section: false,
    has_hl7_acknowledgment: false,
    has_astm_link: false,
    has_vidas_assay_codes: false,
    has_hl7_departures: false,
    connection_context: false,
    file_record_kind: None,
    transports: TransportSupport::Stream,
    has_autolyser_orders: true,
};
impl crate::StackChoice {
    const fn capabilities(self) -> &'static Capabilities {
        match self {
            Self::AstmLis02 => &ASTM_LIS02,
            Self::Hl7V2 => &HL7_V2,
            Self::FileAstm => &FILE_ASTM,
            Self::FileHl7 => &FILE_HL7,
            Self::LawManager => &LAW_MANAGER,
            Self::Poct => &POCT,
            Self::VidasLiteral => &VIDAS_LITERAL,
            Self::Autolyser => &AUTOLYSER,
        }
    }
    /// Returns the stack capability for has astm records.
    #[must_use]
    pub const fn has_astm_records(self) -> bool {
        self.capabilities().has_astm_records
    }
    /// Returns the stack capability for has hl7 records.
    #[must_use]
    pub const fn has_hl7_records(self) -> bool {
        self.capabilities().has_hl7_records
    }
    /// Returns the stack capability for has field mappings.
    #[must_use]
    pub const fn has_field_mappings(self) -> bool {
        self.capabilities().has_field_mappings
    }
    /// Returns the stack capability for allows law section.
    #[must_use]
    pub const fn allows_law_section(self) -> bool {
        self.capabilities().allows_law_section
    }
    /// Returns the stack capability for has hl7 acknowledgment.
    #[must_use]
    pub const fn has_hl7_acknowledgment(self) -> bool {
        self.capabilities().has_hl7_acknowledgment
    }
    /// Returns the stack capability for has astm link.
    #[must_use]
    pub const fn has_astm_link(self) -> bool {
        self.capabilities().has_astm_link
    }
    /// Returns the stack capability for has vidas assay codes.
    #[must_use]
    pub const fn has_vidas_assay_codes(self) -> bool {
        self.capabilities().has_vidas_assay_codes
    }
    /// Returns the stack capability for has hl7 departures.
    #[must_use]
    pub(crate) const fn has_hl7_departures(self) -> bool {
        self.capabilities().has_hl7_departures
    }
    /// Returns the stack capability for connection context.
    #[must_use]
    pub const fn connection_context(self) -> bool {
        self.capabilities().connection_context
    }
    pub(crate) const fn has_autolyser_orders(self) -> bool {
        self.capabilities().has_autolyser_orders
    }
    pub(crate) const fn file_record_kind(self) -> Option<RecordKind> {
        self.capabilities().file_record_kind
    }
    pub(crate) const fn supports_transport(self, kind: crate::TransportKind) -> bool {
        use crate::TransportKind;
        match self.capabilities().transports {
            TransportSupport::Stream => !matches!(kind, TransportKind::WatchedFolder),
            TransportSupport::Tcp => {
                matches!(kind, TransportKind::TcpClient | TransportKind::TcpServer)
            }
            TransportSupport::Folder => matches!(kind, TransportKind::WatchedFolder),
        }
    }
}

/// Validated source of HL7 parsing options, including the fixed LAW profile.
pub enum Hl7OptionsSource<'a> {
    /// Configurable HL7 grammar.
    Syntax(&'a super::Hl7Syntax),
    /// LAW fixes encoding, terminator and escapes and supplies resource limits.
    Law(&'a LawStack),
}
impl Stack {
    /// Borrows ASTM syntax for either stream or complete-file records.
    #[must_use]
    pub fn astm_syntax(&self) -> Option<&super::AstmSyntax> {
        match self {
            Self::Astm(RecordTransport::Stream(s)) => Some(s.settings().syntax()),
            Self::Astm(RecordTransport::Folder(s)) => Some(s.settings().syntax()),
            Self::Hl7(_)
            | Self::Law(_)
            | Self::Poct(_)
            | Self::Vidas(_)
            | Self::Autolyser(_)
            | Self::Record(_) => None,
        }
    }
    /// Borrows the HL7 parser option source when this grammar is supported.
    #[must_use]
    pub fn hl7_options_source(&self) -> Option<Hl7OptionsSource<'_>> {
        match self {
            Self::Hl7(RecordTransport::Stream(s)) => {
                Some(Hl7OptionsSource::Syntax(s.settings().syntax()))
            }
            Self::Hl7(RecordTransport::Folder(s)) => {
                Some(Hl7OptionsSource::Syntax(s.settings().syntax()))
            }
            Self::Law(s) => Some(Hl7OptionsSource::Law(s)),
            Self::Astm(_)
            | Self::Poct(_)
            | Self::Vidas(_)
            | Self::Autolyser(_)
            | Self::Record(_) => None,
        }
    }
    /// Borrows HL7 syntax and mappings for retained interpretation.
    #[must_use]
    pub fn hl7_record_settings(&self) -> Option<(&super::Hl7Syntax, &super::Mappings)> {
        match self {
            Self::Hl7(RecordTransport::Stream(s)) => {
                Some((s.settings().syntax(), s.settings().codes()))
            }
            Self::Hl7(RecordTransport::Folder(s)) => {
                Some((s.settings().syntax(), s.settings().codes()))
            }
            Self::Law(_)
            | Self::Astm(_)
            | Self::Poct(_)
            | Self::Vidas(_)
            | Self::Autolyser(_)
            | Self::Record(_) => None,
        }
    }
    /// Bounds durable custody by both the transport and protocol commit deadlines.
    #[must_use]
    pub fn commit_budget_ms(&self) -> u32 {
        let operation = self.operation_timeout_ms();
        match self {
            Self::Vidas(s) => operation.min(s.policy().commit_ms),
            Self::Autolyser(s) => operation.min(s.policy().storage_ms),
            Self::Record(s) => operation.min(match s.link() {
                crate::RecordLink::Astm(link) => link.commit_ms(),
                crate::RecordLink::Hl7(link) => link.wait_ms(),
                crate::RecordLink::Vidas(link) => link.commit_ms(),
                crate::RecordLink::Autolyser(link) => link.storage_ms(),
            }),
            Self::Astm(_) | Self::Hl7(_) | Self::Law(_) | Self::Poct(_) => operation,
        }
    }
    /// Names vendor interpretation diagnostics; record protocols need no prefix.
    #[must_use]
    pub const fn interpretation_prefix(&self) -> &'static str {
        match self {
            Self::Vidas(_) => "VIDAS interpretation: ",
            Self::Autolyser(_) => "Autolyser interpretation: ",
            Self::Poct(_) => "POCT interpretation: ",
            Self::Record(_) => "Record only: ",
            Self::Astm(_) | Self::Hl7(_) | Self::Law(_) => "",
        }
    }
}

impl Stack {
    /// Returns the capability row selected by this validated stack.
    #[must_use]
    pub const fn choice(&self) -> crate::StackChoice {
        use crate::StackChoice;
        match self {
            Self::Astm(RecordTransport::Stream(_)) => StackChoice::AstmLis02,
            Self::Astm(RecordTransport::Folder(_)) => StackChoice::FileAstm,
            Self::Hl7(RecordTransport::Stream(_)) => StackChoice::Hl7V2,
            Self::Hl7(RecordTransport::Folder(_)) => StackChoice::FileHl7,
            Self::Law(_) => StackChoice::LawManager,
            Self::Poct(_) => StackChoice::Poct,
            Self::Vidas(_) => StackChoice::VidasLiteral,
            Self::Autolyser(_) => StackChoice::Autolyser,
            // A record-only stack speaks its protocol's own link, so its
            // layers are that protocol's; it has none of its mappings.
            Self::Record(s) => match s.protocol() {
                crate::recording::CaptureProtocol::Astm => StackChoice::AstmLis02,
                crate::recording::CaptureProtocol::Hl7 => StackChoice::Hl7V2,
                crate::recording::CaptureProtocol::Vidas => StackChoice::VidasLiteral,
                crate::recording::CaptureProtocol::Autolyser => StackChoice::Autolyser,
            },
        }
    }
    /// Borrows stream ASTM settings for frame departure evidence.
    #[must_use]
    pub fn astm_link(&self) -> Option<&super::AstmStack> {
        match self {
            Self::Astm(RecordTransport::Stream(settings)) => Some(settings),
            Self::Astm(RecordTransport::Folder(_))
            | Self::Hl7(_)
            | Self::Law(_)
            | Self::Poct(_)
            | Self::Vidas(_)
            | Self::Autolyser(_)
            | Self::Record(_) => None,
        }
    }
}
