//! Constructs validated LAW protocol settings.
use super::{
    DialogTimeouts, Error, LawConnectionMode, LawPeer, LawQueryForm, LawWork, Mappings,
    OrderTables, SyntaxLimits, TcpTransport, invalid, limits, order_tables, tcp,
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
    /// Test, specimen and priority translations; the specimen table is always present.
    pub(super) tables: OrderTables,
    /// Required assigning namespace.
    pub(super) placer_namespace: Identifier,
    /// Permission for unsolicited work.
    pub(super) send_ahead: bool,
}

/// LAW over MLLP/TCP with explicit connection ownership.
#[derive(Debug, Clone)]
pub struct LawStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<TcpTransport, SyntaxLimits, Mappings>,
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
        settings: super::StackSettings {
            transport: tcp(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: limits(&machine.syntax),
            codes: machine
                .mappings
                .clone()
                .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
        },
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
                    tables: required_specimens(order_tables(orders, machine, path)?, path)?,
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

/// LAW work names every specimen (SPM-4), so the machine must declare its
/// specimen code system.
fn required_specimens(tables: OrderTables, path: &str) -> Result<OrderTables, Error> {
    if tables.specimen_system().is_none() {
        return Err(invalid(
            path,
            "orders.specimen_system",
            "LAW orders require the machine's specimen_system",
        ));
    }
    Ok(tables)
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
    /// Explicit test, specimen and priority mappings; LAW always has a specimen table.
    #[must_use]
    pub fn tables(&self) -> &OrderTables {
        &self.tables
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
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &LawPolicy {
        &self.policy
    }
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&LawOrders> {
        self.orders.as_ref()
    }
}

impl LawStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<TcpTransport, SyntaxLimits, Mappings> {
        &self.settings
    }
}
