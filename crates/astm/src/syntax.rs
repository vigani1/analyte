//! ASTM record grammar belongs in the syntax layer, without transport or mapping.
use thiserror::Error;

/// Limits applied before allocating structures from input.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum total input bytes.
    pub bytes: usize,
    /// Maximum bytes in one record, excluding CR.
    pub record: usize,
    /// Maximum fields, including record ID.
    pub fields: usize,
    /// Maximum repetitions in one field.
    pub repeats: usize,
    /// Maximum components in one repetition.
    pub components: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 16_777_216,
            record: 65_536,
            fields: 128,
            repeats: 128,
            components: 128,
        }
    }
}

/// Syntax failures contain positions, never clinical payloads.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// An agreed resource ceiling was exceeded.
    #[error("record {record}, field {field}: {kind} limit exceeded")]
    Limit {
        /// One-based record, or zero for input.
        record: usize,
        /// One-based field, or zero for record.
        field: usize,
        /// Resource name.
        kind: &'static str,
    },
    /// Header delimiter definition is absent or inconsistent.
    #[error("record {record}: invalid delimiter definition")]
    Delimiters {
        /// One-based record.
        record: usize,
    },
    /// A record is not CR terminated.
    #[error("input ends without CR")]
    MissingCr,
    /// The record identifier is unsupported or malformed.
    #[error("record {record}: invalid record identifier")]
    RecordType {
        /// One-based record.
        record: usize,
    },
    /// The hierarchy cannot associate a record safely.
    #[error("record {record}: {reason}")]
    Hierarchy {
        /// One-based record.
        record: usize,
        /// Structural reason.
        reason: &'static str,
    },
    /// Required sequence is absent, malformed, or out of order.
    #[error("record {record}: invalid sequence, expected {expected}")]
    Sequence {
        /// One-based record.
        record: usize,
        /// Expected occurrence.
        expected: usize,
    },
    /// An escape has no closing delimiter.
    #[error("record {record}, field {field}: unterminated escape")]
    Escape {
        /// One-based record.
        record: usize,
        /// One-based field.
        field: usize,
    },
    /// A byte is outside the LIS02 repertoire.
    #[error("record {record}: forbidden byte at offset {offset}")]
    Character {
        /// One-based record.
        record: usize,
        /// Zero-based byte offset.
        offset: usize,
    },
    /// The selected decoder refused a component.
    #[error("record {record}, field {field}: decoder rejected text")]
    Decode {
        /// One-based record.
        record: usize,
        /// One-based field.
        field: usize,
    },
}

/// Four distinct header-selected syntax bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delimiters {
    field: u8,
    repeat: u8,
    component: u8,
    escape: u8,
}
impl Delimiters {
    /// Constructs distinct permitted delimiters.
    /// # Errors
    /// Returns `Error::Delimiters` for duplicate or forbidden bytes.
    pub fn new(field: u8, repeat: u8, component: u8, escape: u8) -> Result<Self, Error> {
        let values = [field, repeat, component, escape];
        for (i, value) in values.iter().enumerate() {
            if !legal(*value) || *value == b'\r' || values.iter().take(i).any(|v| v == value) {
                return Err(Error::Delimiters { record: 0 });
            }
        }
        Ok(Self {
            field,
            repeat,
            component,
            escape,
        })
    }
    /// Returns field, repeat, component and escape in header order.
    #[must_use]
    pub fn bytes(self) -> [u8; 4] {
        [self.field, self.repeat, self.component, self.escape]
    }
}

/// Standard record identifiers; case is ignored only for these identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Header, level zero.
    Header,
    /// Patient, level one.
    Patient,
    /// Order, level two.
    Order,
    /// Result, level three.
    Result,
    /// Comment attached to the previous record.
    Comment,
    /// Query, level one.
    Query,
    /// Terminator, level zero.
    Terminator,
    /// Scientific, level one.
    Scientific,
    /// Manufacturer information attached to the previous record.
    Manufacturer,
}
impl Kind {
    fn parse(byte: u8) -> Option<Self> {
        match byte.to_ascii_uppercase() {
            b'H' => Some(Self::Header),
            b'P' => Some(Self::Patient),
            b'O' => Some(Self::Order),
            b'R' => Some(Self::Result),
            b'C' => Some(Self::Comment),
            b'Q' => Some(Self::Query),
            b'L' => Some(Self::Terminator),
            b'S' => Some(Self::Scientific),
            b'M' => Some(Self::Manufacturer),
            _ => None,
        }
    }
    fn level(self) -> Option<usize> {
        match self {
            Self::Header | Self::Terminator => Some(0),
            Self::Patient | Self::Query | Self::Scientific => Some(1),
            Self::Order => Some(2),
            Self::Result => Some(3),
            Self::Comment | Self::Manufacturer => None,
        }
    }
}

/// Distinguishes absence of content from an explicit deletion instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    /// No bytes sent.
    Empty,
    /// Exactly two double quotes sent.
    Delete,
    /// Nonempty content, without clinical interpretation.
    Value,
}

/// One component retains its exact wire spelling beside decoded text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    raw: Vec<u8>,
    text: String,
}
impl Component {
    /// Exact component bytes, including optional or unknown escapes.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Required delimiter escapes decoded; other closed escapes remain literal.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
/// One positional field, preserving internal null repetitions and components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    raw: Vec<u8>,
    repeats: Vec<Vec<Component>>,
}
impl Field {
    /// Exact field bytes as received.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Repetitions, each containing positional components.
    #[must_use]
    pub fn repeats(&self) -> &[Vec<Component>] {
        &self.repeats
    }
    /// Empty and deletion remain distinct, including for composite fields.
    #[must_use]
    pub fn presence(&self) -> Presence {
        match self.raw.as_slice() {
            [] => Presence::Empty,
            b"\"\"" => Presence::Delete,
            _ => Presence::Value,
        }
    }
}
/// Validated record with its parent index in the containing message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    raw: Vec<u8>,
    kind: Kind,
    fields: Vec<Field>,
    parent: Option<usize>,
    sequence: Option<usize>,
}
impl Record {
    /// Exact record bytes, excluding CR.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Validated record type.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }
    /// Parent index; a flat arena avoids recursive input processing and destruction.
    #[must_use]
    pub fn parent(&self) -> Option<usize> {
        self.parent
    }
    /// Validated sequence number; headers have none.
    #[must_use]
    pub fn sequence(&self) -> Option<usize> {
        self.sequence
    }
    /// Retrieves a field by the standard's one-based position; omitted trailing fields return None.
    #[must_use]
    pub fn field(&self, position: usize) -> Option<&Field> {
        position.checked_sub(1).and_then(|i| self.fields.get(i))
    }
}
/// A complete H through L message, stored in wire order with parent links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    delimiters: Delimiters,
    records: Vec<Record>,
}
impl Message {
    /// Delimiters remain fixed throughout this message.
    #[must_use]
    pub fn delimiters(&self) -> Delimiters {
        self.delimiters
    }
    /// Every record in wire order, including header and terminator.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    /// Serializes the original wire spelling without losing optional escapes or trailing nulls.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for record in &self.records {
            bytes.extend_from_slice(&record.raw);
            bytes.push(b'\r');
        }
        bytes
    }
}

fn legal(byte: u8) -> bool {
    matches!(byte, 7 | 9 | 11 | 12 | 32..=126 | 128..=254)
}

/// Decodes every byte reversibly as a Latin-1 code point.
#[must_use]
pub fn latin1(bytes: &[u8]) -> Option<String> {
    Some(bytes.iter().map(|b| char::from(*b)).collect())
}

/// Parses complete CR-terminated messages with Latin-1 text.
/// # Errors
/// Returns positioned errors for limits, malformed syntax, sequence and hierarchy failures.
pub fn parse(bytes: &[u8], limits: Limits) -> Result<Vec<Message>, Error> {
    parse_with(bytes, limits, latin1)
}

/// Parses with a caller-selected strict decoder; None rejects undecodable text.
/// Closed optional and unknown escapes are preserved raw, including their delimiters.
/// Unterminated escapes are rejected. Escape bodies are never parsed recursively.
/// # Errors
/// Returns syntax errors or `Error::Decode` when the decoder refuses a component.
pub fn parse_with(
    bytes: &[u8],
    limits: Limits,
    decode: impl Fn(&[u8]) -> Option<String>,
) -> Result<Vec<Message>, Error> {
    if bytes.len() > limits.bytes {
        return Err(Error::Limit {
            record: 0,
            field: 0,
            kind: "input bytes",
        });
    }
    if !bytes.is_empty() && bytes.last() != Some(&b'\r') {
        return Err(Error::MissingCr);
    }
    let mut messages = Vec::new();
    let mut current: Option<Message> = None;
    let mut ancestors: Vec<usize> = Vec::new();
    let mut counts: Vec<(Kind, usize, usize)> = Vec::new();
    for (index, raw) in bytes
        .strip_suffix(b"\r")
        .unwrap_or_default()
        .split(|b| *b == b'\r')
        .filter(|_| !bytes.is_empty())
        .enumerate()
    {
        let number = index.saturating_add(1);
        if raw.len() > limits.record {
            return Err(Error::Limit {
                record: number,
                field: 0,
                kind: "record bytes",
            });
        }
        for (offset, byte) in raw.iter().enumerate() {
            if !legal(*byte) {
                return Err(Error::Character {
                    record: number,
                    offset,
                });
            }
        }
        let kind = raw
            .first()
            .and_then(|b| Kind::parse(*b))
            .ok_or(Error::RecordType { record: number })?;
        if kind == Kind::Header {
            if current.is_some() {
                return Err(Error::Hierarchy {
                    record: number,
                    reason: "header before terminator",
                });
            }
            current = Some(Message {
                delimiters: parse_delimiters(raw, number)?,
                records: Vec::new(),
            });
            ancestors.clear();
            counts.clear();
        }
        let message = current.as_mut().ok_or(Error::Hierarchy {
            record: number,
            reason: "record outside H through L",
        })?;
        let d = message.delimiters;
        if raw.get(1) != Some(&d.field) {
            return Err(Error::Delimiters { record: number });
        }
        let fields = parse_fields(raw, kind, d, limits, number, &decode)?;
        let level = kind.level().unwrap_or(ancestors.len());
        let parent = record_parent(kind, &ancestors, message, number)?;
        let sequence = parse_sequence(&fields, kind, level, &mut counts, number)?;
        if kind.level().is_some() {
            ancestors.truncate(level);
            ancestors.push(message.records.len());
        }
        message.records.push(Record {
            raw: raw.to_vec(),
            kind,
            fields,
            parent,
            sequence,
        });
        if kind == Kind::Terminator
            && let Some(done) = current.take()
        {
            messages.push(done);
        }
    }
    if current.is_some() {
        return Err(Error::Hierarchy {
            record: 0,
            reason: "message missing terminator",
        });
    }
    Ok(messages)
}

fn record_parent(
    kind: Kind,
    ancestors: &[usize],
    message: &Message,
    number: usize,
) -> Result<Option<usize>, Error> {
    let parent = match kind {
        Kind::Header => None,

        Kind::Comment | Kind::Manufacturer => message.records.len().checked_sub(1),
        Kind::Terminator | Kind::Patient | Kind::Query | Kind::Scientific => Some(0),
        Kind::Order => ancestors.get(1).copied().filter(|i| {
            message
                .records
                .get(*i)
                .is_some_and(|r| r.kind == Kind::Patient)
        }),
        Kind::Result => ancestors.get(2).copied().filter(|i| {
            message
                .records
                .get(*i)
                .is_some_and(|r| r.kind == Kind::Order)
        }),
    };
    if kind != Kind::Header && parent.is_none() {
        return Err(Error::Hierarchy {
            record: number,
            reason: "missing required parent",
        });
    }
    Ok(parent)
}

fn parse_delimiters(raw: &[u8], number: usize) -> Result<Delimiters, Error> {
    let definition = raw.get(1..6).ok_or(Error::Delimiters { record: number })?;
    let mut chars = definition.iter().copied();
    let (Some(f), Some(r), Some(c), Some(e), Some(end)) = (
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
    ) else {
        return Err(Error::Delimiters { record: number });
    };
    if f != end {
        return Err(Error::Delimiters { record: number });
    }
    let delimiters =
        Delimiters::new(f, r, c, e).map_err(|_| Error::Delimiters { record: number })?;
    Ok(delimiters)
}

fn parse_sequence(
    fields: &[Field],
    kind: Kind,
    level: usize,
    counts: &mut Vec<(Kind, usize, usize)>,
    number: usize,
) -> Result<Option<usize>, Error> {
    counts.retain(|(_, old_level, _)| *old_level <= level);
    let sequence = if kind == Kind::Header {
        None
    } else {
        let previous = counts
            .iter()
            .find(|(k, l, _)| *k == kind && *l == level)
            .map_or(0, |(_, _, n)| *n);
        let expected = previous.checked_add(1).ok_or(Error::Sequence {
            record: number,
            expected: previous,
        })?;
        let value = fields.get(1).map(|f| f.raw.as_slice()).unwrap_or_default();
        let parsed = value.iter().try_fold(0usize, |n, b| {
            if b.is_ascii_digit() {
                n.checked_mul(10)?
                    .checked_add(usize::from(b.checked_sub(b'0')?))
            } else {
                None
            }
        });
        if parsed != Some(expected) {
            return Err(Error::Sequence {
                record: number,
                expected,
            });
        }
        counts.retain(|(k, l, _)| *k != kind || *l != level);
        counts.push((kind, level, expected));
        Some(expected)
    };
    Ok(sequence)
}

fn split(
    bytes: &[u8],
    separator: u8,
    escape: u8,
    maximum: usize,
    record: usize,
    field: usize,
) -> Result<Vec<&[u8]>, Error> {
    let mut escaped = false;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if *b == escape {
            escaped = !escaped;
        }
        if *b == separator && !escaped {
            if parts.len().saturating_add(1) >= maximum {
                return Err(Error::Limit {
                    record,
                    field,
                    kind: "positional items",
                });
            }
            parts.push(bytes.get(start..i).unwrap_or_default());
            start = i.saturating_add(1);
        }
    }
    if escaped {
        return Err(Error::Escape { record, field });
    }
    if parts.len() >= maximum {
        return Err(Error::Limit {
            record,
            field,
            kind: "positional items",
        });
    }
    parts.push(bytes.get(start..).unwrap_or_default());
    Ok(parts)
}
fn expand(bytes: &[u8], d: Delimiters) -> Vec<u8> {
    let mut output = Vec::new();
    let mut remaining = bytes.iter().copied();
    while let Some(byte) = remaining.next() {
        if byte != d.escape {
            output.push(byte);
            continue;
        }
        let body: Vec<u8> = remaining.by_ref().take_while(|b| *b != d.escape).collect();
        let replacement = match body.as_slice() {
            b"F" => Some(d.field),
            b"S" => Some(d.component),
            b"R" => Some(d.repeat),
            b"E" => Some(d.escape),
            _ => None,
        };
        if let Some(value) = replacement {
            output.push(value);
        } else {
            output.push(d.escape);
            output.extend(body);
            output.push(d.escape);
        }
    }
    output
}

impl Message {
    /// Serializes using selected delimiters, translating delimiter escapes while preserving other operations.
    /// # Errors
    /// Rejects a new delimiter that collides with a literal optional escape body.
    pub fn encode_with(&self, target: Delimiters) -> Result<Vec<u8>, Error> {
        if target == self.delimiters {
            return Ok(self.encode());
        }
        let mut output = Vec::new();
        for (record_index, record) in self.records.iter().enumerate() {
            if record
                .raw
                .first()
                .is_some_and(|id| target.bytes().contains(id))
            {
                return Err(Error::Delimiters {
                    record: record_index.saturating_add(1),
                });
            }
            for (field_index, field) in record.fields.iter().enumerate() {
                if field_index > 0 {
                    output.push(target.field);
                }
                if record.kind == Kind::Header && field_index == 1 {
                    output.extend([target.repeat, target.component, target.escape]);
                    continue;
                }
                for (repeat_index, repeat) in field.repeats.iter().enumerate() {
                    if repeat_index > 0 {
                        output.push(target.repeat);
                    }
                    for (component_index, component) in repeat.iter().enumerate() {
                        if component_index > 0 {
                            output.push(target.component);
                        }
                        let mut bytes = component.raw.iter().copied();
                        while let Some(byte) = bytes.next() {
                            if byte == self.delimiters.escape {
                                let body: Vec<u8> = bytes
                                    .by_ref()
                                    .take_while(|b| *b != self.delimiters.escape)
                                    .collect();
                                let literal = match body.as_slice() {
                                    b"F" => Some(self.delimiters.field),
                                    b"S" => Some(self.delimiters.component),
                                    b"R" => Some(self.delimiters.repeat),
                                    b"E" => Some(self.delimiters.escape),
                                    _ => None,
                                };
                                if let Some(value) = literal {
                                    write_literal(&mut output, value, target);
                                } else {
                                    if body.contains(&target.escape) {
                                        return Err(Error::Escape {
                                            record: record_index.saturating_add(1),
                                            field: field_index.saturating_add(1),
                                        });
                                    }
                                    output.push(target.escape);
                                    output.extend(body);
                                    output.push(target.escape);
                                }
                            } else {
                                write_literal(&mut output, byte, target);
                            }
                        }
                    }
                }
            }
            output.push(b'\r');
        }
        Ok(output)
    }
}

fn write_literal(output: &mut Vec<u8>, byte: u8, target: Delimiters) {
    if let Some(code) = [
        (target.field, b'F'),
        (target.repeat, b'R'),
        (target.component, b'S'),
        (target.escape, b'E'),
    ]
    .iter()
    .find_map(|(delimiter, code)| (*delimiter == byte).then_some(*code))
    {
        output.extend([target.escape, code, target.escape]);
    } else {
        output.push(byte);
    }
}

fn parse_fields(
    raw: &[u8],
    kind: Kind,
    d: Delimiters,
    limits: Limits,
    number: usize,
    decode: &impl Fn(&[u8]) -> Option<String>,
) -> Result<Vec<Field>, Error> {
    let parts = if kind == Kind::Header {
        let mut parts = vec![
            raw.get(..1).unwrap_or_default(),
            raw.get(2..5).unwrap_or_default(),
        ];
        parts.extend(split(
            raw.get(6..).unwrap_or_default(),
            d.field,
            d.escape,
            limits.fields,
            number,
            0,
        )?);
        parts
    } else {
        split(raw, d.field, d.escape, limits.fields, number, 0)?
    };
    if parts.len() > limits.fields {
        return Err(Error::Limit {
            record: number,
            field: 0,
            kind: "fields",
        });
    }
    let mut fields = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let position = i.saturating_add(1);
        let repeats = if kind == Kind::Header && position == 2 {
            vec![vec![Component {
                raw: part.to_vec(),
                text: latin1(part).unwrap_or_default(),
            }]]
        } else {
            let mut repeats = Vec::new();
            for repeat in split(part, d.repeat, d.escape, limits.repeats, number, position)? {
                let mut components = Vec::new();
                for component in split(
                    repeat,
                    d.component,
                    d.escape,
                    limits.components,
                    number,
                    position,
                )? {
                    let expanded = expand(component, d);
                    let text = decode(&expanded).ok_or(Error::Decode {
                        record: number,
                        field: position,
                    })?;
                    components.push(Component {
                        raw: component.to_vec(),
                        text,
                    });
                }
                repeats.push(components);
            }
            repeats
        };
        fields.push(Field {
            raw: part.to_vec(),
            repeats,
        });
    }
    Ok(fields)
}
