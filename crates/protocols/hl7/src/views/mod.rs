//! Reads typed HL7 segment fields.
use crate::{Error, Field, Message, Segment, Version};
mod location;
pub use location::{Location, location_error, syntax_location};
mod result;
pub use result::validate_result_fields;
mod grammar;
pub use grammar::{ResultGrammarError, ResultMessage};
mod evidence;
pub use evidence::{WireCompatibility, wire_compatibility};

/// A nonempty bounded message identity; preserves spelling without numeric conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlId<'a>(&'a [u8]);
impl<'a> ControlId<'a> {
    /// Checks the ST identity size and excludes wire control bytes.
    /// Delimiter conflicts are checked against the actual message at rendering time.
    /// # Errors
    /// Rejects empty, null, oversized or non-graphic identities.
    pub fn new(raw: &'a [u8]) -> Result<Self, Error> {
        Self::with_limit(raw, 20)
    }
    pub(crate) fn with_limit(raw: &'a [u8], maximum: usize) -> Result<Self, Error> {
        if raw.is_empty()
            || raw == b"\"\""
            || raw.len() > maximum
            || !raw.iter().all(u8::is_ascii_graphic)
        {
            return Err(Error::Control {
                segment: "MSH",
                field: 10,
            });
        }
        Ok(Self(raw))
    }
    /// Constructs a borrowed identity after validation by its owning profile.
    pub(crate) fn from_profile(raw: &'a [u8]) -> Self {
        Self(raw)
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.0
    }
}

/// Conditional acknowledgment request from HL7 table 0155.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// All outcomes.
    Always,
    /// No outcomes.
    Never,
    /// Failed outcomes only.
    Error,
    /// Successful outcomes only.
    Success,
}
impl Condition {
    /// Reports whether a response is requested for this outcome.
    #[must_use]
    pub fn requests(self, success: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Error => !success,
            Self::Success => success,
        }
    }
}
/// Original processing or independently negotiated enhanced responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckMode {
    /// Neither request field has a value.
    Original,
    /// At least one request field has a value.
    Enhanced {
        /// Accept response condition; absent means no accept response.
        accept: Option<Condition>,
        /// Application response condition.
        application: Option<Condition>,
    },
}
/// All six MSA acknowledgment codes, separate from order/query states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckCode {
    /// Application accepted.
    Aa,
    /// Application error.
    Ae,
    /// Application rejected.
    Ar,
    /// Commit accepted.
    Ca,
    /// Commit error.
    Ce,
    /// Commit rejected.
    Cr,
}
impl AckCode {
    pub(crate) fn parse(raw: &[u8]) -> Result<Self, Error> {
        match raw {
            b"AA" => Ok(Self::Aa),
            b"AE" => Ok(Self::Ae),
            b"AR" => Ok(Self::Ar),
            b"CA" => Ok(Self::Ca),
            b"CE" => Ok(Self::Ce),
            b"CR" => Ok(Self::Cr),
            _ => Err(Error::Control {
                segment: "MSA",
                field: 1,
            }),
        }
    }
    pub(crate) fn wire(self) -> &'static [u8] {
        match self {
            Self::Aa => b"AA",
            Self::Ae => b"AE",
            Self::Ar => b"AR",
            Self::Ca => b"CA",
            Self::Ce => b"CE",
            Self::Cr => b"CR",
        }
    }
    pub(crate) fn success(self) -> bool {
        matches!(self, Self::Aa | Self::Ca)
    }
    pub(crate) fn accept(self) -> bool {
        matches!(self, Self::Ca | Self::Ce | Self::Cr)
    }
}
/// Result category carried in MSH-16 by the documented vendor profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCategory {
    /// Patient results.
    Patient,
    /// Calibration data.
    Calibration,
    /// Quality control data.
    QualityControl,
}

/// Validated MSH control view; raw composites remain available through `segment`.
#[derive(Debug, Clone, Copy)]
pub struct Msh<'a> {
    segment: Segment<'a>,
    version: Version,
    mode: AckMode,
    category: Option<MessageCategory>,
    edition: Edition,
}

/// Where a view's edition came from, which decides the reply's MSH-12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edition {
    /// Read from MSH-12; a reply echoes MSH-12 unchanged.
    Declared,
    /// MSH-12 names no recognised edition; a reply declares the fallback.
    Fallback,
}
impl<'a> Msh<'a> {
    /// Validates the required control fields and acknowledgment table values.
    /// # Errors
    /// Returns the exact MSH field that is absent or invalid.
    pub fn new(message: &'a Message) -> Result<Self, Error> {
        let segment = message.segments().next().ok_or(Error::Encoding)?;
        let version =
            Version::parse(first_component(required(segment, 12)?)).ok_or(Error::Control {
                segment: "MSH",
                field: 12,
            })?;
        let rules = version.require_rules()?;
        let maximum = rules.control_bytes;
        for (number, maximum) in [(9, 15), (10, maximum), (11, 3), (12, 60)] {
            let value = required(segment, number)?;
            if value.raw().len() > maximum {
                return Err(Error::Control {
                    segment: "MSH",
                    field: number,
                });
            }
        }
        control_id_bytes(segment)?;
        let processing_field = required(segment, 11)?;
        let mut processing_parts = processing_field
            .raw()
            .split(|b| *b == segment.delimiters.component);
        let processing = processing_parts.next().unwrap_or_default();
        if processing_parts
            .next()
            .is_some_and(|mode| !matches!(mode, b"" | b"A" | b"R" | b"I" | b"T"))
            || processing_parts.next().is_some()
        {
            return Err(Error::Control {
                segment: "MSH",
                field: 11,
            });
        }
        if !matches!(processing, b"P" | b"T" | b"D") {
            return Err(Error::Control {
                segment: "MSH",
                field: 11,
            });
        }
        validate_message_type(segment, version)?;
        if message.control == crate::ControlProfile::RecordOnly {
            return Ok(Self {
                segment,
                version,
                mode: AckMode::Original,
                category: None,
                edition: Edition::Declared,
            });
        }
        validate_timestamp(segment, version)?;
        let category = message_category(message.control, segment, version)?;
        if rules.ack_pair == crate::version::AckPair::Paired {
            for field in [15, 16] {
                if segment.field(field).is_some_and(Field::is_null) {
                    return Err(Error::Control {
                        segment: "MSH",
                        field,
                    });
                }
            }
        }
        let accept = condition(segment, 15)?;
        let application = if category.is_some() && accept.is_none() {
            None
        } else {
            condition(segment, 16)?
        };
        if rules.ack_pair == crate::version::AckPair::Paired
            && accept.is_some() != application.is_some()
        {
            return Err(Error::Control {
                segment: "MSH",
                field: if accept.is_none() { 15 } else { 16 },
            });
        }
        let mode = if accept.is_none() && application.is_none() {
            AckMode::Original
        } else {
            AckMode::Enhanced {
                accept,
                application,
            }
        };
        Ok(Self {
            segment,
            version,
            mode,
            category,
            edition: Edition::Declared,
        })
    }
    /// Reads only what a rejection needs from a header whose MSH-12 names no
    /// edition this crate recognises, so the sender learns why its message was
    /// refused (HL7 2.5.1 §2.9.2.1 b, §2.9.3.2 b; table 0357 code 203).
    ///
    /// The view's edition is the fallback HL7 2.5.1: it bounds MSH-10 and
    /// shapes the reply (MSH-9 structure, MSH-7 form, ERR layout), and the
    /// reply declares 2.5.1 in its MSH-12, the edition it is written in, rather
    /// than echoing one we cannot write. MSA-2 still echoes MSH-10. Read the
    /// header with [`Message::parse_header`]; reply only with [`crate::rejection`].
    /// Returns `Ok(None)` when MSH-12 names a recognised edition: use [`Self::new`].
    /// # Errors
    /// Returns the exact MSH field when MSH-10, MSH-12, MSH-15 or MSH-16 cannot
    /// be read; the message then gets no reply.
    pub fn unsupported_version(message: &'a Message) -> Result<Option<Self>, Error> {
        let segment = message.segments().next().ok_or(Error::Encoding)?;
        if Version::parse(first_component(required(segment, 12)?)).is_some() {
            return Ok(None);
        }
        let version = Version::V251;
        let maximum = version.require_rules()?.control_bytes;
        for (number, maximum) in [(10, maximum), (12, 60)] {
            if required(segment, number)?.raw().len() > maximum {
                return Err(Error::Control {
                    segment: "MSH",
                    field: number,
                });
            }
        }
        control_id_bytes(segment)?;
        let (accept, application) = if message.control == crate::ControlProfile::RecordOnly {
            (None, None)
        } else {
            (condition(segment, 15)?, condition(segment, 16)?)
        };
        let mode = if accept.is_none() && application.is_none() {
            AckMode::Original
        } else {
            AckMode::Enhanced {
                accept,
                application,
            }
        };
        Ok(Some(Self {
            segment,
            version,
            mode,
            category: None,
            edition: Edition::Fallback,
        }))
    }
    /// Constructs a view after LAW control-profile validation, including its
    /// compatible version and ignored-component policies.
    pub(crate) fn from_law(segment: Segment<'a>, triggered: bool) -> Self {
        Self {
            segment,
            version: Version::V251,
            category: None,
            edition: Edition::Declared,
            mode: if triggered {
                AckMode::Enhanced {
                    accept: Some(Condition::Never),
                    application: Some(Condition::Always),
                }
            } else {
                AckMode::Original
            },
        }
    }

    /// Returns the category when the scoped MSH-16 departure applied.
    #[must_use]
    pub fn message_category(self) -> Option<MessageCategory> {
        self.category
    }

    /// Returns the source segment, including all composite identifier components.
    #[must_use]
    pub fn segment(self) -> Segment<'a> {
        self.segment
    }
    /// Returns the selected edition.
    #[must_use]
    pub fn version(self) -> Version {
        self.version
    }
    /// Tells whether the edition was declared or is the fallback.
    pub(crate) fn edition(self) -> Edition {
        self.edition
    }
    /// Returns the independently validated acknowledgment conditions.
    #[must_use]
    pub fn mode(self) -> AckMode {
        self.mode
    }
    /// Returns MSH-9's code, event and optional structure without flattening components.
    pub fn message_type(self) -> impl Iterator<Item = &'a [u8]> {
        self.segment
            .field(9)
            .into_iter()
            .flat_map(Field::repetitions)
            .flat_map(crate::Repetition::components)
            .map(crate::Component::raw)
    }
    /// Returns the original MSH-10 bytes, preserving case and leading zeros.
    #[must_use]
    pub fn control_id(self) -> &'a [u8] {
        self.segment.field(10).map_or(&[], Field::raw)
    }
    /// Returns complete MSH-11 processing ID and optional processing mode.
    #[must_use]
    pub fn processing_id(self) -> Option<Field<'a>> {
        self.segment.field(11)
    }
}
// YHLO LIS Protocol Manual V1.1 sections 2.3.1, 2.4.1. Only the inbound
// 2.3.1 result header uses these codes in place of negotiation.
fn message_category(
    profile: crate::ControlProfile,
    segment: Segment<'_>,
    version: Version,
) -> Result<Option<MessageCategory>, Error> {
    if profile != crate::ControlProfile::Msh16MessageCategory || version != Version::V231 {
        return Ok(None);
    }
    let kind = required(segment, 9)?;
    let mut components = kind
        .raw()
        .split(|byte| *byte == segment.delimiters.component);
    if components.next() != Some(b"ORU")
        || components.next() != Some(b"R01")
        || components.next().is_some()
    {
        return Ok(None);
    }
    match segment.field(16).map(Field::raw) {
        Some(b"0") => Ok(Some(MessageCategory::Patient)),
        Some(b"1") => Ok(Some(MessageCategory::Calibration)),
        Some(b"2") => Ok(Some(MessageCategory::QualityControl)),
        _ => Err(Error::Control {
            segment: "MSH",
            field: 16,
        }),
    }
}

fn validate_timestamp(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    let rules = version.require_rules()?;
    // HL7 2.5.1 section 2.15.9.7 made MSH-7 required as of 2.4;
    // 2.3.1 retains optional usage. Validate a supplied TS in either edition.
    let timestamp = segment.field(7);
    if version.required_timestamp() || timestamp.is_some_and(|field| !field.raw().is_empty()) {
        let field = required(segment, 7)?;
        let mut repetitions = field.repetitions();
        let valid = repetitions.next().is_some_and(|value| {
            crate::datatypes::edition_timestamp(&value.components().collect::<Vec<_>>(), version)
        });
        let maximum = match rules.time_format {
            crate::version::TimeFormat::Ts => 26,
            crate::version::TimeFormat::Dtm => 24,
        };
        if field.raw().len() > maximum || !valid || repetitions.next().is_some() {
            return Err(Error::Control {
                segment: "MSH",
                field: 7,
            });
        }
    }
    Ok(())
}

fn condition(segment: Segment<'_>, field: usize) -> Result<Option<Condition>, Error> {
    match segment.field(field).map_or(&[][..], Field::raw) {
        b"" | b"\"\"" => Ok(None),
        b"AL" => Ok(Some(Condition::Always)),
        b"NE" => Ok(Some(Condition::Never)),
        b"ER" => Ok(Some(Condition::Error)),
        b"SU" => Ok(Some(Condition::Success)),
        _ => Err(Error::Control {
            segment: "MSH",
            field,
        }),
    }
}
// MSH-10 is echoed in MSA-2, so it must be printable and free of delimiters.
fn control_id_bytes(segment: Segment<'_>) -> Result<(), Error> {
    let id = required(segment, 10)?;
    if id.raw().iter().any(|b| {
        !b.is_ascii_graphic()
            || segment.delimiters.bytes().contains(b)
            || segment.delimiters.truncation == Some(*b)
    }) {
        return Err(Error::Control {
            segment: "MSH",
            field: 10,
        });
    }
    Ok(())
}
fn first_component(field: Field<'_>) -> &[u8] {
    field
        .repetitions()
        .next()
        .and_then(|r| r.components().next())
        .map_or(&[], |c| c.raw())
}
pub(crate) fn required(segment: Segment<'_>, field: usize) -> Result<Field<'_>, Error> {
    segment
        .field(field)
        .filter(|f| !f.raw().is_empty() && !f.is_null())
        .ok_or(Error::Control {
            segment: "MSH",
            field,
        })
}
/// MSA view validates its code and correlation ID; legacy fields remain raw.
#[derive(Debug, Clone, Copy)]
pub struct Msa<'a> {
    segment: Segment<'a>,
    code: AckCode,
}
impl<'a> Msa<'a> {
    /// Validates an MSA segment without interpreting query or order statuses.
    /// # Errors
    /// Rejects a different segment, missing correlation or unknown code.
    pub fn new(segment: Segment<'a>) -> Result<Self, Error> {
        Self::with_control_limit(segment, 20)
    }
    /// Validates the correlation length declared by the message edition.
    /// # Errors
    /// Refuses unsupported editions and malformed MSA control fields.
    pub fn for_version(segment: Segment<'a>, version: Version) -> Result<Self, Error> {
        let rules = version.require_rules()?;
        for field in rules.withdrawn_msa {
            if segment
                .field(*field)
                .is_some_and(|value| !value.raw().is_empty())
            {
                return Err(Error::Control {
                    segment: "MSA",
                    field: *field,
                });
            }
        }
        Self::with_control_limit(segment, rules.control_bytes)
    }
    /// Constructs a view after the LAW boundary has validated the MSA profile.
    pub(crate) fn from_law(segment: Segment<'a>) -> Result<Self, Error> {
        Ok(Self {
            segment,
            code: AckCode::parse(segment.field(1).map_or(&[], Field::raw))?,
        })
    }

    pub(crate) fn with_control_limit(segment: Segment<'a>, maximum: usize) -> Result<Self, Error> {
        if segment.id() != b"MSA" {
            return Err(Error::Control {
                segment: "MSA",
                field: 0,
            });
        }
        let code = AckCode::parse(segment.field(1).map_or(&[], Field::raw))?;
        if segment
            .field(2)
            .is_none_or(|f| f.raw().is_empty() || f.is_null() || f.raw().len() > maximum)
        {
            return Err(Error::Control {
                segment: "MSA",
                field: 2,
            });
        }
        Ok(Self { segment, code })
    }
    /// Borrows the checked code.
    #[must_use]
    pub fn code(self) -> AckCode {
        self.code
    }
    /// Returns the exact correlated message ID.
    #[must_use]
    pub fn control_id(self) -> &'a [u8] {
        self.segment.field(2).map_or(&[], Field::raw)
    }
    /// Returns all source fields, including legacy MSA-3/6.
    #[must_use]
    pub fn segment(self) -> Segment<'a> {
        self.segment
    }
}
/// Edition-specific ERR interpretation; never infers edition from field content.
#[derive(Debug, Clone, Copy)]
pub enum ErrView<'a> {
    /// 2.3.1 ERR-1 composite location and code.
    V231(Field<'a>),
    /// 2.5.1 error fields; preserves repeated locations and application details.
    V251 {
        /// ERR-2 locations.
        locations: Option<Field<'a>>,
        /// ERR-3 HL7 code.
        code: Field<'a>,
        /// ERR-4 severity.
        severity: Field<'a>,
        /// ERR-5 application code.
        application: Option<Field<'a>>,
        /// Complete source, including ERR-6..12.
        segment: Segment<'a>,
    },
}
impl<'a> ErrView<'a> {
    /// Selects the edition's error shape and validates required fields.
    /// # Errors
    /// Rejects missing required fields or invalid severity.
    pub fn new(segment: Segment<'a>, version: Version) -> Result<Self, Error> {
        let get = |field| {
            segment
                .field(field)
                .filter(|f| !f.raw().is_empty() && !f.is_null())
                .ok_or(Error::Control {
                    segment: "ERR",
                    field,
                })
        };
        if segment.id() != b"ERR" {
            return Err(Error::Control {
                segment: "ERR",
                field: 0,
            });
        }
        for field in version.require_rules()?.withdrawn_err {
            if segment
                .field(*field)
                .is_some_and(|value| !value.raw().is_empty())
            {
                return Err(Error::Control {
                    segment: "ERR",
                    field: *field,
                });
            }
        }
        match version.modern_error_layout() {
            false => Ok(Self::V231(get(1)?)),
            true => {
                let severity = get(4)?;
                if !matches!(severity.raw(), b"W" | b"I" | b"E")
                    && !(version.fatal_error_severity() && severity.raw() == b"F")
                    || (get(3)?
                        .repetitions()
                        .flat_map(crate::Repetition::components)
                        .next()
                        .is_some_and(|c| c.raw() == b"0")
                        && severity.raw() != b"I")
                {
                    return Err(Error::Control {
                        segment: "ERR",
                        field: 4,
                    });
                }
                let code = get(3)?;
                if !code
                    .repetitions()
                    .flat_map(crate::Repetition::components)
                    .next()
                    .is_some_and(|c| error_code(c.raw(), version))
                {
                    return Err(Error::Control {
                        segment: "ERR",
                        field: 3,
                    });
                }
                Ok(Self::V251 {
                    locations: segment.field(2),
                    code,
                    severity,
                    application: segment.field(5),
                    segment,
                })
            }
        }
    }
}

fn validate_message_type(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    let rules = version.require_rules()?;
    let message_type = required(segment, 9)?;
    let type_parts: Vec<_> = message_type
        .raw()
        .split(|b| *b == segment.delimiters.component)
        .collect();
    // HL7 2.3.1 section 2.24.1.9 permits omitted response/ACK triggers.
    let response = matches!(
        type_parts.first().copied().unwrap_or_default(),
        b"ACK"
            | b"ADR"
            | b"DOC"
            | b"DSR"
            | b"EDR"
            | b"ERP"
            | b"MFD"
            | b"MFK"
            | b"MFR"
            | b"ORD"
            | b"ORF"
            | b"ORN"
            | b"ORR"
            | b"ORS"
            | b"OSR"
            | b"PPT"
            | b"PPV"
            | b"PRR"
            | b"PTR"
            | b"QCK"
            | b"R0R"
            | b"RPA"
            | b"RPI"
            | b"RPL"
            | b"RRA"
            | b"RRD"
            | b"RRE"
            | b"RRG"
            | b"RRI"
            | b"RRO"
            | b"SQR"
            | b"SRR"
            | b"TBR"
            | b"VXR"
            | b"VXX"
    );
    let omitted_trigger = rules.optional_response_trigger && response;
    if (rules.message_lengths.is_some() && type_parts.len() != 3)
        || type_parts.len() > 3
        || (type_parts.len() < 2 && !omitted_trigger)
        || type_parts.iter().enumerate().any(|(index, part)| {
            (rules
                .message_lengths
                .is_some_and(|lengths| part.len() > lengths.get(index).copied().unwrap_or(0)))
                || (part.is_empty() && !(index == 1 && omitted_trigger))
                || !part.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        })
    {
        return Err(Error::Control {
            segment: "MSH",
            field: 9,
        });
    }
    Ok(())
}

// HL7 table 0357, shared by the selected editions; local codes belong in ERR-5.
pub(crate) fn error_code(code: &[u8], version: Version) -> bool {
    (version.unknown_key_error() && code == b"104")
        || matches!(
            code,
            b"0" | b"100"
                | b"101"
                | b"102"
                | b"103"
                | b"200"
                | b"201"
                | b"202"
                | b"203"
                | b"204"
                | b"205"
                | b"206"
                | crate::APPLICATION_INTERNAL_ERROR
        )
}

#[cfg(test)]
mod category_tests {
    use super::*;
    use crate::{ControlProfile, Options};

    // HL7 2.3.1 section 2.24.1.16; YHLO LIS Protocol Manual V1.1 sections 2.3.1, 2.4.1.
    #[test]
    fn msh16_is_a_category_only_under_the_scoped_departure() -> Result<(), Error> {
        let departed = Options {
            control: ControlProfile::Msh16MessageCategory,
            ..Options::default()
        };
        for (wire, category) in [
            ("0", MessageCategory::Patient),
            ("1", MessageCategory::Calibration),
            ("2", MessageCategory::QualityControl),
        ] {
            let raw = format!("MSH|^~\\&|A|B|||20261123||ORU^R01|0007|P|2.3.1||||{wire}\r");
            let strict = Message::parse(raw.as_bytes(), Options::default())?;
            assert!(matches!(
                Msh::new(&strict),
                Err(Error::Control {
                    segment: "MSH",
                    field: 16
                })
            ));
            let message = Message::parse(raw.as_bytes(), departed)?;
            let msh = Msh::new(&message)?;
            assert_eq!(msh.message_category(), Some(category));
            assert_eq!(msh.mode(), AckMode::Original);
            assert_eq!(message.raw(), raw.as_bytes());
        }

        for (kind, edition, accept, value) in [
            ("QRY^Q02", "2.3.1", "", "0"),
            ("ORU^R01^ORU_R01", "2.5.1", "", "0"),
            ("ORU^R01", "2.3.1", "AL", "0"),
            ("ORU^R01", "2.3.1", "", "3"),
            ("ORU^R01", "2.3.1", "", ""),
        ] {
            let raw =
                format!("MSH|^~\\&|A|B|||20261123||{kind}|0007|P|{edition}|||{accept}|{value}\r");
            let message = Message::parse(raw.as_bytes(), departed)?;
            assert!(matches!(
                Msh::new(&message),
                Err(Error::Control {
                    segment: "MSH",
                    field: 16
                })
            ));
        }
        Ok(())
    }
}
