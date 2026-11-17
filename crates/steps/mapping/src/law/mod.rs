//! Maps LAW results and outgoing AWOS orders using validated protocol values.
use crate::inbound::{grouping, source};
use crate::{Input, Output};

mod orders;
mod status;
pub use orders::{Command, address, map_order, map_work};
pub use status::{AvailabilityEstimate, StatusNotice};
/// Maps atomically validated LAB-29 through the shared field mapper and translations.
/// AWOS identity takes precedence; unsolicited work needs an explicit analyzer identity.
/// # Errors
/// Rejects mismatched configuration, unsupported mapping content or resource bounds.
/// A different protocol's selected settings cannot be passed here.
/// ```compile_fail
/// fn wrong(results: &analyte_hl7::law::Results<'_>, settings: &analyte_config::Hl7Syntax, input: analyte_mapping::Input<'_>) {
///     analyte_mapping::law::map(settings, results, input);
/// }
/// ```
pub fn map(
    settings: &analyte_config::LawStack,
    results: &analyte_hl7::law::Results<'_>,
    input: Input<'_>,
) -> Result<Output, Error> {
    crate::hl7::validate_message(
        results.message(),
        analyte_config::Encoding::Utf8,
        settings.settings().limits(),
    )?;
    let mut source = source::law(results)?;
    source.attach_comments()?;
    let mut output = grouping::message(
        &source,
        &crate::RecordSettings {
            mappings: settings.settings().codes(),
            limits: settings.settings().limits(),
        },
        input,
        Some(results),
    )?;
    status::map(results, &source, &mut output)?;
    Ok(output)
}

/// Specific outgoing LAW wire mapping failures.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// LAW order mappings absent.
    #[error("LAW order mappings absent")]
    MappingsAbsent,
    /// LAW outbound value cannot be represented without changing its identity.
    #[error("LAW outbound value cannot be represented without changing its identity")]
    UnrepresentableIdentity,
    /// LAW work empty.
    #[error("LAW work empty")]
    EmptyWork,
    /// LAW specimen type missing.
    #[error("LAW specimen type missing")]
    SpecimenTypeMissing,
    /// LAW query AWOS limit exceeded.
    #[error("LAW query AWOS limit exceeded")]
    QueryLimitExceeded,
    /// LAW item must cover one AWOS test.
    #[error("LAW item must cover one AWOS test")]
    ItemTestCount,
    /// LAW test components.
    #[error("LAW test components")]
    TestComponents,
    /// LAW specimen type unmapped.
    #[error("LAW specimen type unmapped")]
    UnmappedSpecimenType(#[source] crate::OrderMappingError),
    /// LAW priority unmapped.
    #[error("LAW priority unmapped")]
    UnmappedPriority(#[source] crate::OrderMappingError),
    /// LAW test unmapped.
    #[error("LAW test unmapped")]
    UnmappedTest(#[source] crate::OrderMappingError),
    /// App evidence violates a common-model text bound.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// HL7 syntax rejected mapped content.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW profile rejected mapped content.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::law::Error),
}

/// Specific incoming LAW mapping failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A validated LAW message lacks its header.
    #[error("LAW message header missing")]
    MissingHeader,
    /// A LAW notice references an order outside its original message.
    #[error("LAW notice order is not in the original message")]
    NoticeOrder,
    /// A one-based LAW notice position cannot identify a source record.
    #[error("LAW notice position is zero")]
    NoticePosition,
    /// Shared field mapping failed.
    #[error(transparent)]
    Mapping(#[from] crate::Error),
}
