//! Maps HL7 laboratory results and outgoing orders without clocks, storage or transport.
use crate::inbound::{grouping, source};
use crate::{Error, Input, Output, Position};

mod departures;
mod display;
mod orders;
mod report;
pub use display::map as map_display;
pub use orders::{Command, map_order};
/// Selected HL7 syntax, incoming mappings and qualified machine departures.
pub struct Settings<'a> {
    /// Validated syntax and bounds.
    pub syntax: &'a analyte_config::Hl7Syntax,
    /// Explicit incoming translations.
    pub mappings: &'a analyte_config::Mappings,
    /// Evidence-backed declarations applied during mapping.
    pub declarations: &'a [analyte_config::MachineDeparture],
}

/// Maps ORU R01 or OUL R22 (2.5.1 only), with group-relative selectors.
/// # Errors
/// Rejects unsupported/misplaced segments rather than guessing group ownership.
/// A different protocol's selected settings cannot be passed here.
/// ```compile_fail
/// fn wrong(message: &analyte_hl7::Message, settings: &analyte_config::LawStack, mappings: &analyte_config::Mappings, input: analyte_mapping::Input<'_>) {
///     analyte_mapping::hl7::map(settings, message, input);
/// }
/// ```
pub fn map(
    settings: &Settings<'_>,
    message: &analyte_hl7::Message,
    input: Input<'_>,
) -> Result<Output, Error> {
    let Settings {
        syntax,
        mappings,
        declarations,
    } = settings;
    validate_message(message, syntax.encoding(), syntax.limits())?;
    map_checked(message, syntax, mappings, declarations, input)
}

pub(crate) fn validate_message(
    message: &analyte_hl7::Message,
    encoding: analyte_config::Encoding,
    settings: &analyte_config::SyntaxLimits,
) -> Result<(), Error> {
    message
        .require_complete_values()
        .map_err(|source| Error::Hl7 {
            position: Position {
                record: 0,
                field: 0,
            },
            source,
        })?;
    if !matches!(
        (encoding, message.charset()),
        (analyte_config::Encoding::Ascii, analyte_hl7::Charset::Ascii)
            | (
                analyte_config::Encoding::Latin1,
                analyte_hl7::Charset::Latin1
            )
            | (analyte_config::Encoding::Utf8, analyte_hl7::Charset::Utf8)
    ) {
        return Err(Error::CharsetMismatch);
    }
    if message.raw().len() > settings.bytes() || message.segments().count() > settings.records() {
        return Err(Error::Limit {
            position: Position {
                record: 0,
                field: 0,
            },
            resource: "message bytes or segments",
        });
    }
    Ok(())
}

fn map_checked(
    message: &analyte_hl7::Message,
    syntax: &analyte_config::Hl7Syntax,
    mappings: &analyte_config::Mappings,
    declarations: &[analyte_config::MachineDeparture],
    input: Input<'_>,
) -> Result<Output, Error> {
    let edition = syntax.edition();
    let mut source = source::hl7(message)?;
    let version = analyte_hl7::Msh::new(message)
        .map_err(|source| Error::Hl7 {
            position: Position {
                record: 1,
                field: 12,
            },
            source,
        })?
        .version();
    if analyte_hl7::Version::from(edition) != version {
        return Err(Error::Structure {
            position: Position {
                record: 1,
                field: 12,
            },
            reason: "message edition disagrees with validated configuration",
        });
    }
    source.attach_comments()?;
    let mut output = grouping::message(
        &source,
        &crate::RecordSettings {
            mappings,
            limits: syntax.limits(),
        },
        input,
        None,
    )?;
    report::qualify(message, &mut output)?;
    departures::apply(message, declarations, &mut output)?;
    Ok(output)
}
