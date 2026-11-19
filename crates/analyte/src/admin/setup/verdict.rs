//! Guesses a machine's protocol family from the first bytes of a connection test and says it in plain words.
//!
//! The guess is a hint for people, never a decision: it looks only at how the
//! stream starts, with the framing crates' own start markers, and answers
//! "unknown" for anything else. It reads no values and keeps no bytes.
use crate::admin::view::Tone;
use analyte_config::Stack;
use analyte_framing::{astm, hl7, vidas};

/// A protocol family Analyte speaks, as far as a listen-only test can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Astm,
    Hl7,
    Vidas,
    Poct,
    Autolyser,
}

impl Family {
    /// The family a validated machine stack speaks; IHE LAW is HL7 over MLLP.
    pub(super) fn of(stack: &Stack) -> Self {
        match stack {
            Stack::Astm(_) => Self::Astm,
            Stack::Hl7(_) | Stack::Law(_) => Self::Hl7,
            Stack::Poct(_) => Self::Poct,
            Stack::Vidas(_) => Self::Vidas,
            Stack::Autolyser(_) => Self::Autolyser,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Astm => "ASTM",
            Self::Hl7 => "HL7",
            Self::Vidas => "VIDAS",
            Self::Poct => "POCT1-A",
            Self::Autolyser => "Autolyser",
        }
    }
}

/// What the first bytes of a test look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Guess {
    /// Nothing received.
    Nothing,
    /// The start marker of one family.
    Family(Family),
    /// Only ENQ so far: ASTM and VIDAS both ask to send this way, and the test
    /// never answers, so nothing more decides between them.
    AstmOrVidas,
    /// Bytes that start like no family Analyte knows.
    Unknown,
}

/// The guess looks at no more than this many leading bytes.
const PREFIX: usize = 64;

/// Classifies how `bytes` start; pure and bounded to the first [`PREFIX`] bytes.
pub(super) fn classify(bytes: &[u8]) -> Guess {
    let bytes = bytes.get(..PREFIX.min(bytes.len())).unwrap_or_default();
    if bytes.is_empty() {
        return Guess::Nothing;
    }
    // A sender repeats ENQ while nobody answers; what follows may still decide.
    let after_enq = bytes
        .iter()
        .position(|&byte| byte != astm::ENQ)
        .map_or(&[][..], |at| bytes.get(at..).unwrap_or_default());
    let asked = after_enq.len() < bytes.len();
    if let Some(family) = framed(after_enq) {
        return Guess::Family(family);
    }
    if asked && after_enq.is_empty() {
        return Guess::AstmOrVidas;
    }
    if !asked && poct(bytes) {
        return Guess::Family(Family::Poct);
    }
    Guess::Unknown
}

/// A frame or block start: ASTM, VIDAS or HL7 over MLLP.
fn framed(bytes: &[u8]) -> Option<Family> {
    match bytes {
        [astm::STX, number, ..] if number.is_ascii_digit() => Some(Family::Astm),
        [vidas::STX, rest @ ..] => {
            // VIDAS may end each control byte with CR, LF or CR LF before the first RS.
            let rest = rest
                .strip_prefix(&[vidas::CR, vidas::LF])
                .or_else(|| rest.strip_prefix(&[vidas::CR]))
                .or_else(|| rest.strip_prefix(&[vidas::LF]))
                .unwrap_or(rest);
            (rest.first() == Some(&vidas::RS)).then_some(Family::Vidas)
        }
        [hl7::SB, rest @ ..] if rest.starts_with(b"MSH|") => Some(Family::Hl7),
        _ => None,
    }
}

/// POCT1-A messages are XML; a device opens with its HEL.R01 hello.
fn poct(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let start = text
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(text.len());
    let text = text.get(start..).unwrap_or_default();
    text.starts_with(b"<?xml") || text.starts_with(b"<HEL.R01")
}

/// The plain verdict for a test of a machine whose model speaks `model`.
pub(super) fn verdict(guess: Guess, model: Family) -> (Tone, String) {
    let receiving = |seen: &str| {
        if fits(guess, model) {
            (
                Tone::Good,
                format!(
                    "Receiving data from the machine, looks like {}.",
                    model.name()
                ),
            )
        } else {
            (
                Tone::Attention,
                format!(
                    "Receiving data from the machine. It looks like {seen}, but this model uses {}: check the model or the machine's setting.",
                    model.name()
                ),
            )
        }
    };
    match guess {
        Guess::Nothing if model == Family::Autolyser => (
            Tone::Attention,
            concat!(
                "Nothing received yet. That is expected for this model: it waits for ",
                "Analyte to ask first, and the test never sends anything."
            )
            .into(),
        ),
        Guess::Nothing => (
            Tone::Attention,
            "Nothing received yet: check the cable and the machine's host settings.".into(),
        ),
        Guess::Family(family) => receiving(family.name()),
        Guess::AstmOrVidas => receiving("ASTM or VIDAS"),
        Guess::Unknown => (
            Tone::Attention,
            concat!(
                "Receiving data, but it doesn't look like a format Analyte knows. ",
                "Check the machine's host protocol setting."
            )
            .into(),
        ),
    }
}

/// Whether what was seen can be the model's own protocol.
fn fits(guess: Guess, model: Family) -> bool {
    match guess {
        Guess::Family(family) => family == model,
        Guess::AstmOrVidas => matches!(model, Family::Astm | Family::Vidas),
        Guess::Nothing | Guess::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Family, Guess, Tone, classify, verdict};

    // CLSI LIS01-A2 §6.2 and §6.3.1.1: a sender asks with ENQ, then each frame
    // starts with STX and a digit frame number.
    #[test]
    fn astm_enq_and_frames_are_astm() {
        assert_eq!(classify(b"\x05"), Guess::AstmOrVidas);
        assert_eq!(classify(b"\x05\x05\x05"), Guess::AstmOrVidas);
        assert_eq!(classify(b"\x05\x021H|\\^&\r"), Guess::Family(Family::Astm));
        assert_eq!(classify(b"\x021H|\\^&\r\x03"), Guess::Family(Family::Astm));
        assert_eq!(classify(b"\x027R|1\r"), Guess::Family(Family::Astm));
    }

    // MLLP Release 1 §1.2: a block starts with VT (0x0B); HL7 v2 chapter 2: a
    // message starts with its MSH segment.
    #[test]
    fn mllp_block_with_msh_is_hl7() {
        assert_eq!(
            classify(b"\x0bMSH|^~\\&|LAB|SYN\r"),
            Guess::Family(Family::Hl7)
        );
        // Without the MLLP start block it is not claimed as HL7.
        assert_eq!(classify(b"MSH|^~\\&|LAB|SYN\r"), Guess::Unknown);
        assert_eq!(classify(b"\x0bPID|1\r"), Guess::Unknown);
    }

    // bioMérieux BCI Appendix A chapter 3: a VIDAS packet is STX, an optional
    // CR/LF suffix, then RS-led chunks; an upload starts with ENQ.
    #[test]
    fn vidas_packets_are_vidas() {
        assert_eq!(classify(b"\x02\x1emtrsl|"), Guess::Family(Family::Vidas));
        assert_eq!(classify(b"\x02\r\n\x1emt"), Guess::Family(Family::Vidas));
        assert_eq!(classify(b"\x02\r\x1emt"), Guess::Family(Family::Vidas));
        assert_eq!(classify(b"\x05\x02\n\x1emt"), Guess::Family(Family::Vidas));
    }

    // POCT1-A Appendix B §4.4: messages are XML and a device opens with HEL.R01.
    #[test]
    fn xml_hello_is_poct() {
        assert_eq!(
            classify(b"<?xml version=\"1.0\"?><HEL.R01>"),
            Guess::Family(Family::Poct)
        );
        assert_eq!(classify(b"\r\n  <HEL.R01>"), Guess::Family(Family::Poct));
        assert_eq!(
            classify(b"\xef\xbb\xbf<?xml version"),
            Guess::Family(Family::Poct)
        );
    }

    // Owner request 2026-11-19 (item 5): nothing, garbage, or a half-seen
    // marker is never given a family.
    #[test]
    fn empty_and_garbage_are_not_guessed() {
        assert_eq!(classify(b""), Guess::Nothing);
        assert_eq!(classify(b"hello world"), Guess::Unknown);
        assert_eq!(classify(&[0xff, 0x00, 0x13, 0x37]), Guess::Unknown);
        assert_eq!(classify(b"\x02"), Guess::Unknown);
        assert_eq!(classify(b"\x02AB"), Guess::Unknown);
        assert_eq!(classify(b"\x05garbage"), Guess::Unknown);
        assert_eq!(classify(b"x<?xml"), Guess::Unknown);
        // Only the first bytes count: a marker far into the stream is ignored.
        let mut late = vec![b'x'; 100];
        late.extend_from_slice(b"\x0bMSH|");
        assert_eq!(classify(&late), Guess::Unknown);
    }

    // Owner request 2026-11-19 (item 5): the verdict says plainly what was seen,
    // and warns when it is not the model's protocol.
    #[test]
    fn verdict_names_the_family_and_warns_on_a_mismatch() {
        let astm = Guess::Family(Family::Astm);
        assert_eq!(
            verdict(astm, Family::Astm),
            (
                Tone::Good,
                "Receiving data from the machine, looks like ASTM.".into()
            )
        );
        assert_eq!(
            verdict(Guess::AstmOrVidas, Family::Vidas).1,
            "Receiving data from the machine, looks like VIDAS."
        );
        assert_eq!(
            verdict(Guess::Family(Family::Hl7), Family::Astm),
            (
                Tone::Attention,
                "Receiving data from the machine. It looks like HL7, but this model uses ASTM: check the model or the machine's setting.".into()
            )
        );
        assert!(
            verdict(Guess::AstmOrVidas, Family::Hl7)
                .1
                .contains("It looks like ASTM or VIDAS, but this model uses HL7")
        );
        assert_eq!(
            verdict(Guess::Nothing, Family::Hl7).1,
            "Nothing received yet: check the cable and the machine's host settings."
        );
        assert!(
            verdict(Guess::Nothing, Family::Autolyser)
                .1
                .contains("it waits for Analyte to ask first")
        );
        assert!(
            verdict(Guess::Unknown, Family::Poct)
                .1
                .contains("doesn't look like a format Analyte knows")
        );
    }
}
