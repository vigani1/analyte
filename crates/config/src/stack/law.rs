//! Validated law settings and construction at the config boundary.
use super::*;
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

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
pub(super) fn law_stack(machine: &MachineDocument, timeouts: DialogTimeouts) -> LawStack {
    let law = machine
        .law
        .as_ref()
        .expect("validated LAW stack requires policy");
    LawStack {
        transport: tcp(&machine.transport),
        frame_limit: machine.framing.limit,
        timeouts,
        acknowledgment: machine
            .dialog
            .acknowledgment
            .expect("validated MLLP dialog requires acknowledgment policy"),
        limits: limits(&machine.syntax),
        mappings: machine
            .mappings
            .clone()
            .expect("validated record stack requires mappings"),
        policy: LawPolicy {
            links: match law.connection_mode {
                LawConnectionMode::Persistent => LawLinks::Persistent,
                LawConnectionMode::IndependentPerTransaction => LawLinks::Independent {
                    outbound: law
                        .outbound_endpoint
                        .expect("validated independent LAW has an outbound endpoint"),
                },
            },
            query_forms: law.query_forms.clone(),
            peer: law.peer.clone(),
            query_deadline_ms: law.query_deadline_ms,
            response_ms: law.response_ms,
        },
        orders: machine.orders.as_ref().map(|orders| LawOrders {
            work: law
                .work
                .clone()
                .expect("validated LAW orders require work layout"),
            tests: orders.tests.clone(),
            priorities: orders.priorities.clone(),
            specimen_types: orders
                .specimen_types
                .clone()
                .expect("validated LAW orders require specimen mappings"),
            placer_namespace: orders
                .placer_namespace
                .clone()
                .expect("validated LAW orders require placer namespace"),
            send_ahead: orders.send_ahead,
        }),
    }
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
    /// Returns the validated links setting.
    #[must_use]
    pub fn links(&self) -> LawLinks {
        self.links
    }
    /// Returns the validated query forms setting.
    #[must_use]
    pub fn query_forms(&self) -> &[LawQueryForm] {
        &self.query_forms
    }
    /// Returns the validated peer setting.
    #[must_use]
    pub fn peer(&self) -> &LawPeer {
        &self.peer
    }
    /// Returns the validated query deadline ms setting.
    #[must_use]
    pub fn query_deadline_ms(&self) -> u32 {
        self.query_deadline_ms
    }
    /// Returns the validated response ms setting.
    #[must_use]
    pub fn response_ms(&self) -> u32 {
        self.response_ms
    }
}

impl LawOrders {
    /// Returns the validated work setting.
    #[must_use]
    pub fn work(&self) -> &LawWork {
        &self.work
    }
    /// Returns the validated tests setting.
    #[must_use]
    pub fn tests(&self) -> &Vec<OutboundTest> {
        &self.tests
    }
    /// Returns the validated specimen types setting.
    #[must_use]
    pub fn specimen_types(&self) -> &Vec<OutboundSpecimen> {
        &self.specimen_types
    }
    /// Returns the validated priorities setting.
    #[must_use]
    pub fn priorities(&self) -> &Vec<OrderPriority> {
        &self.priorities
    }
    /// Returns the validated placer namespace setting.
    #[must_use]
    pub fn placer_namespace(&self) -> &Identifier {
        &self.placer_namespace
    }
    /// Returns the validated send ahead setting.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        self.send_ahead
    }
}

impl LawStack {
    /// Returns the validated transport setting.
    #[must_use]
    pub fn transport(&self) -> &TcpTransport {
        &self.transport
    }
    /// Returns the validated frame limit setting.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Returns the validated timeouts setting.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Returns the validated acknowledgment setting.
    #[must_use]
    pub fn acknowledgment(&self) -> Responsibility {
        self.acknowledgment
    }
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated policy setting.
    #[must_use]
    pub fn policy(&self) -> &LawPolicy {
        &self.policy
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<LawOrders> {
        &self.orders
    }
}
