//! Finds POCT XML root message delimiters without validating message syntax.
use quick_xml::{Reader, events::Event};

/// A message exceeds the host's bounded framing policy.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Stream ended before a complete XML root.
    #[error("incomplete POCT root")]
    Incomplete,
    /// Message byte bound exceeded.
    #[error("POCT message limit exceeded")]
    Limit,
    /// XML cannot become valid by appending bytes.
    #[error("malformed POCT XML: {0}")]
    Malformed(#[source] quick_xml::Error),
    /// Element depth bound exceeded.
    #[error("POCT XML depth limit exceeded")]
    Depth,
    /// A closing element has no open root.
    #[error("POCT unbalanced XML root")]
    Unbalanced,
}

/// Scans an append-only, host-owned message buffer without revisiting complete tokens.
/// Only incomplete tokens remain pending for more bytes.
#[derive(Debug)]
pub struct Decoder {
    stream: Vec<u8>,
    cursor: usize,
    depth: usize,
    maximum: usize,
    maximum_depth: usize,
}

impl Decoder {
    /// Sets explicit byte and element-depth bounds.
    #[must_use]
    pub fn new(maximum: usize, maximum_depth: usize) -> Self {
        Self {
            stream: Vec::new(),
            cursor: 0,
            depth: 0,
            maximum,
            maximum_depth,
        }
    }

    /// Returns exact pending bytes when the owning dialog stops or rejects input.
    pub fn take_pending(&mut self) -> Vec<u8> {
        self.reset();
        std::mem::take(&mut self.stream)
    }

    /// Starts the next root after the host consumes the previous message.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.depth = 0;
    }

    /// Returns the complete root's byte length, retaining incomplete tokens.
    /// Call with the same buffer plus appended bytes until completion, then reset.
    /// # Errors
    /// Rejects byte/depth limit violations and an unbalanced closing root.
    pub fn scan(&mut self, raw: &[u8]) -> Result<Option<usize>, Error> {
        if raw.len() > self.maximum {
            return Err(Error::Limit);
        }
        loop {
            let tail = raw.get(self.cursor..).ok_or(Error::Limit)?;
            let mut reader = Reader::from_reader(tail);
            reader.config_mut().check_end_names = false;
            reader.config_mut().allow_unmatched_ends = true;
            let event = match reader.read_event() {
                Ok(Event::Eof) => return Ok(None),
                Err(error) if incomplete(&error, tail) => return Ok(None),
                Err(error) => return Err(Error::Malformed(error)),
                Ok(event) => event,
            };
            let consumed = usize::try_from(reader.buffer_position()).map_err(|_| Error::Limit)?;
            // A trailing text/reference fragment may still grow. Retain it until
            // a following markup delimiter makes its token boundary definite.
            if matches!(event, Event::Text(_) | Event::GeneralRef(_)) && consumed == tail.len() {
                return Ok(None);
            }
            self.cursor = self.cursor.checked_add(consumed).ok_or(Error::Limit)?;
            match event {
                Event::Start(_) => {
                    self.depth = self.depth.checked_add(1).ok_or(Error::Depth)?;
                    if self.depth > self.maximum_depth {
                        return Err(Error::Depth);
                    }
                }
                Event::End(_) => {
                    if self.depth == 0 {
                        return Err(Error::Unbalanced);
                    }
                    self.depth = self.depth.checked_sub(1).ok_or(Error::Unbalanced)?;
                    if self.depth == 0 {
                        return Ok(Some(self.cursor));
                    }
                }
                Event::Empty(_) if self.depth == 0 => return Ok(Some(self.cursor)),
                Event::Empty(_)
                | Event::Text(_)
                | Event::CData(_)
                | Event::Comment(_)
                | Event::Decl(_)
                | Event::PI(_)
                | Event::DocType(_)
                | Event::GeneralRef(_) => {}
                Event::Eof => return Ok(None),
            }
        }
    }
}

// quick-xml distinguishes EOF-truncated constructs from definite syntax errors.
// A partial bang introducer needs special handling before the reader knows its kind.
fn incomplete(error: &quick_xml::Error, tail: &[u8]) -> bool {
    use quick_xml::{Error as XmlError, errors::SyntaxError};
    match error {
        XmlError::Syntax(SyntaxError::InvalidBangMarkup) => {
            [b"<!--".as_slice(), b"<![CDATA[", b"<!DOCTYPE"]
                .iter()
                .any(|prefix| prefix.starts_with(tail))
        }
        XmlError::Syntax(
            SyntaxError::UnclosedPI
            | SyntaxError::UnclosedXmlDecl
            | SyntaxError::UnclosedComment
            | SyntaxError::UnclosedDoctype
            | SyntaxError::UnclosedCData
            | SyntaxError::UnclosedTag
            | SyntaxError::UnclosedSingleQuotedAttributeValue
            | SyntaxError::UnclosedDoubleQuotedAttributeValue,
        ) => true,
        XmlError::Io(_)
        | XmlError::IllFormed(_)
        | XmlError::InvalidAttr(_)
        | XmlError::Encoding(_)
        | XmlError::Escape(_)
        | XmlError::Namespace(_) => false,
    }
}

impl Decoder {
    /// Consumes one byte, returning a complete frame or a specific refusal.
    /// # Errors
    /// Reports invalid or overlong input.
    pub fn push(&mut self, byte: u8) -> Result<Option<Vec<u8>>, Error> {
        if self.stream.len() >= self.maximum {
            return Err(Error::Limit);
        }
        self.stream.push(byte);
        if byte != b'>' {
            return Ok(None);
        }
        let raw = std::mem::take(&mut self.stream);
        let result = self.scan(&raw);
        match result {
            Ok(Some(_)) => {
                self.reset();
                Ok(Some(raw))
            }
            Ok(None) => {
                self.stream = raw;
                Ok(None)
            }
            Err(error) => {
                self.stream = raw;
                Err(error)
            }
        }
    }
    /// Ends decoding and refuses incomplete evidence.
    /// # Errors
    /// Returns the protocol-specific incomplete-frame error.
    pub fn finish(&mut self) -> Result<Option<Vec<u8>>, Error> {
        if self.stream.is_empty() {
            Ok(None)
        } else {
            Err(Error::Incomplete)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CLSI POCT01-A2 Appendix B XML
    #[test]
    fn root_found_across_splits_and_comments() -> Result<(), Error> {
        for raw in [
            b"<a><b V=\"x>y\"/></a>".as_slice(),
            b"<!-- > --><a><!-- </a> --></a>",
            b"<a><b></c></wrong>",
            b"<a/>",
        ] {
            for split in 0..raw.len() {
                let mut decoder = Decoder::new(1024, 16);
                assert_eq!(decoder.scan(&raw[..split])?, None);
                assert_eq!(decoder.scan(raw)?, Some(raw.len()));
            }
        }
        Ok(())
    }

    // CLSI POCT01-A2 Appendix B XML
    #[test]
    fn definite_xml_errors_are_refused() {
        assert!(matches!(
            Decoder::new(1024, 16).scan(b"<a><!invalid>"),
            Err(Error::Malformed(_))
        ));
        assert!(matches!(
            Decoder::new(1024, 16).scan(b"</root>"),
            Err(Error::Unbalanced)
        ));
    }

    // CLSI POCT01-A2 Appendix B XML
    #[test]
    fn incomplete_roots_remain_pending() -> Result<(), Error> {
        for raw in [
            b"<a><!-- unfinished".as_slice(),
            b"<a><",
            b"<a><![CDATA[unfinished",
            b"<a",
        ] {
            let mut decoder = Decoder::new(1024, 16);
            assert_eq!(decoder.scan(raw)?, None);
        }
        assert!(matches!(
            Decoder::new(1024, 1).scan(b"<a><b>"),
            Err(Error::Depth)
        ));
        assert!(matches!(
            Decoder::new(1024, 1).scan(b"</a>"),
            Err(Error::Unbalanced)
        ));
        Ok(())
    }
}

#[cfg(test)]
mod equivalence {
    use super::*;
    use proptest::prelude::*;
    /// Locates a complete XML root without treating quoted text or comments as framing.
    /// Syntax and POCT containment remain the protocol crate's responsibility.
    fn original_frame_length(raw: &[u8], maximum_depth: usize) -> Result<Option<usize>, Error> {
        let mut reader = Reader::from_reader(raw);
        // Boundaries need balanced depth even for a mismatched tag name. The syntax
        // reader rejects it afterwards, while retaining the complete rejected root.
        reader.config_mut().check_end_names = false;
        reader.config_mut().allow_unmatched_ends = true;
        let mut depth = 0usize;
        loop {
            let position = usize::try_from(reader.buffer_position()).map_err(|_| Error::Limit)?;
            match reader.read_event() {
                Ok(Event::Start(_)) => {
                    depth = depth.checked_add(1).ok_or(Error::Depth)?;
                    if depth > maximum_depth {
                        return Err(Error::Depth);
                    }
                }
                Ok(Event::End(_)) => {
                    depth = depth.checked_sub(1).ok_or(Error::Unbalanced)?;
                    if depth == 0 {
                        return Ok(Some(
                            usize::try_from(reader.buffer_position()).map_err(|_| Error::Limit)?,
                        ));
                    }
                }
                Ok(Event::Empty(_)) if depth == 0 => {
                    return Ok(Some(
                        usize::try_from(reader.buffer_position()).map_err(|_| Error::Limit)?,
                    ));
                }
                // The external XML reader can reject a partial token. Keep its bytes
                // until the bounded message/deadline gate; never discard that evidence.
                Ok(Event::Eof) => return Ok(None),
                Err(error) if incomplete(&error, raw.get(position..).ok_or(Error::Limit)?) => {
                    return Ok(None);
                }
                Err(error) => return Err(Error::Malformed(error)),
                Ok(
                    Event::Empty(_)
                    | Event::Text(_)
                    | Event::CData(_)
                    | Event::Comment(_)
                    | Event::Decl(_)
                    | Event::PI(_)
                    | Event::DocType(_)
                    | Event::GeneralRef(_),
                ) => {}
            }
        }
    }

    proptest! {
        // CLSI POCT01-A2 Appendix B XML
        #[test]
        fn incremental_scan_matches_reference_boundary(tokens in prop::collection::vec(prop::sample::select(vec!["<a>","</a>","<b/>","<b",">","<", "<!--", "-->","<![CDATA[", "]]>","&amp;", "text", "</wrong>","<?xml version=\"1.0\"?>"]), 0..40)) {
            let raw = tokens.concat().into_bytes();
            let mut decoder = Decoder::new(4096, 16);
            for (i, byte) in raw.iter().enumerate() {
                if *byte != b'>' { continue; }
                let prefix = &raw[..=i];
                let expected = original_frame_length(prefix,16);
                let actual = decoder.scan(prefix);
                prop_assert_eq!(actual.as_ref().ok(),expected.as_ref().ok());
                if !matches!(expected, Ok(None)) { break; }
            }
        }
    }
}
