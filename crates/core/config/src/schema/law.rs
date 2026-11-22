//! Selects LAW options explicitly (TF-1 §5.2 and TF-2b 2.2/3.27).
use crate::{Encoding, Error, Escapes, ParsedMachine, Terminator, invalid};
use serde::{Deserialize, Serialize};

/// Supported host lookup scopes. Other LAW scopes must receive an error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LawQueryForm {
    /// Exact specimen/container identifier using the configured sample identity.
    Container,
    /// Pending work addressed to this analyzer, under the shared cache trust rule.
    AllPending,
}

/// Whether a LAW peer explicitly permits one enhanced-content capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "bool", into = "bool")]
pub enum PeerSupport {
    /// Peer did not grant this capability.
    Unsupported,
    /// Peer explicitly accepts this capability.
    Supported,
}

impl From<bool> for PeerSupport {
    fn from(supported: bool) -> Self {
        if supported {
            Self::Supported
        } else {
            Self::Unsupported
        }
    }
}

impl From<PeerSupport> for bool {
    fn from(support: PeerSupport) -> Self {
        match support {
            PeerSupport::Unsupported => false,
            PeerSupport::Supported => true,
        }
    }
}

/// Explicit peer support, with no implicit enhanced-content permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LawPeer {
    /// Peer accepts patient demographics.
    pub patient: PeerSupport,
    /// Peer accepts container characteristics.
    pub container: PeerSupport,
    /// Peer accepts specimen characteristics.
    pub specimen: PeerSupport,
    /// Peer accepts explicit dilution instructions.
    pub dilution: PeerSupport,
    /// Peer accepts app-authorized manager repeats.
    pub manager_repeat: PeerSupport,
    /// Peer accepts app-supplied repeat/reflex permissions.
    pub repeat_control: PeerSupport,
    /// Peer accepts explicitly mapped priority.
    pub priority: PeerSupport,
}

impl LawPeer {
    /// Supplies the existing LAW builders' peer-capability intersection.
    #[must_use]
    pub fn capabilities(&self) -> analyte_hl7::law::Capabilities {
        analyte_hl7::law::Capabilities {
            content: analyte_hl7::law::PeerContent {
                patient: self.patient.into(),
                container: self.container.into(),
                specimen: self.specimen.into(),
            },
            instructions: analyte_hl7::law::PeerInstructions {
                dilution: self.dilution.into(),
                priority: self.priority.into(),
                repeat: analyte_hl7::law::PeerRepeat {
                    manager_repeat: self.manager_repeat.into(),
                    repeat_control: self.repeat_control.into(),
                },
            },
        }
    }
}

/// Declared LAW link topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LawConnectionMode {
    /// One configured TCP client or server link carries LAB-27, LAB-28 and LAB-29.
    Persistent,
    /// Analyzer-initiated LAB-27/29 and independent manager-initiated LAB-28 links (B.6).
    IndependentPerTransaction,
}

/// Analyzer Manager policy, separate from generic HL7 and legacy display queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Law {
    /// Explicit link ownership; transport selects the persistent direction or inbound listener.
    pub connection_mode: LawConnectionMode,
    /// Analyzer listener for independent manager-initiated LAB-28, separate from the site listener.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbound_endpoint: Option<std::net::SocketAddr>,
    /// Explicit work layout; absence leaves query execution and work sending disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<LawWork>,
    /// Explicit supported lookup scopes; no positional or parent-ID inference.
    pub query_forms: Vec<LawQueryForm>,
    /// Peer-declared enhanced-content support, consumed by existing LAW builders.
    pub peer: LawPeer,
    /// First query byte through storage, lookup and reservation.
    pub query_deadline_ms: u32,
    /// LAB-28 application response deadline after issuance.
    pub response_ms: u32,
}

/// Explicit header and sample/specimen bindings for LAB-28.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LawWork {
    /// Sending application.
    pub application: analyte_model::Identifier,
    /// Sending facility.
    pub facility: analyte_model::Identifier,
    /// Receiving application.
    pub peer_application: analyte_model::Identifier,
    /// Receiving facility.
    pub peer_facility: analyte_model::Identifier,
    /// Explicit assigning authority when the app `SampleId` is used as a specimen ID.
    pub specimen_namespace: analyte_model::Identifier,
    /// Authority binding for qualified incoming container IDs. Outbound SAC uses EI.1 only.
    pub container_namespace: analyte_model::Identifier,
    /// Explicit patient-specimen role; QC authorization is not inferred.
    pub specimen_role: analyte_model::Identifier,
}

pub(crate) fn validate_orders(
    machine: &ParsedMachine<'_>,
    path: &str,
    orders: &crate::Orders,
) -> Result<(), Error> {
    if machine
        .law
        .as_ref()
        .and_then(|law| law.work.as_ref())
        .is_none()
    {
        return Err(invalid(
            path,
            "law.work",
            "LAW outbound mappings require an explicit query work layout",
        ));
    }
    if orders.query.is_some()
        || orders.hl7.is_some()
        || orders.hl7_send.is_some()
        || orders.astm.is_some()
        || orders.clone.is_some()
        || orders.patient_namespace.is_some()
        || orders.placer_namespace.as_ref().is_none_or(|namespace| {
            !analyte_hl7::is_safe_field(
                (namespace.as_str()).as_bytes(),
                18,
                analyte_hl7::FieldSyntax::Plain,
            )
        })
        || machine.mappings.as_ref().is_none_or(|mappings| {
            !analyte_hl7::is_safe_field(
                mappings.test_system.as_str().as_bytes(),
                12,
                analyte_hl7::FieldSyntax::Plain,
            )
        })
        || orders.specimen_system.as_ref().is_none_or(|system| {
            !analyte_hl7::is_safe_field(
                system.as_str().as_bytes(),
                12,
                analyte_hl7::FieldSyntax::Plain,
            )
        })
    {
        return Err(invalid(
            path,
            "orders",
            "LAW work requires a plain test_system and specimen_system of at most 12 bytes each and a placer namespace; legacy and patient layouts are disabled",
        ));
    }
    Ok(())
}

pub(crate) fn validate(machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    if machine.choice().allows_law_section() != machine.law.is_some() {
        return Err(invalid(
            path,
            "law",
            "supply LAW settings only for the law_manager dialog",
        ));
    }
    let Some(law) = &machine.law else {
        return Ok(());
    };
    validate_link_and_work(machine, law, path)?;
    if machine.mappings.as_ref().is_some_and(|mapping| {
        mapping.identity.correction_status_codes.len() != 1
            || mapping
                .identity
                .correction_status_codes
                .first()
                .is_none_or(|code| code.as_str() != "C")
    }) {
        return Err(invalid(
            path,
            "mappings.identity.correction_status_codes",
            "LAW corrections require exactly the explicit status C",
        ));
    }
    if machine.mappings.as_ref().is_some_and(|mapping| {
        let selects = |field: &crate::Field, group, record, number, component| {
            field.group == group
                && field.record.as_str() == record
                && field.field == number
                && field.component == component
                && field.repetition == 1
        };
        !selects(&mapping.identity.order_id, crate::Group::Order, "OBR", 2, 1)
            || match &mapping.identity.order_namespace {
                crate::OrderNamespace::Fixed { .. } => false,
                crate::OrderNamespace::Field { .. } => true,
            }
            || match &mapping.identity.sub_id {
                crate::SubId::Field(field) => {
                    !selects(field, crate::Group::Observation, "OBX", 4, 1)
                }
                crate::SubId::None(_) => true,
            }
            || !mapping.identity.run.is_empty()
    }) {
        return Err(invalid(
            path,
            "mappings.identity",
            "LAW uses OBR-2 AWOS and complete OBX-4 OG; use an explicit fixed AWOS namespace (OBR-2.2 is X) and no extra run selectors",
        ));
    }
    if machine
        .orders
        .as_ref()
        .is_some_and(|orders| orders.hl7.is_some() || orders.hl7_send.is_some())
    {
        return Err(invalid(
            path,
            "orders",
            "LAW cannot use legacy queries or non-LAW work messages",
        ));
    }
    Ok(())
}

fn validate_link_and_work(machine: &ParsedMachine<'_>, law: &Law, path: &str) -> Result<(), Error> {
    match law.connection_mode {
        LawConnectionMode::Persistent => {
            if law.outbound_endpoint.is_some() {
                return Err(invalid(
                    path,
                    "law.outbound_endpoint",
                    "a persistent link uses only the site endpoint",
                ));
            }
        }
        LawConnectionMode::IndependentPerTransaction => {
            if machine.transport.kind != crate::TransportKind::TcpServer {
                return Err(invalid(
                    path,
                    "law.connection_mode",
                    "independent LAW requires an inbound tcp_server for LAB-27/LAB-29",
                ));
            }
            if law.outbound_endpoint.is_none_or(|peer| {
                peer.port() == 0 || peer.ip().is_unspecified() || peer.ip().is_multicast()
            }) {
                return Err(invalid(
                    path,
                    "law.outbound_endpoint",
                    "independent LAW requires an explicit analyzer listener address and nonzero port",
                ));
            }
        }
    }

    if let Some(work) = &law.work
        && (machine.orders.is_none()
            || work.specimen_role.as_str() != "P"
            || [
                &work.application,
                &work.facility,
                &work.peer_application,
                &work.peer_facility,
                &work.specimen_namespace,
                &work.container_namespace,
            ]
            .into_iter()
            .any(|value| {
                !analyte_hl7::is_safe_field(
                    (value.as_str()).as_bytes(),
                    20,
                    analyte_hl7::FieldSyntax::Plain,
                )
            }))
    {
        return Err(invalid(
            path,
            "law.work",
            "provide explicit bounded headers, specimen/container namespaces, P role and outbound mappings",
        ));
    }
    if !machine
        .syntax
        .edition
        .is_some_and(|edition| analyte_hl7::Version::from(edition).law_edition())
        || machine.syntax.encoding != Encoding::Utf8
        || machine.syntax.terminator != Some(Terminator::Cr)
        || machine.syntax.escapes != Some(Escapes::Reject)
    {
        return Err(invalid(
            path,
            "syntax",
            "LAW requires HL7 2.5.1, UTF-8, CR segments and strict escapes",
        ));
    }
    if law.query_forms.is_empty()
        || law.query_forms.len() > 2
        || (law.query_forms.len() == 2 && law.query_forms.first() == law.query_forms.get(1))
    {
        return Err(invalid(
            path,
            "law.query_forms",
            "declare one or both supported query forms once",
        ));
    }
    if !(1..=60_000).contains(&law.query_deadline_ms) || !(1..=60_000).contains(&law.response_ms) {
        return Err(invalid(
            path,
            "law",
            "use query and response deadlines from 1 through 60000 milliseconds",
        ));
    }
    Ok(())
}
