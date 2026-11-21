//! The sign-in pipe's two lines as the app writes and reads them (ADR 0021 §2): its request and the engine's answer, each one JSON object of at most 1 KiB ending in a newline.
//!
//! The engine reads requests and writes answers; the app does the opposite,
//! so each program keeps its own half and neither depends on the other. The
//! wire shape has one source, `crates/testing/fixtures/windows-sign-in.txt`:
//! the engine's tests and these read the same examples, so a change on one
//! side fails a test until the file and the other side follow.

/// The most bytes either line may take, its newline included.
pub const LIMIT: usize = 1024;

/// The request as sent: `{"v":1,"next":"/parked/12"}`.
#[derive(serde::Serialize)]
struct Wire<'a> {
    v: u8,
    next: &'a str,
}

/// The request for the Overview, the fallback for a page too long to ask for.
const HOME: &[u8] = b"{\"v\":1,\"next\":\"/\"}\n";

/// The request line asking to sign in and land on `next`, newline included,
/// never over [`LIMIT`] bytes: a `next` that would not fit asks for the
/// Overview instead. The engine checks `next` again and keeps only a local
/// page, so the app does not judge it.
#[must_use]
pub fn request(next: &str) -> Vec<u8> {
    match serde_json::to_vec(&Wire { v: 1, next }) {
        Ok(mut line) if line.len() < LIMIT => {
            line.push(b'\n');
            line
        }
        Ok(_) | Err(_) => HOME.to_vec(),
    }
}

/// A one-time sign-in code: it opens a session for 20 s, so it is never
/// logged and its `Debug` hides it.
pub struct Code(String);

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Code(hidden)")
    }
}

impl Code {
    /// The code's text, for the address that redeems it.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0
    }
}

/// Why the engine gave no code; each is one fixed word on the wire, and the
/// engine's sign-in page has one fixed sentence for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// The account is in neither allowed group.
    NotAllowed,
    /// The engine refused the request line.
    BadRequest,
    /// Windows could not say who is asking.
    Unidentified,
    /// The engine could not answer in time or could not make a code.
    Unavailable,
}

impl Refusal {
    /// The word as the wire and `/login?windows=<word>` spell it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::NotAllowed => "not-allowed",
            Self::BadRequest => "bad-request",
            Self::Unidentified => "unidentified",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The engine's answer.
#[derive(Debug)]
pub enum Answer {
    /// The account may sign in with this code.
    Code(Code),
    /// No code, for this reason.
    Refused(Refusal),
}

/// The answer as sent: a code with its account, or a refusal with the
/// account when the engine knows it. The engine's own pages show the account
/// once signed in, so the app reads it only to check the shape and drops it.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Out {
    Code(CodeOut),
    Refused(RefusedOut),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeOut {
    code: String,
    #[serde(rename = "account")]
    _account: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RefusedOut {
    refused: Refusal,
    #[serde(default, rename = "account")]
    _account: Option<String>,
}

/// Why an answer line was refused.
#[derive(Debug, thiserror::Error)]
pub enum AnswerError {
    /// More than [`LIMIT`] bytes came before a newline.
    #[error("the sign-in answer is longer than 1 KiB")]
    TooLong,
    /// The line ended without its newline.
    #[error("the sign-in answer does not end in a newline")]
    Unterminated,
    /// Not a JSON object: serde would read a struct from an array too.
    #[error("the sign-in answer is not a JSON object")]
    NotObject,
    /// Not a code with its account, nor a known refusal word.
    #[error("the sign-in answer is neither a code nor a known refusal")]
    Malformed(#[source] serde_json::Error),
    /// A code with no text.
    #[error("the sign-in answer holds an empty code")]
    EmptyCode,
}

/// Reads one answer line, newline included.
///
/// # Errors
///
/// [`AnswerError`] for an oversized, unterminated or malformed line, an
/// unknown refusal word or an empty code.
pub fn answer(line: &[u8]) -> Result<Answer, AnswerError> {
    if line.len() > LIMIT {
        return Err(AnswerError::TooLong);
    }
    let body = line.strip_suffix(b"\n").ok_or(AnswerError::Unterminated)?;
    if body.first() != Some(&b'{') {
        return Err(AnswerError::NotObject);
    }
    match serde_json::from_slice(body).map_err(AnswerError::Malformed)? {
        Out::Code(CodeOut { code, .. }) if code.is_empty() => Err(AnswerError::EmptyCode),
        Out::Code(CodeOut { code, .. }) => Ok(Answer::Code(Code(code))),
        Out::Refused(RefusedOut { refused, .. }) => Ok(Answer::Refused(refused)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One example line from the file the engine's tests read too, with its newline.
    fn example(name: &str) -> String {
        let examples = include_str!("../../../testing/fixtures/windows-sign-in.txt");
        let found = examples
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(' '));
        format!("{}\n", found.expect("the example exists"))
    }

    fn refusal(line: &str) -> Option<Refusal> {
        match answer(line.as_bytes()) {
            Ok(Answer::Refused(refusal)) => Some(refusal),
            Ok(Answer::Code(_)) | Err(_) => None,
        }
    }

    // ADR 0021 §2: the request is one JSON line with exactly v = 1 and next,
    // spelled as the engine reads it.
    #[test]
    fn request_is_the_engines_line() {
        assert_eq!(request("/parked/12"), example("request-parked").as_bytes());
        assert_eq!(request("/"), example("request-home").as_bytes());
    }

    // ADR 0021 §2: the request never passes 1 KiB; a page too long to ask
    // for becomes the Overview, which the engine would choose anyway.
    #[test]
    fn request_is_bounded() {
        let fits = format!("/{}", "a".repeat(LIMIT - 19));
        assert_eq!(request(&fits).len(), LIMIT);
        let long = format!("/{}", "a".repeat(LIMIT));
        assert_eq!(request(&long), HOME);
        assert_eq!(request("/\"\n"), b"{\"v\":1,\"next\":\"/\\\"\\n\"}\n");
    }

    // ADR 0021 §2: the engine's code answer gives the code; its account is
    // read past and not kept.
    #[test]
    fn answer_reads_the_engines_code() -> Result<(), AnswerError> {
        match answer(example("answer-code").as_bytes())? {
            Answer::Code(code) => assert_eq!(code.text(), format!("{}=", "q".repeat(43))),
            Answer::Refused(refused) => panic!("a code was read as {refused:?}"),
        }
        Ok(())
    }

    // ADR 0021 §2: the four refusal words, with or without the account, and
    // the same words the engine's sign-in page takes back.
    #[test]
    fn answer_reads_the_engines_refusals() {
        for (name, expected) in [
            ("answer-not-allowed", Refusal::NotAllowed),
            ("answer-bad-request", Refusal::BadRequest),
            ("answer-unidentified", Refusal::Unidentified),
            ("answer-unavailable", Refusal::Unavailable),
        ] {
            let line = example(name);
            assert_eq!(refusal(&line), Some(expected), "{line}");
            assert!(line.contains(&format!("\"{}\"", expected.word())), "{line}");
        }
    }

    // ADR 0021 §2: anything but one known answer object is refused, never
    // guessed: unknown words and fields, a code without its account, arrays,
    // broken JSON and an empty code.
    #[test]
    fn answer_refuses_anything_else() {
        for line in [
            &b"{\"refused\":\"Not-Allowed\"}\n"[..],
            b"{\"refused\":\"maybe\"}\n",
            b"{\"code\":\"abc\"}\n",
            b"{\"code\":\"abc\",\"account\":\"x\",\"admin\":true}\n",
            b"{\"refused\":\"unavailable\",\"code\":\"abc\"}\n",
            b"{}\n",
            b"{\"code\":\n",
        ] {
            assert!(
                matches!(answer(line), Err(AnswerError::Malformed(_))),
                "{line:?}"
            );
        }
        for line in [&b"[\"abc\",\"x\"]\n"[..], b"\"abc\"\n", b"\n"] {
            assert!(
                matches!(answer(line), Err(AnswerError::NotObject)),
                "{line:?}"
            );
        }
        assert!(matches!(
            answer(b"{\"code\":\"\",\"account\":\"x\"}\n"),
            Err(AnswerError::EmptyCode)
        ));
    }

    // ADR 0021 §2: an answer must end in its newline and stay within 1 KiB.
    #[test]
    fn answer_is_bounded_and_terminated() {
        assert!(matches!(
            answer(b"{\"refused\":\"unavailable\"}"),
            Err(AnswerError::Unterminated)
        ));
        let long = format!(
            "{{\"refused\":\"unavailable\",\"account\":\"{}\"}}\n",
            "a".repeat(LIMIT)
        );
        assert!(matches!(answer(long.as_bytes()), Err(AnswerError::TooLong)));
    }
}
