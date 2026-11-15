//! POCT01-A2 Appendix B XML root boundaries; syntax remains authoritative.
use quick_xml::{Reader, events::Event};

/// A message exceeds the host's bounded framing policy.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Message byte bound exceeded.
    #[error("POCT message limit exceeded")]
    Limit,
    /// Element depth bound exceeded.
    #[error("POCT XML depth limit exceeded")]
    Depth,
    /// A closing element has no open root.
    #[error("POCT unbalanced XML root")]
    Unbalanced,
}

/// Scans an append-only, host-owned message buffer without revisiting complete tokens.
/// Incomplete or malformed tokens remain pending for syntax or the host deadline.
#[derive(Debug)]
pub struct Decoder {
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
            cursor: 0,
            depth: 0,
            maximum,
            maximum_depth,
        }
    }

    /// Starts the next root after the host consumes the previous message.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.depth = 0;
    }

    /// Returns the complete root's byte length, retaining partial/malformed tokens.
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
                Ok(Event::Eof) | Err(_) => return Ok(None),
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
                        self.cursor = self.cursor.saturating_sub(consumed);
                        return Ok(None);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poct01_b_xml_split_tokens_comments_and_mismatched_roots() -> Result<(), Error> {
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

    #[test]
    fn poct01_b_xml_partial_and_malformed_roots_remain_pending() -> Result<(), Error> {
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
        assert_eq!(Decoder::new(1024, 1).scan(b"</a>")?, None);
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
        let mut depth = 0usize;
        loop {
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
                Ok(Event::Eof) | Err(_) => return Ok(None),
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
        #[test]
        fn poct01_b_xml_incremental_matches_original_boundary(tokens in prop::collection::vec(prop::sample::select(vec!["<a>","</a>","<b/>","<b",">","<", "<!--", "-->","<![CDATA[", "]]>","&amp;", "text", "</wrong>","<?xml version=\"1.0\"?>"]), 0..40)) {
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
