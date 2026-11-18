//! Parses HL7 segments without transport or mapping.
use thiserror::Error;
mod text;
pub(crate) use text::validate_escapes;
pub use text::{TextEscape, decode_characters, decode_text, encode_text, inspect_text_escapes};

/// Syntax failures carry positions, never message contents.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// The edition is recognized, but its typed field rules are not implemented.
    #[error("typed field rules are not implemented for HL7 {0}; retain the raw message")]
    EditionRules(&'static str),
    /// The original query messages were withdrawn in this exact edition.
    #[error("HL7 {edition} withdraws original queries (section {section})")]
    WithdrawnQuery {
        /// Exact declared edition.
        edition: &'static str,
        /// Section withdrawing the original query messages.
        section: &'static str,
    },
    /// A configured resource ceiling was reached.
    #[error("{kind} limit exceeded at segment {segment}, field {field}")]
    Limit {
        /// Resource being bounded.
        kind: &'static str,
        /// One-based segment; zero before segmentation.
        segment: usize,
        /// One-based field; zero before fields.
        field: usize,
    },
    /// The first segment or its separators are invalid.
    #[error("invalid MSH encoding characters")]
    Encoding,
    /// The selected edition does not define a truncation character.
    #[error("truncation encoding character is not defined in HL7 2.3.1/2.5.1")]
    Truncation,
    /// A sender marked a primitive incomplete; it must not enter clinical mapping.
    #[error("sender-marked truncation at segment {segment}, field {field}")]
    Truncated {
        /// One-based segment position.
        segment: usize,
        /// One-based field position.
        field: usize,
    },
    /// A segment lacks its configured terminator.
    #[error("invalid segment terminator")]
    Terminator,
    /// A segment name or separator is invalid.
    #[error("invalid segment at position {0}")]
    Segment(usize),
    /// Escape grammar is invalid.
    #[error("malformed escape at segment {segment}, field {field}, byte {offset}")]
    Escape {
        /// One-based segment.
        segment: usize,
        /// One-based field.
        field: usize,
        /// Zero-based byte within field.
        offset: usize,
    },
    /// Character set declarations are not implemented.
    #[error("unsupported MSH-18 character set")]
    UnsupportedCharset,
    /// Bytes cannot be decoded in the declared character set.
    #[error("invalid bytes for declared character set")]
    CharacterEncoding,
    /// Connection and header encodings disagree.
    #[error("connection encoding disagrees with MSH-18")]
    CharsetMismatch,
    /// A typed control field is missing or invalid.
    #[error("invalid {segment}-{field}")]
    Control {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field.
        field: usize,
    },
    /// A caller attempted positive acknowledgment without storage.
    #[error("positive acknowledgment requires durable storage")]
    NotStored,
    /// Requested acknowledgment belongs to a different exchange phase.
    #[error("acknowledgment code does not match negotiated mode or condition")]
    AckMode,
}

/// Explicit wire line-ending policy; strict CR is recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    /// HL7 standard CR.
    Cr,
    /// Negotiated CRLF only.
    CrLf,
    /// Negotiated LF only.
    Lf,
}
/// Handling of malformed escapes; preservation never changes boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapePolicy {
    /// Reject malformed escape syntax.
    Reject,
    /// Retain malformed escape bytes verbatim.
    Preserve,
}
/// Resource ceilings applied before allocation and while scanning.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum wire bytes.
    pub message_bytes: usize,
    /// Maximum segments.
    pub segments: usize,
    /// Maximum fields per segment, including MSH-1.
    pub fields: usize,
    /// Maximum repetitions per field.
    pub repetitions: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            message_bytes: 1_048_576,
            segments: 4096,
            fields: 1024,
            repetitions: 1024,
        }
    }
}
/// Explicit parsing policies and bounded resource configuration.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Encoding negotiated for this connection, checked against MSH-18.
    pub charset: Charset,
    /// Wire line endings.
    pub terminator: Terminator,
    /// Malformed escapes.
    pub escapes: EscapePolicy,
    /// Explicit vendor departure allowing the later UTF-8 code in HL7 2.3.1.
    pub allow_utf8_in_v231: bool,
    /// Resource limits.
    pub limits: Limits,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            charset: Charset::Ascii,
            terminator: Terminator::Cr,
            escapes: EscapePolicy::Reject,
            allow_utf8_in_v231: false,
            limits: Limits::default(),
        }
    }
}
/// Distinct ASCII graphic delimiters taken from MSH-1/2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delimiters {
    pub(crate) field: u8,
    pub(crate) component: u8,
    pub(crate) repetition: u8,
    pub(crate) escape: u8,
    pub(crate) subcomponent: u8,
    pub(crate) truncation: Option<u8>,
}
impl Delimiters {
    /// Returns the optional fifth MSH-2 character, never an assumed default.
    #[must_use]
    pub fn truncation(self) -> Option<u8> {
        self.truncation
    }
    /// Returns field, component, repetition, escape and subcomponent bytes in that order.
    #[must_use]
    pub fn bytes(self) -> [u8; 5] {
        [
            self.field,
            self.component,
            self.repetition,
            self.escape,
            self.subcomponent,
        ]
    }
}
/// Supported default text encodings. Switching repertoires remains raw escape text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// Seven-bit ASCII, also the default when MSH-18 is absent.
    Ascii,
    /// ISO 8859-1.
    Latin1,
    /// Unicode UTF-8.
    Utf8,
}
/// A parsed message retains exact wire bytes, including trailing empties and line endings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    raw: Vec<u8>,
    delimiters: Delimiters,
    terminator: Terminator,
    charset: Charset,
}
impl Message {
    /// Parses syntax without I/O or clinical interpretation.
    /// # Errors
    /// Rejects invalid headers, delimiters, limits, escapes, versions and character encodings.
    pub fn parse(raw: &[u8], options: Options) -> Result<Self, Error> {
        Self::parse_with_profile(raw, options, false)
    }

    /// Applies LAW version compatibility while retaining the shared bounded syntax checks.
    pub(crate) fn parse_law(raw: &[u8], options: Options) -> Result<Self, Error> {
        Self::parse_with_profile(raw, options, true)
    }

    fn parse_with_profile(raw: &[u8], options: Options, law: bool) -> Result<Self, Error> {
        if raw.len() > options.limits.message_bytes {
            return Err(Error::Limit {
                kind: "message bytes",
                segment: 0,
                field: 0,
            });
        }
        if raw.get(..3) != Some(b"MSH") {
            return Err(Error::Encoding);
        }
        let field = *raw.get(3).ok_or(Error::Encoding)?;
        let encoding = raw
            .get(4..)
            .ok_or(Error::Encoding)?
            .split(|b| *b == field)
            .next()
            .ok_or(Error::Encoding)?;
        let [component, repetition, escape, subcomponent, rest @ ..] = encoding else {
            return Err(Error::Encoding);
        };
        if rest.len() > 1 {
            return Err(Error::Encoding);
        }
        let delimiters = Delimiters {
            field,
            component: *component,
            repetition: *repetition,
            escape: *escape,
            subcomponent: *subcomponent,
            truncation: rest.first().copied(),
        };
        let bytes = delimiters.bytes();
        for (i, byte) in bytes.iter().enumerate() {
            if !byte.is_ascii_graphic()
                || byte.is_ascii_alphanumeric()
                || bytes
                    .get(..i)
                    .is_some_and(|previous| previous.contains(byte))
            {
                return Err(Error::Encoding);
            }
        }
        if delimiters.truncation.is_some_and(|byte| {
            !byte.is_ascii_graphic() || byte.is_ascii_alphanumeric() || bytes.contains(&byte)
        }) {
            return Err(Error::Encoding);
        }
        let message = Self {
            raw: raw.to_vec(),
            delimiters,
            terminator: options.terminator,
            charset: Charset::Ascii,
        };
        message.validate_terminators()?;
        message.validate_segments(options)?;
        let msh = message.segments().next().ok_or(Error::Encoding)?;
        let version = msh.field(12).ok_or(Error::Control {
            segment: "MSH",
            field: 12,
        })?;
        let version_id = version
            .raw
            .split(|b| *b == delimiters.component)
            .next()
            .unwrap_or_default();
        // HL7 2.8.2 §2.14.9.2; 2.9.1 §2.13.9.2. Semantic edition
        // validation remains the responsibility of the typed MSH view.
        let modern_encoding = crate::Version::defines_truncation(version_id);
        if delimiters.truncation.is_some() && !modern_encoding {
            return Err(Error::Truncation);
        }
        if crate::Version::parse_wire(version_id, law).is_none() && !modern_encoding {
            return Err(Error::Control {
                segment: "MSH",
                field: 12,
            });
        }
        let charset = text::declared_charset(msh, version_id, options, law)?;
        match charset {
            Charset::Ascii if !raw.is_ascii() => return Err(Error::CharacterEncoding),
            Charset::Utf8 => {
                std::str::from_utf8(raw).map_err(|_| Error::CharacterEncoding)?;
            }
            Charset::Ascii | Charset::Latin1 => {}
        }
        Ok(Self { charset, ..message })
    }
    fn validate_terminators(&self) -> Result<(), Error> {
        let raw = self.raw.as_slice();
        let ending = self.ending();
        if !raw.ends_with(ending) {
            return Err(Error::Terminator);
        }
        if self.terminator == Terminator::CrLf {
            for (index, byte) in raw.iter().enumerate() {
                if (*byte == b'\r' && raw.get(index.saturating_add(1)) != Some(&b'\n'))
                    || (*byte == b'\n'
                        && index.checked_sub(1).and_then(|previous| raw.get(previous))
                            != Some(&b'\r'))
                {
                    return Err(Error::Terminator);
                }
            }
        }
        Ok(())
    }
    fn validate_segments(&self, options: Options) -> Result<(), Error> {
        let delimiters = self.delimiters;
        let field = delimiters.field;
        for (index, segment) in self.segments().enumerate() {
            let position = index.saturating_add(1);
            if position > options.limits.segments {
                return Err(Error::Limit {
                    kind: "segments",
                    segment: position,
                    field: 0,
                });
            }
            if segment.raw.iter().any(|b| *b == b'\r' || *b == b'\n') {
                return Err(Error::Terminator);
            }
            if segment
                .raw
                .get(..3)
                .is_none_or(|id| !id.iter().all(u8::is_ascii_alphanumeric))
                || (segment.raw.len() > 3 && segment.raw.get(3) != Some(&field))
                || (index != 0 && segment.id() == b"MSH")
            {
                return Err(Error::Segment(position));
            }
            for (field_index, value) in segment.fields().enumerate() {
                let number = field_index.saturating_add(1);
                if number > options.limits.fields {
                    return Err(Error::Limit {
                        kind: "fields",
                        segment: position,
                        field: number,
                    });
                }
                if segment.id() == b"MSH" && number <= 2 {
                    continue;
                }
                if value.repetitions().count() > options.limits.repetitions {
                    return Err(Error::Limit {
                        kind: "repetitions",
                        segment: position,
                        field: number,
                    });
                }
                if options.escapes == EscapePolicy::Reject {
                    validate_escapes(value.raw, delimiters, position, number)?;
                }
            }
        }
        Ok(())
    }
    fn ending(&self) -> &'static [u8] {
        match self.terminator {
            Terminator::Cr => b"\r",
            Terminator::CrLf => b"\r\n",
            Terminator::Lf => b"\n",
        }
    }
    /// Serializes exactly the accepted bytes; no normalization or loss occurs.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.raw.clone()
    }
    /// Borrows the original wire bytes.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Borrows the checked delimiter set.
    #[must_use]
    pub fn delimiters(&self) -> Delimiters {
        self.delimiters
    }
    /// Borrows the checked default character set.
    #[must_use]
    pub fn charset(&self) -> Charset {
        self.charset
    }
    /// Iterates segments in order, without allocating a syntax tree.
    pub fn segments(&self) -> impl Iterator<Item = Segment<'_>> {
        let end = match self.terminator {
            Terminator::Cr | Terminator::CrLf => b'\r',
            Terminator::Lf => b'\n',
        };
        self.raw
            .get(..self.raw.len().saturating_sub(self.ending().len()))
            .unwrap_or_default()
            .split(move |b| *b == end)
            .map(move |s| {
                let raw = if self.terminator == Terminator::CrLf {
                    s.strip_prefix(b"\n").unwrap_or(s)
                } else {
                    s
                };
                Segment {
                    raw,
                    delimiters: self.delimiters,
                }
            })
    }

    /// Refuses sender-marked incomplete data before typed interpretation or delivery.
    /// The original bytes remain available through `raw`.
    /// # Errors
    /// Returns the first marked field, excluding MSH delimiter declarations.
    pub fn require_complete_values(&self) -> Result<(), Error> {
        for (index, segment) in self.segments().enumerate() {
            for (number, field) in segment.fields().enumerate() {
                if segment.id() == b"MSH" && number < 2 {
                    continue;
                }
                if field.is_truncated() {
                    return Err(Error::Truncated {
                        segment: index.saturating_add(1),
                        field: number.saturating_add(1),
                    });
                }
            }
        }
        Ok(())
    }
}
/// Borrowed segment; all positions in this API are one-based.
#[derive(Debug, Clone, Copy)]
pub struct Segment<'a> {
    pub(crate) raw: &'a [u8],
    pub(crate) delimiters: Delimiters,
}
impl<'a> Segment<'a> {
    /// Returns the segment identifier bytes.
    #[must_use]
    pub fn id(self) -> &'a [u8] {
        self.raw.get(..3).unwrap_or_default()
    }
    /// Returns exact segment bytes without terminator.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    /// Iterates fields, treating MSH-1/2 as literal delimiter declarations.
    pub fn fields(self) -> impl Iterator<Item = Field<'a>> {
        let msh = self.id() == b"MSH";
        let first = msh.then(|| Field {
            raw: self.raw.get(3..4).unwrap_or_default(),
            delimiters: self.delimiters,
        });
        first.into_iter().chain(
            self.raw
                .get(4..)
                .into_iter()
                .flat_map(move |s| s.split(move |b| *b == self.delimiters.field))
                .map(move |raw| Field {
                    raw,
                    delimiters: self.delimiters,
                }),
        )
    }
    /// Looks up a field without confusing an absent field with an empty one.
    #[must_use]
    pub fn field(self, position: usize) -> Option<Field<'a>> {
        position
            .checked_sub(1)
            .and_then(|index| self.fields().nth(index))
    }
}
/// A raw field with positional repetitions intact.
#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    pub(crate) raw: &'a [u8],
    pub(crate) delimiters: Delimiters,
}
impl<'a> Field<'a> {
    /// Reports a sender truncation marker on any primitive, retaining `raw` unchanged.
    #[must_use]
    pub fn is_truncated(self) -> bool {
        self.repetitions()
            .any(|repetition| repetition.components().any(Component::is_truncated))
    }
    /// Returns exact field bytes.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    /// Distinguishes an explicit HL7 null from an empty value.
    #[must_use]
    pub fn is_null(self) -> bool {
        is_null_text(self.raw)
    }
    /// Iterates repetitions without decoding escapes into delimiters.
    pub fn repetitions(self) -> impl Iterator<Item = Repetition<'a>> {
        Parts::new(self.raw, self.delimiters.repetition, self.delimiters.escape).map(move |raw| {
            Repetition {
                raw,
                delimiters: self.delimiters,
            }
        })
    }
}
/// Borrowed repetition retaining component positions.
#[derive(Debug, Clone, Copy)]
pub struct Repetition<'a> {
    raw: &'a [u8],
    delimiters: Delimiters,
}
impl<'a> Repetition<'a> {
    /// Returns exact repetition bytes.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    /// Iterates components, including empty trailing components.
    pub fn components(self) -> impl Iterator<Item = Component<'a>> {
        Parts::new(self.raw, self.delimiters.component, self.delimiters.escape).map(move |raw| {
            Component {
                raw,
                delimiters: self.delimiters,
            }
        })
    }
}
/// Borrowed component retaining subcomponent positions.
#[derive(Debug, Clone, Copy)]
pub struct Component<'a> {
    raw: &'a [u8],
    delimiters: Delimiters,
}
impl<'a> Component<'a> {
    /// Reports an unescaped sender truncation marker on any primitive in this component.
    /// This conservative flag does not establish whether the datatype permits truncation.
    #[must_use]
    pub fn is_truncated(self) -> bool {
        self.delimiters
            .truncation
            .is_some_and(|marker| self.subcomponents().any(|raw| raw.last() == Some(&marker)))
    }
    /// Returns exact component bytes.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    /// Iterates raw subcomponents, including empty trailing subcomponents.
    pub fn subcomponents(self) -> impl Iterator<Item = &'a [u8]> {
        Parts::new(
            self.raw,
            self.delimiters.subcomponent,
            self.delimiters.escape,
        )
    }
}
struct Parts<'a> {
    remaining: Option<&'a [u8]>,
    separator: u8,
    escape: u8,
}
impl<'a> Parts<'a> {
    fn new(raw: &'a [u8], separator: u8, escape: u8) -> Self {
        Self {
            remaining: Some(raw),
            separator,
            escape,
        }
    }
}
impl<'a> Iterator for Parts<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        let raw = self.remaining.take()?;
        let mut escaped = false;
        for (i, b) in raw.iter().enumerate() {
            if *b == self.escape {
                escaped = !escaped;
            }
            if *b == self.separator && !escaped {
                self.remaining = raw.get(i.saturating_add(1)..);
                return raw.get(..i);
            }
        }
        Some(raw)
    }
}

/// Recognizes the explicit HL7 null token (HL7 2.5.1 section 2.5.3).
#[must_use]
pub fn is_null_text(raw: &[u8]) -> bool {
    raw == b"\"\""
}
