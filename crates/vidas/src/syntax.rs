//! Tagged fields, from research/vidas.md ?4 (BCI A ?4).
use analyte_model::{ResultValue, SampleId, SourceText, ValueInterpretation};
use thiserror::Error;

/// Required installed sample ID limit; never truncate an identifier.
#[derive(Debug, Clone, Copy)]
pub enum SampleLimit {
    /// Older PC limit.
    Twelve,
    /// Later guide limit.
    Seventeen,
}

/// Direction and meaning of a logical message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One work-list request.
    Request,
    /// One result, possibly an assay component or control.
    Result,
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
    sample: SampleId,
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
                "ci" => match limit {
                    SampleLimit::Twelve => 12,
                    SampleLimit::Seventeen => 17,
                },
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
        let sample = SampleId::new(get("ci").ok_or_else(|| Error::Field("ci".into()))?)?;
        Ok(Self {
            original: text.to_owned(),
            fields,
            sample,
            kind,
        })
    }

    /// Builds through the same validation boundary, without reordering or truncation.
    /// # Errors
    /// Returns the same syntax errors as parsing; pipe/control injection is rejected.
    pub fn build(fields: &[(&str, &str)], limit: SampleLimit) -> Result<Self, Error> {
        let mut text = String::new();
        for (tag, value) in fields {
            if tag.len() != 2
                || value.contains('|')
                || text
                    .len()
                    .checked_add(tag.len())
                    .and_then(|n| n.checked_add(value.len()))
                    .and_then(|n| n.checked_add(1))
                    .is_none_or(|n| n > 1920)
            {
                return Err(Error::Syntax);
            }
            text.push_str(tag);
            text.push_str(value);
            text.push('|');
        }
        Self::parse(&text, limit)
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

    /// Returns the entered/downloaded identifier exactly.
    #[must_use]
    pub fn sample(&self) -> &SampleId {
        &self.sample
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

    /// Maps result text conservatively. Identity remains unresolved (ADR 0003).
    /// # Errors
    /// Rejects requests or model text violations. No number or unit is guessed.
    pub fn result(&self) -> Result<ResultEvidence, Error> {
        if self.kind != Kind::Result {
            return Err(Error::NotResult);
        }
        Ok(ResultEvidence {
            sample: self.sample.clone(),
            assay: SourceText::new(self.get("rt").ok_or_else(|| Error::Field("rt".into()))?)?,
            value: ResultValue {
                original: SourceText::new(
                    self.get("qn").ok_or_else(|| Error::Field("qn".into()))?,
                )?,
                parsed: ValueInterpretation::Text,
            },
            call: SourceText::new(self.get("ql").ok_or_else(|| Error::Field("ql".into()))?)?,
            unit: self.get("y3").map(SourceText::new).transpose()?,
            flag: self.get("nc").map(SourceText::new).transpose()?,
            dilution: self.get("qd").map(SourceText::new).transpose()?,
        })
    }
}

/// Uninterpreted result evidence, not a deliverable identified observation.
/// No physical position exists in the documented schema. The host must resolve
/// result/revision identity and patient versus QC classification before delivery.
#[derive(Debug, Clone)]
pub struct ResultEvidence {
    /// Exact source sample identifier.
    pub sample: SampleId,
    /// Unmapped assay code; no assay name guessing.
    pub assay: SourceText,
    /// Original measured text, including comparator, error token or embedded unit.
    pub value: ResultValue,
    /// Original qualitative call in the installed language.
    pub call: SourceText,
    /// Separately transmitted unit, if any; no inferred conversion.
    pub unit: Option<SourceText>,
    /// Calibration/control/lot flag; never a clinical judgement.
    pub flag: Option<SourceText>,
    /// Original ratio or factor; application status is unknown.
    pub dilution: Option<SourceText>,
}
