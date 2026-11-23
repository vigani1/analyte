//! Parses VIDAS tagged fields (BCI Appendix A chapter 4).
use analyte_model::{SampleId, SourceText};
use thiserror::Error;
#[cfg(test)]
mod tests;

/// Required installed sample ID limit; never truncate an identifier.
#[derive(Debug, Clone, Copy)]
pub enum SampleLimit {
    /// Older PC limit.
    Twelve,
    /// Later guide limit.
    Seventeen,
}

/// Selects which documented source evidence may identify result controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlLabels {
    /// Only the reserved VIDAS PC control identifiers.
    PcSlots,
    /// Reserved identifiers plus labels identified by Appendix B flags.
    AppendixB,
}

/// Direction and meaning of a logical message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One work-list request.
    Request,
    /// One result, possibly an assay component or control.
    Result,
}

/// Subject carried by the VIDAS ci field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// Exact sample identifier.
    Sample(SampleId),
    /// Internal kit control, with the strip lot separate from its name.
    KitControl {
        /// Exact control name.
        name: SourceText,
        /// Exact strip lot.
        lot: SourceText,
    },
    /// Exact external quality control identifier.
    QualityControl(SourceText),
}

impl Subject {
    fn parse(
        raw: &str,
        kind: Kind,
        limit: SampleLimit,
        flag: Option<&str>,
        labels: ControlLabels,
    ) -> Result<Self, Error> {
        // BCI Link rev C p. 4-5; VIDAS PC Manual F, internal and external controls.
        if kind == Kind::Result {
            let (name, lot) = raw.split_once(' ').unwrap_or((raw, ""));
            let kit = matches!(name, "C1" | "C2" | "C3");
            let qc = matches!(raw, "QC1" | "QC2" | "QC3" | "QC4" | "QC5" | "QC6");
            if kit && flag == Some("conthwst") || qc && flag == Some("conthnor") {
                return Err(Error::Field("ci".into()));
            }
            if !kit
                && !qc
                && matches!(flag, Some("conthnor" | "conthwst"))
                && labels == ControlLabels::PcSlots
            {
                return Err(Error::Field("ci".into()));
            }
            if kit || flag == Some("conthnor") {
                if !is_strip_lot(lot) || name.is_empty() || raw.len() > 17 {
                    return Err(Error::Field("ci".into()));
                }
                return Ok(Self::KitControl {
                    name: SourceText::new(name)?,
                    lot: SourceText::new(lot)?,
                });
            }
            if qc || flag == Some("conthwst") {
                if raw.len() > 12 || raw.trim().is_empty() {
                    return Err(Error::Field("ci".into()));
                }
                return Ok(Self::QualityControl(SourceText::new(raw)?));
            }
            // A composite that looks like a kit label needs source evidence
            // before it can be assigned to either a patient or a control.
            if is_strip_lot(lot) {
                return Err(Error::Field("ci".into()));
            }
        }
        let max = match limit {
            SampleLimit::Twelve => 12,
            SampleLimit::Seventeen => 17,
        };
        if raw.len() > max {
            return Err(Error::Field("ci".into()));
        }
        Ok(Self::Sample(SampleId::new(raw)?))
    }
}

fn is_strip_lot(lot: &str) -> bool {
    lot.split_once('-').is_some_and(|(date, version)| {
        date.len() == 6
            && date.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(version, "0" | "2")
    })
}

const REQUEST: &[&str] = &["mt", "pi", "pn", "pb", "ps", "so", "si", "ci", "rt", "qd"];
const RESULT: &[&str] = &[
    "mt", "pi", "pn", "pb", "ps", "so", "si", "ci", "rt", "rn", "tt", "td", "ql", "qn", "y3", "qd",
    "nc", "id", "sn", "m4",
];

/// One tag and its exact value, including unknown extension tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    tag: String,
    value: String,
}

impl Field {
    /// Returns the two-character source tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Returns unchanged text, including empty values.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A bounded syntactically valid message with required identity fields.
#[derive(Debug, Clone)]
pub struct Message {
    original: String,
    fields: Vec<Field>,
    subject: Subject,
    kind: Kind,
}

/// Tagged syntax failures; original input must remain in the protected journal.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Malformed printable ASCII tagged fields.
    #[error("malformed tagged message")]
    Syntax,
    /// Missing mandatory tag, duplicate tag, or invalid field length/value.
    #[error("invalid field {0}")]
    Field(String),
    /// Known fields are out of documented order or direction.
    #[error("field order or direction violation")]
    Order,
    /// Common model text constraint.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Only results may be mapped to result evidence.
    #[error("request is not a result")]
    NotResult,
}

impl Message {
    /// Parses ordered tags while retaining unknown tags and original text.
    /// # Errors
    /// Rejects malformed, excessive, duplicate, out-of-order or incomplete input.
    pub fn parse(text: &str, limit: SampleLimit) -> Result<Self, Error> {
        Self::parse_with_control_labels(text, limit, ControlLabels::PcSlots)
    }

    /// Parses a message using the source control labels accepted by the definition.
    /// # Errors
    /// Rejects malformed messages and controls without an established label layout.
    pub fn parse_with_control_labels(
        text: &str,
        limit: SampleLimit,
        labels: ControlLabels,
    ) -> Result<Self, Error> {
        if text.len() > 1920
            || !text.ends_with('|')
            || !text.bytes().all(|b| (32..=126).contains(&b))
        {
            return Err(Error::Syntax);
        }
        let mut fields: Vec<Field> = Vec::new();
        for part in text.strip_suffix('|').ok_or(Error::Syntax)?.split('|') {
            let tag = part.get(..2).ok_or(Error::Syntax)?;
            let value = part.get(2..).ok_or(Error::Syntax)?;
            if !tag
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            {
                return Err(Error::Syntax);
            }
            if fields.iter().any(|f| f.tag == tag) {
                return Err(Error::Field(tag.to_owned()));
            }
            fields.push(Field {
                tag: tag.to_owned(),
                value: value.to_owned(),
            });
        }
        let get = |tag: &str| {
            fields
                .iter()
                .find(|f| f.tag == tag)
                .map(|f| f.value.as_str())
        };
        let kind = match get("mt") {
            Some("mpr") => Kind::Request,
            Some("rsl") => Kind::Result,
            _ => return Err(Error::Field("mt".into())),
        };
        let order = if kind == Kind::Request {
            REQUEST
        } else {
            RESULT
        };
        let mut previous = None;
        for field in &fields {
            if let Some(index) = order.iter().position(|tag| *tag == field.tag) {
                if previous.is_some_and(|p| index <= p) {
                    return Err(Error::Order);
                }
                previous = Some(index);
            } else if RESULT.contains(&field.tag.as_str()) {
                return Err(Error::Order);
            }
            let maximum = match field.tag.as_str() {
                "pi" => 17,
                "pn" | "y3" => 30,
                "so" => 5,
                "si" => 0,
                "rt" => 4,
                "rn" => 40,
                "ql" | "qn" => 29,
                "nc" | "id" => 15,
                "sn" | "m4" => 20,
                "ps" => 1,
                _ => 1920,
            };
            if field.value.len() > maximum {
                return Err(Error::Field(field.tag.clone()));
            }
        }
        for tag in ["mt", "pi", "si", "ci", "rt"] {
            let value = get(tag).ok_or_else(|| Error::Field(tag.into()))?;
            if ["ci", "rt"].contains(&tag) && value.is_empty()
                || kind == Kind::Request && tag == "pi" && value.is_empty()
            {
                return Err(Error::Field(tag.into()));
            }
        }
        if kind == Kind::Result {
            for tag in ["tt", "td", "ql", "qn"] {
                if get(tag).is_none() {
                    return Err(Error::Field(tag.into()));
                }
            }
        }
        let subject = Subject::parse(
            get("ci").ok_or_else(|| Error::Field("ci".into()))?,
            kind,
            limit,
            get("nc"),
            labels,
        )?;
        Ok(Self {
            original: text.to_owned(),
            fields,
            subject,
            kind,
        })
    }

    /// Exact original tagged text; also the lossless wire builder output.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.original
    }

    /// Returns every field, including unknown extensions.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Returns the sample or control identified by the source.
    #[must_use]
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// Returns the declared logical message kind.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Gets exact text for a tag without interpreting locale, calls or dates.
    #[must_use]
    pub fn get(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.tag == tag)
            .map(|f| f.value.as_str())
    }
}
