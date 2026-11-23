//! Maps HL7 laboratory results and outgoing orders without clocks, storage or transport.
use crate::inbound::{grouping, source};
use crate::{Error, Input, Output, Position};

pub(crate) mod categorized;
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
    let msh = analyte_hl7::Msh::new(message).map_err(|source| Error::Hl7 {
        position: Position {
            record: 1,
            field: 12,
        },
        source,
    })?;
    let category = msh.message_category();
    let layout = declarations.iter().find(|rule| {
        matches!(
            rule,
            analyte_config::MachineDeparture::Hl7CategorizedOruResults(_)
        )
    });
    let patient = category == Some(analyte_hl7::MessageCategory::Patient) && layout.is_some();
    if category == Some(analyte_hl7::MessageCategory::Calibration)
        || (category == Some(analyte_hl7::MessageCategory::QualityControl) && layout.is_none())
    {
        return Err(Error::Structure {
            position: Position {
                record: 1,
                field: 16,
            },
            reason: "no interpretation is configured for this MSH-16 result category",
        });
    }
    let version = msh.version();
    let mut source = source::hl7(message)?;
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
    if let Some(layout) =
        layout.filter(|_| category == Some(analyte_hl7::MessageCategory::QualityControl))
    {
        return categorized::controls(message, &source, mappings, input, layout);
    }
    let mut output = grouping::message(
        &source,
        &crate::RecordSettings {
            absent_astm_processing: false,
            mappings,
            limits: syntax.limits(),
        },
        input,
        None,
        patient,
    )?;
    if let Some(layout) = layout.filter(|_| patient) {
        categorized::record_application(message, layout, &mut output)?;
    }
    report::qualify(message, &mut output)?;
    departures::apply(message, declarations, &mut output)?;
    Ok(output)
}

/// Specific outgoing HL7 mapping refusals.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// HL7 display line violates repertoire, width or record limits.
    #[error("HL7 display line violates repertoire, width or record limits")]
    DisplayLineBounds,
    /// HL7 display contains no lines.
    #[error("HL7 display contains no lines")]
    DisplayEmpty,
    /// Shared order evidence is unavailable or unmapped.
    #[error(transparent)]
    Shared(#[from] crate::OrderMappingError),
}

/// Specific outgoing HL7 wire mapping failures.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// The order requests data this destination cannot carry.
    #[error(transparent)]
    Shared(#[from] crate::OrderMappingError),
    /// Requested priority has no supported wire representation.
    #[error("HL7-unsupported-priority")]
    PriorityUnsupported,
    /// HL7-patient-name-required-but-not-carried-by-contract.
    #[error("HL7-patient-name-required-but-not-carried-by-contract")]
    PatientNameRequired,
    /// HL7 send config absent.
    #[error("HL7 send config absent")]
    SendSettingsAbsent,
    /// HL7-app-order-id-too-long-for-ORC-4.
    #[error("HL7-app-order-id-too-long-for-ORC-4")]
    GroupIdTooLong,
    /// HL7-item-has-no-placer-namespace.
    #[error("HL7-item-has-no-placer-namespace")]
    PlacerNamespaceMissing,
    /// HL7-send-item-must-cover-one-test.
    #[error("HL7-send-item-must-cover-one-test")]
    ItemTestCount,
    /// HL7-patient-namespace-mismatch.
    #[error("HL7-patient-namespace-mismatch")]
    PatientNamespaceMismatch,
    /// HL7-order-edition-rules-not-implemented.
    #[error("HL7-order-edition-rules-not-implemented")]
    EditionUnsupported,
    /// HL7-missing-specimen-type.
    #[error("HL7-missing-specimen-type")]
    SpecimenTypeMissing,
    /// HL7-test-not-in-machine-code-system.
    #[error("HL7-test-not-in-machine-code-system")]
    ForeignTest(#[source] crate::OrderMappingError),
    /// HL7-specimen-type-not-in-machine-code-system.
    #[error("HL7-specimen-type-not-in-machine-code-system")]
    ForeignSpecimenType(#[source] crate::OrderMappingError),
    /// HL7-unmapped-priority.
    #[error("HL7-unmapped-priority")]
    UnmappedPriority(#[source] crate::OrderMappingError),
    /// App evidence violates a common-model text bound.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// HL7 syntax rejected mapped content.
    #[error(transparent)]
    Syntax(#[from] analyte_hl7::Error),
    /// HL7 order builder rejected mapped content.
    #[error(transparent)]
    Order(#[from] analyte_hl7::order::Error),
}
