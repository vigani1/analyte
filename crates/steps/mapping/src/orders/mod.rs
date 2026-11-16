//! Maps outgoing orders without reversing incoming result tables.
mod app;
pub(crate) mod codes;
pub use crate::hl7::Command as Hl7Command;
pub use crate::hl7::map_display;
pub use crate::law::{Command as LawCommand, address as law_address, map_order as map_law_work};
use crate::orders::codes::map_specimen as outbound_specimen;
pub use app::map as app_order;

use analyte_config::Validated;
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
    /// A standard O record, including CR.
    Astm(Vec<u8>),
    /// One validated request per assay, in the app's order.
    Vidas(Vec<analyte_vidas::syntax::Message>),
    /// A complete validated host-initiated insertion.
    Autolyser(analyte_autolyser::syntax::Worklist),
}

impl MachineOrder {
    /// Borrows an ASTM record, refusing another protocol's payload.
    /// # Errors
    /// Returns Disabled when this is not an ASTM command.
    pub fn record(&self) -> Result<&[u8], OrderMappingError> {
        match self {
            Self::Astm(record) => Ok(record),
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
    /// No explicitly addressed cup can be encoded; zero is never invented.
    #[error("invalid-cup-position")]
    InvalidCupPosition,
    /// Autolyser requires an explicit addressed cup.
    #[error("missing-cup-position")]
    MissingCupPosition,
    /// An Autolyser field fails the documented fixed-width rules.
    #[error("Autolyser request is not representable: {0}")]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    /// A field exceeds its documented or installed ceiling.
    #[error("overlong-{0}")]
    Overlong(&'static str),
    /// Vendor syntax refuses the mapped request, without exposing patient content.
    #[error("VIDAS request is not representable: {0}")]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// No exact app-system and code match exists.
    #[error("unmapped test {0:?}")]
    UnmappedTest(NamespacedId),
    /// The app did not provide the required position.
    #[error("required specimen position was not supplied")]
    MissingPosition,
    /// Priority has no explicit translation.
    #[error("unmapped order priority")]
    UnmappedPriority,
    /// Input cannot be represented under the selected protocol repertoire and bounds.
    #[error("order cannot be represented within the configured protocol bounds")]
    Representation,
}

/// Maps every requested test before constructing complete protocol commands.
/// No wire output is returned for partial or ambiguous mappings.
/// # Errors
/// Refuses disabled capability, unmapped values, missing required context and wire bounds.
pub fn map_order(order: &AppOrder, config: &Validated) -> Result<MachineOrder, OrderMappingError> {
    use analyte_config::Stack;
    match config.effective().stack() {
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => crate::astm::map_order(
            order,
            s.syntax().limits(),
            s.orders().as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => crate::astm::map_order(
            order,
            s.syntax().limits(),
            s.orders().as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::VidasLiteral(s) => crate::vidas::map_order(
            order,
            s,
            s.orders().as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::Autolyser(s) => crate::autolyser::map_order(
            order,
            s,
            s.orders().as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | Stack::Hl7(analyte_config::RecordTransport::Folder(_))
        | Stack::Law(_)
        | Stack::Poct(_) => Err(OrderMappingError::Disabled),
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
    Ok(
        outbound_specimen(settings.specimen_types().as_deref(), requested)?
            .machine
            .as_str(),
    )
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
/// Mapping errors for initiating HL7/LAW work, retaining protocol-specific diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum WireMappingError {
    /// A specific input or capability refusal.
    #[error("{0}")]
    Context(&'static str),
    /// A specific unmapped-code refusal with the shared lookup cause.
    #[error("{context}")]
    Lookup {
        /// Stable protocol-specific diagnostic.
        context: &'static str,
        /// Specific shared mapping failure.
        #[source]
        source: OrderMappingError,
    },
    /// App evidence violates a common-model text bound.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// HL7 syntax rejected mapped content.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// HL7 order builder rejected mapped content.
    #[error(transparent)]
    Order(#[from] analyte_hl7::order::Error),
    /// LAW profile rejected mapped content.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Error),
}
impl WireMappingError {
    pub(super) fn lookup(context: &'static str, source: OrderMappingError) -> Self {
        Self::Lookup { context, source }
    }
}
