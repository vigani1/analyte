//! Maps HL7 laboratory results and outgoing orders without clocks, storage or transport.
use crate::inbound::{grouping, source};
use crate::{Error, Input, Output, Position, hl7_edition, is_law};
use analyte_config::Validated;
mod display;
mod orders;
mod report;
pub use display::map as map_display;
pub use orders::Command;
/// Maps ORU R01 or OUL R22 (2.5.1 only), with group-relative selectors.
/// # Errors
/// Rejects unsupported/misplaced segments rather than guessing group ownership.
pub fn map(
    message: &analyte_hl7::Message,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    if is_law(config) {
        return Err(Error::Syntax);
    }
    validate_message(message, config)?;
    map_checked(message, config, input)
}

pub(crate) fn validate_message(
    message: &analyte_hl7::Message,
    config: &Validated,
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
    if config
        .effective()
        .hl7_options()
        .is_none_or(|o| o.charset != message.charset())
    {
        return Err(Error::Syntax);
    }
    let settings = config.effective().stack().limits();
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
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    let edition = hl7_edition(config)?;
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
    let mut output = grouping::message(&source, config, input, None)?;
    report::qualify(message, &mut output)?;
    Ok(output)
}
