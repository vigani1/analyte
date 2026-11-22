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
    /// Requested tests, each a machine code in the machine's code system.
    pub tests: Vec<NamespacedId>,
    /// The lab app's name for each test, in the order of `tests`; `None`
    /// where it gave none. Never derived from the code.
    pub test_names: Vec<Option<analyte_model::SourceText>>,
    /// Explicit priority from the app.
    pub priority: AppOrderPriority,
    /// Position supplied by the app, never invented by the bridge.
    pub position: Option<SamplePosition>,
    /// Optional exact patient identifier; the namespace is checked before encoding.
    pub patient: Option<NamespacedId>,
    /// Specimen type evidence, never substituted for origin metadata.
    pub specimen_type: Option<NamespacedId>,
}

impl AppOrder {
    /// The lab app's name for `test`, from its first occurrence in `tests`.
    #[must_use]
    pub fn test_name(&self, test: &NamespacedId) -> Option<&analyte_model::SourceText> {
        self.tests
            .iter()
            .position(|t| t == test)
            .and_then(|index| self.test_names.get(index))
            .and_then(Option::as_ref)
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
    /// The order test is not in this machine's own code system (ADR 0029).
    #[error("test {0} is not in this machine's code system")]
    ForeignTest(NamespacedId),
    /// The app did not provide the required position.
    #[error("required specimen position was not supplied")]
    MissingPosition,
    /// Priority has no explicit translation.
    #[error("unmapped order priority")]
    UnmappedPriority,
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
/// Borrowed reservation identities and requested test codes; storage remains host-owned.
pub struct OutboundItem<'a> {
    /// Stable placer identity assigned once by the store.
    pub placer: &'a str,
    /// Explicit retained assigning namespace, if the protocol carries it.
    pub namespace: Option<&'a analyte_model::Identifier>,
    /// Tests covered by this immutable reservation item.
    pub tests: &'a [NamespacedId],
}
