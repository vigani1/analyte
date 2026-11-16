//! Parses bounded POCT XML without dialog decisions.
pub(crate) mod structure;
use quick_xml::{Reader, events::Event};

/// Hard allocation and work limits, applied before copying input.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum complete document size in bytes.
    pub message_bytes: usize,
    /// Maximum open element count.
    pub depth: usize,
    /// Maximum total element count.
    pub elements: usize,
    /// Maximum encoded attribute value size.
    pub attribute_bytes: usize,
}

/// Syntax failures contain no patient payloads.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The document exceeds the configured byte limit.
    #[error("message byte limit exceeded")]
    MessageLimit,
    /// Nesting exceeds the configured limit.
    #[error("element depth limit exceeded")]
    DepthLimit,
    /// Element count exceeds the configured limit.
    #[error("element count limit exceeded")]
    ElementLimit,
    /// An attribute exceeds the configured byte limit.
    #[error("attribute byte limit exceeded")]
    AttributeLimit,
    /// DTDs are forbidden, including internal entity declarations.
    #[error("DOCTYPE is forbidden")]
    Doctype,
    /// XML syntax is malformed; the host can inspect the protected raw receipt.
    #[error("malformed XML")]
    Syntax,
    /// Only UTF-8 XML 1.0 is supported by this syntax boundary.
    #[error("unsupported XML declaration")]
    Declaration,
    /// Exactly one balanced root is required.
    #[error("document must contain exactly one balanced root")]
    Root,
    /// Processing instructions are not part of supported DML syntax.
    #[error("processing instructions are unsupported")]
    ProcessingInstruction,
}

/// One element in document order, with its parent index and decoded attributes.
/// This is syntax evidence, not a semantically accepted DML object.
#[derive(PartialEq, Eq)]
pub struct Element {
    name: String,
    parent: Option<usize>,
    attributes: Vec<(String, String)>,
    text: String,
    first_text: String,
    text_ended: bool,
}

impl Element {
    /// Returns the element name without changing case.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parent index into the document's element list.
    #[must_use]
    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// Returns decoded attributes in their received order.
    #[must_use]
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    /// Returns direct character content, excluding descendant content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns initial direct XML text, before any child element.
    #[must_use]
    pub fn first_text(&self) -> &str {
        &self.first_text
    }
}

/// A bounded, balanced XML document alongside its exact received bytes.
/// Debug deliberately omits content because it can contain patient data.
#[derive(PartialEq, Eq)]
pub struct Document {
    raw: Vec<u8>,
    elements: Vec<Element>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document").finish_non_exhaustive()
    }
}

impl Document {
    /// Parses one complete UTF-8 XML document without recursion or I/O.
    /// No external entity or schema is ever resolved.
    /// # Errors
    /// Rejects malformed XML, declarations other than UTF-8 XML 1.0, DTDs,
    /// unsupported processing instructions and configured limit violations.
    /// Names are restricted to the ASCII DML vocabulary (no namespaces).
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Self, Error> {
        if bytes.len() > limits.message_bytes {
            return Err(Error::MessageLimit);
        }
        let input = std::str::from_utf8(bytes).map_err(|_| Error::Syntax)?;
        if input.chars().any(|c| !xml_character(c)) {
            return Err(Error::Syntax);
        }
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut reader = Reader::from_str(input);
        reader.config_mut().check_comments = true;
        let mut elements = Vec::<Element>::new();
        let mut stack = Vec::new();
        let mut roots = 0usize;
        let mut declaration = false;
        loop {
            let event = reader.read_event().map_err(|_| Error::Syntax)?;
            let empty = matches!(&event, Event::Empty(_));
            match event {
                Event::Start(start) | Event::Empty(start) => {
                    if elements.len() >= limits.elements {
                        return Err(Error::ElementLimit);
                    }
                    if stack.len() >= limits.depth {
                        return Err(Error::DepthLimit);
                    }
                    if stack.is_empty() {
                        roots = roots.checked_add(1).ok_or(Error::Root)?;
                        if roots > 1 {
                            return Err(Error::Root);
                        }
                    }
                    let (name, attributes) = parse_start(&start, limits)?;
                    end_initial_text(&mut elements, &stack)?;
                    let index = elements.len();
                    elements.push(Element {
                        name: name.to_owned(),
                        parent: stack.last().copied(),
                        attributes,
                        text: String::new(),
                        first_text: String::new(),
                        text_ended: false,
                    });
                    if !empty {
                        stack.push(index);
                    }
                }
                Event::End(_) => {
                    stack.pop().ok_or(Error::Root)?;
                }
                Event::Text(text) => {
                    if text.as_ref().contains("]]>") {
                        return Err(Error::Syntax);
                    }
                    let decoded = text.xml_content(quick_xml::XmlVersion::Implicit1_0);
                    append_text(&mut elements, &stack, &decoded)?;
                }
                Event::GeneralRef(reference) => {
                    let name = reference.as_ref();
                    let encoded = format!("&{name};");
                    let decoded =
                        quick_xml::escape::unescape(&encoded).map_err(|_| Error::Syntax)?;
                    append_text(&mut elements, &stack, &decoded)?;
                }
                Event::CData(text) => {
                    if stack.is_empty() {
                        return Err(Error::Root);
                    }
                    if text.as_ref().contains("]]>") {
                        return Err(Error::Syntax);
                    }
                    let decoded = text.xml_content(quick_xml::XmlVersion::Implicit1_0);
                    append_text(&mut elements, &stack, &decoded)?;
                }
                Event::Decl(decl) => {
                    validate_declaration(
                        &decl,
                        declaration,
                        &elements,
                        reader.buffer_position(),
                        limits,
                    )?;
                    declaration = true;
                }
                Event::DocType(_) => return Err(Error::Doctype),
                Event::PI(_) => return Err(Error::ProcessingInstruction),
                Event::Comment(_) => {
                    end_initial_text(&mut elements, &stack)?;
                }
                Event::Eof => break,
            }
        }
        if roots != 1 || !stack.is_empty() {
            return Err(Error::Root);
        }
        Ok(Self {
            raw: bytes.to_vec(),
            elements,
        })
    }

    /// Returns all original bytes, including whitespace and entity spelling.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Returns elements in document order; parent indices always precede children.
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
}

fn parse_start(
    start: &quick_xml::events::BytesStart<'_>,
    limits: Limits,
) -> Result<(String, Vec<(String, String)>), Error> {
    let name = start.name().as_ref().to_owned();
    let name = name.as_str();
    if !valid_name(name) {
        return Err(Error::Syntax);
    }
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| Error::Syntax)?;
        if attribute.value.len() > limits.attribute_bytes {
            return Err(Error::AttributeLimit);
        }
        if attribute.value.contains('<') {
            return Err(Error::Syntax);
        }
        let key = attribute.key.as_ref();
        if !valid_name(key) && !matches!(key, "xml:lang" | "xml:lang-T" | "xml:lang-HL7_NAME") {
            return Err(Error::Syntax);
        }
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|_| Error::Syntax)?;
        if value.chars().any(|c| !xml_character(c)) {
            return Err(Error::Syntax);
        }
        attributes.push((key.to_owned(), value.into_owned()));
    }
    Ok((name.to_owned(), attributes))
}

fn validate_declaration(
    decl: &quick_xml::events::BytesDecl<'_>,
    declaration: bool,
    elements: &[Element],
    position: u64,
    limits: Limits,
) -> Result<(), Error> {
    if declaration
        || !elements.is_empty()
        || position
            != u64::try_from(decl.as_ref().len())
                .map_err(|_| Error::Declaration)?
                .checked_add(4)
                .ok_or(Error::Declaration)?
    {
        return Err(Error::Declaration);
    }
    let attributes = quick_xml::events::BytesStart::from_content(decl.as_ref(), 3);
    let mut order = 0usize;
    for attribute in attributes.attributes() {
        let attribute = attribute.map_err(|_| Error::Declaration)?;
        if attribute.value.len() > limits.attribute_bytes {
            return Err(Error::AttributeLimit);
        }
        let rank = match attribute.key.as_ref() {
            "version" => 1,
            "encoding" => 2,
            "standalone" => 3,
            _ => return Err(Error::Declaration),
        };
        if rank <= order {
            return Err(Error::Declaration);
        }
        order = rank;
        if rank == 3 && !matches!(attribute.value.as_ref(), "yes" | "no") {
            return Err(Error::Declaration);
        }
    }
    if decl.version().map_err(|_| Error::Declaration)?.as_ref() != "1.0" {
        return Err(Error::Declaration);
    }
    if let Some(encoding) = decl.encoding()
        && !encoding
            .map_err(|_| Error::Declaration)?
            .eq_ignore_ascii_case("UTF-8")
    {
        return Err(Error::Declaration);
    }
    Ok(())
}

// ED uses an initial text child only; element and comment nodes end that child.
fn end_initial_text(elements: &mut [Element], stack: &[usize]) -> Result<(), Error> {
    if let Some(index) = stack.last().copied() {
        elements.get_mut(index).ok_or(Error::Root)?.text_ended = true;
    }
    Ok(())
}

fn append_text(elements: &mut [Element], stack: &[usize], text: &str) -> Result<(), Error> {
    if text.chars().any(|c| !xml_character(c)) {
        return Err(Error::Syntax);
    }
    if let Some(index) = stack.last() {
        let element = elements.get_mut(*index).ok_or(Error::Root)?;
        element.text.push_str(text);
        if !element.text_ended {
            element.first_text.push_str(text);
        }
    } else if !text
        .bytes()
        .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return Err(Error::Root);
    }
    Ok(())
}

pub(crate) fn xml_character(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}
