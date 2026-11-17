//! Constructs validated LAW protocol settings.
use super::{
    DialogTimeouts, Encoding, Error, Escapes, LawConnectionMode, LawPeer, LawQueryForm, LawWork,
    Mappings, OrderPriority, OutboundSpecimen, OutboundTest, Responsibility, SyntaxLimits,
    TcpTransport, Terminator, hl7_options, invalid, limits, tcp,
};
use crate::ParsedMachine;
use analyte_model::Identifier;

/// Validated LAW connection ownership, with no absent independent destination.
#[derive(Debug, Clone, Copy)]
pub enum LawLinks {
    /// All transactions share the configured persistent link.
    Persistent,
    /// Separate inbound query/result links and a host-owned outbound work destination.
    Independent {
        /// Analyzer's LAB-28 listener.
        outbound: std::net::SocketAddr,
    },
}

/// LAW link policy; work is carried by the enabled order capability.
#[derive(Debug, Clone)]
pub struct LawPolicy {
    /// Validated ownership and destination.
    pub(super) links: LawLinks,
    /// Supported lookup scopes.
    pub(super) query_forms: Vec<LawQueryForm>,
    /// Explicit peer capabilities.
    pub(super) peer: LawPeer,
    /// Query storage, lookup and reservation deadline.
    pub(super) query_deadline_ms: u32,
    /// Application response deadline.
    pub(super) response_ms: u32,
}

/// Enabled LAW work always has a layout, namespace and specimen mappings.
#[derive(Debug, Clone)]
pub struct LawOrders {
    /// Required LAW work layout.
    pub(super) work: LawWork,
    /// Exact outbound test translations.
    pub(super) tests: Vec<OutboundTest>,
    /// Required specimen translations.
    pub(super) specimen_types: Vec<OutboundSpecimen>,
    /// Exact priority translations.
    pub(super) priorities: Vec<OrderPriority>,
    /// Required assigning namespace.
    pub(super) placer_namespace: Identifier,
    /// Permission for unsolicited work.
    pub(super) send_ahead: bool,
}

/// LAW over MLLP/TCP with explicit connection ownership.
#[derive(Debug, Clone)]
pub struct LawStack {
    /// TCP link.
    pub(super) transport: TcpTransport,
    /// Maximum MLLP payload bytes.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Required acknowledgment promise.
    pub(super) acknowledgment: Responsibility,
    /// LAW fixes HL7 2.5.1, UTF-8, CR endings and strict escapes.
    pub(super) limits: SyntaxLimits,
    /// Required field selectors and code tables.
    pub(super) mappings: Mappings,
    /// Required Analyzer Manager policy.
    pub(super) policy: LawPolicy,
    /// Explicit outbound capability; omission disables it.
    pub(super) orders: Option<LawOrders>,
}

pub(super) fn law_stack(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<LawStack, Error> {
    let law = machine
        .law
        .as_ref()
        .ok_or_else(|| invalid(path, "law", "LAW stack requires policy"))?;
    Ok(LawStack {
        transport: tcp(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        timeouts,
        acknowledgment: machine.dialog.acknowledgment.ok_or_else(|| {
            invalid(
                path,
                "dialog.acknowledgment",
                "MLLP dialog requires acknowledgment policy",
            )
        })?,
        limits: limits(&machine.syntax),
        mappings: machine
            .mappings
            .clone()
            .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        policy: LawPolicy {
            links: match law.connection_mode {
                LawConnectionMode::Persistent => LawLinks::Persistent,
                LawConnectionMode::IndependentPerTransaction => LawLinks::Independent {
                    outbound: law.outbound_endpoint.ok_or_else(|| {
                        invalid(
                            path,
                            "law.outbound_endpoint",
                            "independent LAW has an outbound endpoint",
                        )
                    })?,
                },
            },
            query_forms: law.query_forms.clone(),
            peer: law.peer.clone(),
            query_deadline_ms: law.query_deadline_ms,
            response_ms: law.response_ms,
        },
        orders: machine
            .orders
            .as_ref()
            .map(|orders| {
                Ok::<_, Error>(LawOrders {
                    work: law.work.clone().ok_or_else(|| {
                        invalid(path, "law.work", "LAW orders require work layout")
                    })?,
                    tests: orders.tests.clone(),
                    priorities: orders.priorities.clone(),
                    specimen_types: orders.specimen_types.clone().ok_or_else(|| {
                        invalid(
                            path,
                            "orders.specimen_types",
                            "LAW orders require specimen mappings",
                        )
                    })?,
                    placer_namespace: orders.placer_namespace.clone().ok_or_else(|| {
                        invalid(
                            path,
                            "orders.placer_namespace",
                            "LAW orders require placer namespace",
                        )
                    })?,
                    send_ahead: orders.send_ahead,
                })
            })
            .transpose()?,
    })
}

impl LawStack {
    /// Supplies LAW's fixed HL7 2.5.1 syntax policy and configured resource ceilings.
    #[must_use]
    pub fn options(&self) -> analyte_hl7::Options {
        hl7_options(
            Encoding::Utf8,
            Terminator::Cr,
            Escapes::Reject,
            &self.limits,
        )
    }
}

impl LawPolicy {
    /// Validated directional LAW links and their endpoint roles.
    #[must_use]
    pub fn links(&self) -> LawLinks {
        self.links
    }
    /// Query forms this peer explicitly supports.
    #[must_use]
    pub fn query_forms(&self) -> &[LawQueryForm] {
        &self.query_forms
    }
    /// Declared peer capabilities; unsupported exchanges are refused.
    #[must_use]
    pub fn peer(&self) -> &LawPeer {
        &self.peer
    }
    /// Maximum host-query wait in milliseconds.
    #[must_use]
    pub fn query_deadline_ms(&self) -> u32 {
        self.query_deadline_ms
    }
    /// Maximum time allowed for a protocol response, in milliseconds.
    #[must_use]
    pub fn response_ms(&self) -> u32 {
        self.response_ms
    }
}

impl LawOrders {
    /// LAW work-handling policy validated against peer capabilities.
    #[must_use]
    pub fn work(&self) -> &LawWork {
        &self.work
    }
    /// Explicit application-to-instrument test-code mapping.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Explicit specimen-code mappings; unknown specimens remain unresolved.
    #[must_use]
    pub fn specimen_types(&self) -> &Vec<OutboundSpecimen> {
        &self.specimen_types
    }
    /// Explicit order-priority mapping.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
    /// Assigning system required for retained placer order identities.
    #[must_use]
    pub fn placer_namespace(&self) -> &Identifier {
        &self.placer_namespace
    }
    /// Work-list settings; absence does not authorize unsolicited orders.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}

impl LawStack {
    /// Physical link settings, with unsupported protocol/link combinations rejected at load time.
    #[must_use]
    pub fn transport(&self) -> &TcpTransport {
        &self.transport
    }
    /// Maximum retained frame size before framing refuses more bytes.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Bounded protocol waits used by the connection state machine.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Acknowledgment responsibility promised only after durable custody.
    #[must_use]
    pub fn acknowledgment(&self) -> Responsibility {
        self.acknowledgment
    }
    /// Size and count ceilings applied before parsing untrusted records.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Explicit source-to-common-model mappings; unknown codes are not guessed.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &LawPolicy {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> &Option<LawOrders> {
        &self.orders
    }
}

impl LawStack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.orders().as_ref().is_some_and(LawOrders::send_ahead)
    }
}
