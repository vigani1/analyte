//! The sign-in pipe's two lines (ADR 0021 §2): the app's request and the engine's answer, each one JSON object of at most 1 KiB ending in a newline.

/// The most bytes either line may take, its newline included.
pub(in crate::admin) const LIMIT: usize = 1024;

/// The request as sent: `{"v":1,"next":"/parked/12"}`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    v: u64,
    next: String,
}

/// What the app asked for.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::admin) struct Request {
    /// Where the app wants to land once signed in.
    pub(in crate::admin) next: Next,
}

/// The page the app asked to open after signing in, exactly as sent. It is not
/// yet checked as a local path: that is the job of whatever keeps the code,
/// since only a redeemed code ever opens it.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::admin) struct Next(String);

impl Next {
    /// The path as the app sent it.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read when a code is redeemed (ADR 0021 §4)")
    )]
    pub(in crate::admin) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a request line was refused.
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum LineError {
    /// More than [`LIMIT`] bytes came before a newline.
    #[error("the sign-in request is longer than 1 KiB")]
    TooLong,
    /// The line ended without its newline.
    #[error("the sign-in request does not end in a newline")]
    Unterminated,
    /// Not a JSON object: serde would read a struct from an array too.
    #[error("the sign-in request is not a JSON object")]
    NotObject,
    /// Not one JSON object with exactly `v` and `next`.
    #[error("the sign-in request is not the expected JSON object")]
    Malformed(#[source] serde_json::Error),
    /// A request format this engine does not know.
    #[error("the sign-in request has version {0}; this engine reads version 1")]
    Version(u64),
}

/// Reads one request line, newline included.
/// # Errors
/// [`LineError`] for an oversized, unterminated, malformed or other-version line.
pub(in crate::admin) fn request(line: &[u8]) -> Result<Request, LineError> {
    if line.len() > LIMIT {
        return Err(LineError::TooLong);
    }
    let body = line.strip_suffix(b"\n").ok_or(LineError::Unterminated)?;
    if body.first() != Some(&b'{') {
        return Err(LineError::NotObject);
    }
    let wire: Wire = serde_json::from_slice(body).map_err(LineError::Malformed)?;
    if wire.v != 1 {
        return Err(LineError::Version(wire.v));
    }
    Ok(Request {
        next: Next(wire.next),
    })
}

/// A one-time sign-in code: secret until redeemed, so it has no `Debug`.
pub(in crate::admin) struct Code(String);

impl Code {
    /// Wraps freshly minted code text.
    pub(in crate::admin) fn new(text: String) -> Self {
        Self(text)
    }
}

/// Why a caller got no code; each is one fixed word on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::admin) enum Refusal {
    /// The account is in neither allowed group.
    NotAllowed,
    /// The request line was refused.
    BadRequest,
    /// Windows could not say who the caller is.
    Unidentified,
    /// The engine could not answer in time or could not mint a code.
    Unavailable,
}

/// The engine's answer.
pub(in crate::admin) enum Answer {
    /// The caller may sign in with `code`.
    Code {
        /// The one-time code.
        code: Code,
        /// `DOMAIN\name`, or the SID when Windows gave no name.
        account: String,
    },
    /// No code.
    Refused {
        /// Why.
        refused: Refusal,
        /// Who asked, when the engine knows.
        account: Option<String>,
    },
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum Out<'a> {
    Code {
        code: &'a str,
        account: &'a str,
    },
    Refused {
        refused: Refusal,
        #[serde(skip_serializing_if = "Option::is_none")]
        account: Option<&'a str>,
    },
}

/// The fallback when an answer cannot be written within [`LIMIT`].
const UNAVAILABLE: &[u8] = b"{\"refused\":\"unavailable\"}\n";

/// The answer line, newline included, never over [`LIMIT`] bytes: an answer
/// that would not fit becomes `{"refused":"unavailable"}`.
pub(in crate::admin) fn answer(answer: &Answer) -> Vec<u8> {
    let out = match answer {
        Answer::Code { code, account } => Out::Code {
            code: &code.0,
            account,
        },
        Answer::Refused { refused, account } => Out::Refused {
            refused: *refused,
            account: account.as_deref(),
        },
    };
    match serde_json::to_vec(&out) {
        Ok(mut line) if line.len() < LIMIT => {
            line.push(b'\n');
            line
        }
        Ok(_) | Err(_) => UNAVAILABLE.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0021 §2: the request is one JSON line with exactly v = 1 and next.
    #[test]
    fn request_reads_version_and_next() -> Result<(), LineError> {
        let request = request(b"{\"v\":1,\"next\":\"/parked/12\"}\n")?;
        assert_eq!(request.next.as_str(), "/parked/12");
        Ok(())
    }

    // ADR 0021 §2: unknown fields, other versions and broken JSON are refused.
    #[test]
    fn request_refuses_unknown_fields_versions_and_bad_json() {
        for line in [
            &b"{\"v\":1,\"next\":\"/\",\"user\":\"LAB-PC\\\\maria\"}\n"[..],
            b"{\"next\":\"/\"}\n",
            b"{\"v\":1}\n",
            b"{\"v\":1,\"next\":\"/\"\n",
            b"{\xff}\n",
            b"{\"v\":1,\"next\":\"/\"}\n{}\n",
        ] {
            assert!(
                matches!(request(line), Err(LineError::Malformed(_))),
                "{line:?}"
            );
        }
        // serde reads a struct from an array too, so only an object passes.
        for line in [
            &b"[1,\"/\"]\n"[..],
            b" {\"v\":1,\"next\":\"/\"}\n",
            b"\xff\n",
        ] {
            assert!(
                matches!(request(line), Err(LineError::NotObject)),
                "{line:?}"
            );
        }
        assert!(matches!(
            request(b"{\"v\":2,\"next\":\"/\"}\n"),
            Err(LineError::Version(2))
        ));
    }

    // ADR 0021 §2: a line must end in its newline and stay within 1 KiB.
    #[test]
    fn request_is_bounded_and_terminated() {
        assert!(matches!(
            request(b"{\"v\":1,\"next\":\"/\"}"),
            Err(LineError::Unterminated)
        ));
        let next = "a".repeat(LIMIT);
        let long = format!("{{\"v\":1,\"next\":\"{next}\"}}\n");
        assert!(matches!(request(long.as_bytes()), Err(LineError::TooLong)));
        let fits = format!("{{\"v\":1,\"next\":\"{}\"}}\n", "a".repeat(LIMIT - 18));
        assert_eq!(fits.len(), LIMIT);
        assert!(request(fits.as_bytes()).is_ok());
    }

    // ADR 0021 §4: the answer is one line: a code with the account, or a fixed
    // refusal word with the account when known.
    #[test]
    fn answers_are_one_fixed_shape_line() {
        let code = Answer::Code {
            code: Code::new("q".repeat(43) + "="),
            account: "LAB-PC\\maria".into(),
        };
        let expected = format!(
            "{{\"code\":\"{}=\",\"account\":\"LAB-PC\\\\maria\"}}\n",
            "q".repeat(43)
        );
        assert_eq!(answer(&code), expected.as_bytes());
        let refused = Answer::Refused {
            refused: Refusal::NotAllowed,
            account: Some("LAB-PC\\bob".into()),
        };
        assert_eq!(
            answer(&refused),
            b"{\"refused\":\"not-allowed\",\"account\":\"LAB-PC\\\\bob\"}\n"
        );
        let unknown = Answer::Refused {
            refused: Refusal::BadRequest,
            account: None,
        };
        assert_eq!(answer(&unknown), b"{\"refused\":\"bad-request\"}\n");
    }

    // ADR 0021 §2: the app reads four fixed refusal words and nothing else.
    #[test]
    fn refusal_words_are_fixed() {
        for (refused, word) in [
            (Refusal::NotAllowed, "not-allowed"),
            (Refusal::BadRequest, "bad-request"),
            (Refusal::Unidentified, "unidentified"),
            (Refusal::Unavailable, "unavailable"),
        ] {
            let line = answer(&Answer::Refused {
                refused,
                account: None,
            });
            assert_eq!(line, format!("{{\"refused\":\"{word}\"}}\n").as_bytes());
        }
    }

    // ADR 0021 §2: an answer that would pass 1 KiB is replaced, never cut.
    #[test]
    fn oversized_answer_becomes_unavailable() {
        let long = Answer::Refused {
            refused: Refusal::NotAllowed,
            account: Some("\\".repeat(LIMIT)),
        };
        assert_eq!(answer(&long), UNAVAILABLE);
    }
}
