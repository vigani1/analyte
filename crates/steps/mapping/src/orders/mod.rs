//! Maps outgoing orders without reversing incoming result tables.
mod app;
pub(crate) mod codes;
pub use crate::hl7::map_display;
pub use crate::hl7::{Command as Hl7Command, map_order as map_hl7_order};
pub use crate::law::{
    Command as LawCommand, address as law_address, map_order as map_law_order,
    map_work as map_law_work,
};

pub use app::map as app_order;

use analyte_model::{NamespacedId, SampleId, SamplePosition};

/// Explicit app priority, translated only by the outbound priority table.
#[derive(Debug, Clone, Copy)]
pub enum AppOrderPriority {
    /// Routine work.
    Routine,
    /// Urgent work.
    Urgent,
    /// Immediate work.
    Stat,
}

impl AppOrderPriority {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Routine => "routine",
            Self::Urgent => "urgent",
            Self::Stat => "stat",
        }
    }
}

/// Host-adapted app order; identities and clinical strings are copied exactly.
pub struct AppOrder {
    /// Exact sample identifier, including case and leading zeros.
    pub sample: SampleId,
    /// Requested tests, each with its app assigning system.
    pub tests: Vec<NamespacedId>,
    /// Explicit priority from the app.
    pub priority: AppOrderPriority,
    /// Position supplied by the app, never invented by the bridge.
    pub position: Option<SamplePosition>,
    /// Optional exact patient identifier; the namespace is checked before encoding.
    pub patient: Option<NamespacedId>,
    /// Specimen type evidence, never substituted for origin metadata.
    pub specimen_type: Option<NamespacedId>,
}

/// Complete mapped commands; mapping has no transmission or reservation side effects.
pub enum MachineOrder {
    /// A validated standard O record; the protocol grammar owns encoding.
    Astm(analyte_astm::OrderRecord),
    /// One validated request per assay, in the app's order.
    Vidas(Vec<analyte_vidas::syntax::Message>),
    /// A complete validated host-initiated insertion.
    Autolyser(analyte_autolyser::syntax::Worklist),
}

impl MachineOrder {
    /// Encodes an ASTM record, refusing another protocol's payload.
    /// # Errors
    /// Returns Disabled when this is not an ASTM command.
    pub fn record(&self) -> Result<Vec<u8>, OrderMappingError> {
        match self {
            Self::Astm(record) => Ok(record.encode()),
            Self::Vidas(_) | Self::Autolyser(_) => Err(OrderMappingError::Disabled),
        }
    }
}

/// Specific mapping refusal; diagnostic details belong in protected order events.
#[derive(Debug, thiserror::Error)]
pub enum OrderMappingError {
    /// This machine has no outbound capability.
    #[error("outbound orders are disabled")]
    Disabled,
    /// VIDAS requires an explicit patient identifier.
    #[error("missing-patient")]
    MissingPatient,
    /// Patient assigning system does not match the configured system.
    #[error("patient-namespace-mismatch")]
    PatientNamespace,
    /// The supplied specimen type has no wire mapping.
    #[error("unmapped-specimen-type")]
    UnmappedSpecimenType,
    /// The machine requires a specimen medium supplied by the app.
    #[error("missing-specimen-type")]
    MissingSpecimenType,
    /// Patient identity exceeds the vendor field width.
    #[error("overlong-patient-id")]
    PatientIdTooLong,
    /// No exact app-system and code match exists.
    #[error("unmapped test {0}")]
    UnmappedTest(NamespacedId),
    /// The app did not provide the required position.
    #[error("required specimen position was not supplied")]
    MissingPosition,
    /// Priority has no explicit translation.
    #[error("unmapped order priority")]
    UnmappedPriority,
}

/// Protocol-specific refusal from dispatching an outgoing order.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    /// ASTM mapping refused the order.
    #[error(transparent)]
    Astm(#[from] crate::astm::OrderError),
    /// VIDAS mapping refused the order.
    #[error(transparent)]
    Vidas(#[from] crate::vidas::OrderError),
    /// AUTOLYSER mapping refused the order.
    #[error(transparent)]
    Autolyser(#[from] crate::autolyser::OrderError),
    /// HL7 mapping refused the order.
    #[error(transparent)]
    Hl7(#[from] crate::hl7::OrderError),
    /// Shared order evidence is unavailable.
    #[error(transparent)]
    Shared(#[from] OrderMappingError),
}

/// Maps every requested test before constructing complete protocol commands.
/// No wire output is returned for partial or ambiguous mappings.
/// # Errors
/// Refuses disabled capability, unmapped values, missing required context and wire bounds.
pub fn map_order(
    order: &AppOrder,
    stack: &analyte_config::Stack,
) -> Result<MachineOrder, DispatchError> {
    use analyte_config::Stack;
    match stack {
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => crate::astm::map_order(
            order,
            s.settings().syntax().limits(),
            s.orders().ok_or(OrderMappingError::Disabled)?,
        )
        .map(MachineOrder::Astm)
        .map_err(Into::into),
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => crate::astm::map_order(
            order,
            s.settings().syntax().limits(),
            s.orders().ok_or(OrderMappingError::Disabled)?,
        )
        .map(MachineOrder::Astm)
        .map_err(Into::into),
        Stack::Vidas(s) => {
            crate::vidas::map_order(order, s, s.orders().ok_or(OrderMappingError::Disabled)?)
                .map(MachineOrder::Vidas)
                .map_err(Into::into)
        }
        Stack::Autolyser(s) => {
            crate::autolyser::map_order(order, s, s.orders().ok_or(OrderMappingError::Disabled)?)
                .map(MachineOrder::Autolyser)
                .map_err(Into::into)
        }
        Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | Stack::Hl7(analyte_config::RecordTransport::Folder(_))
        | Stack::Law(_)
        | Stack::Poct(_) => Err(OrderMappingError::Disabled.into()),
    }
}

// One shared patient boundary for all outbound stacks with an unqualified field.
pub(super) fn map_patient<'a>(
    order: &'a AppOrder,
    namespace: &analyte_model::Identifier,
) -> Result<&'a str, OrderMappingError> {
    let patient = order
        .patient
        .as_ref()
        .ok_or(OrderMappingError::MissingPatient)?;
    if namespace != &patient.namespace {
        return Err(OrderMappingError::PatientNamespace);
    }
    Ok(patient.value.as_str())
}

pub(super) fn map_specimen<'a>(
    order: &AppOrder,
    settings: &'a analyte_config::OrderTables,
) -> Result<&'a str, OrderMappingError> {
    let requested = order
        .specimen_type
        .as_ref()
        .ok_or(OrderMappingError::MissingSpecimenType)?;
    Ok(codes::map_specimen(settings.specimen_types(), requested)?
        .machine
        .as_str())
}
/// Borrowed reservation identities and requested app codes; storage remains host-owned.
pub struct OutboundItem<'a> {
    /// Stable placer identity assigned once by the store.
    pub placer: &'a str,
    /// Explicit retained assigning namespace, if the protocol carries it.
    pub namespace: Option<&'a analyte_model::Identifier>,
    /// Tests covered by this immutable reservation item.
    pub tests: &'a [NamespacedId],
}
