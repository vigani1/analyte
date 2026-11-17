//! Maps LAW results and outgoing AWOS orders using validated protocol values.
use crate::inbound::{grouping, source};
use crate::{Error, Input, Output};

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
/// fn wrong(results: &analyte_hl7::law::Results<'_>, settings: &analyte_config::Hl7Syntax, input: analyte_mapping::Input) {
///     analyte_mapping::map_law(results, settings, input);
/// }
/// ```
pub fn map(
    results: &analyte_hl7::law::Results<'_>,
    settings: &analyte_config::LawStack,
    input: Input,
) -> Result<Output, Error> {
    crate::hl7::validate_message(results.message(), settings.options(), settings.limits())?;
    let mut source = source::law(results)?;
    source.attach_comments()?;
    let mut output = grouping::message(
        &source,
        &crate::RecordSettings {
            mappings: settings.mappings(),
            limits: settings.limits(),
        },
        input,
        Some(results),
    )?;
    status::map(results, &source, &mut output)?;
    Ok(output)
}
