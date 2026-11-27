//! Asks for a missing choice at an interactive terminal, one question at a time, and reads keys without echo or from piped input.
//!
//! A command asks only when both its input and its output are a terminal, so
//! scripts and piped input are never prompted: a missing option is then the
//! page's own refusal. Every answer is checked by the same code as the page's
//! form, after it is given. Reads are bounded.
use analyte_i18n::{message as tr_args, text as tr};
use std::io::{self, BufRead as _, IsTerminal as _, Read as _, Write as _};

/// The longest answer read for one question; a key may be 4096 bytes.
const LINE_LIMIT: u64 = 4200;

/// How the command may ask for what is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ask {
    /// A person is at the terminal: ask, then confirm before saving.
    Terminal,
    /// Piped input or output, as in a script: never ask.
    Never,
}

/// One numbered choice: the value the command uses and the words shown.
pub(super) struct Choice {
    pub(super) value: String,
    pub(super) label: String,
}

impl Ask {
    /// [`Ask::Terminal`] when standard input and output are both a terminal.
    pub(super) fn detect() -> Self {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            Self::Terminal
        } else {
            Self::Never
        }
    }

    /// Asks `question` and returns the trimmed answer; `None` when no one can
    /// be asked or nothing was typed.
    pub(super) fn line(self, question: &str) -> io::Result<Option<String>> {
        if self == Self::Never {
            return Ok(None);
        }
        prompt(&format!("{question}: "))?;
        let answer = read_line(&mut io::stdin().lock())?;
        let answer = answer.trim();
        Ok((!answer.is_empty()).then(|| answer.to_owned()))
    }

    /// Lists `choices` numbered from 1 and returns the chosen value; the only
    /// choice is offered as the answer to keep. `None` when no one can be
    /// asked, there is nothing to choose, or the answer is not a listed number.
    pub(super) fn choose(self, question: &str, choices: &[Choice]) -> io::Result<Option<String>> {
        if self == Self::Never || choices.is_empty() {
            return Ok(None);
        }
        let mut out = io::stdout().lock();
        writeln!(out, "{question}")?;
        for (number, choice) in choices.iter().enumerate() {
            writeln!(out, "  {}. {}", number.saturating_add(1), choice.label)?;
        }
        drop(out);
        let only = choices.len() == 1;
        let answer = self.line(if only {
            "Number [1]"
        } else {
            tr("setup-number")
        })?;
        let number = match answer {
            None if only => 1,
            None => return Ok(None),
            Some(text) => match text.parse::<usize>() {
                Ok(number) => number,
                Err(_) => return Ok(None),
            },
        };
        Ok(number
            .checked_sub(1)
            .and_then(|index| choices.get(index))
            .map(|choice| choice.value.clone()))
    }

    /// Asks a yes or no question; only "y" or "yes" is yes. Never asked: no.
    pub(super) fn confirm(self, question: &str) -> io::Result<bool> {
        Ok(self
            .line(&format!("{question} [y/N]"))?
            .is_some_and(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes")))
    }
}

/// Reads `names.len()` keys: at a terminal one prompt each with nothing
/// echoed, otherwise one line each from standard input. An empty answer keeps
/// the stored key. Values are never printed or logged.
pub(super) fn keys(ask: Ask, names: &[&str]) -> io::Result<Vec<String>> {
    match ask {
        Ask::Terminal => names
            .iter()
            .map(|name| {
                rpassword::prompt_password(tr_args(
                    "setup-nothing-is-shown-as-you-type-leave-empty-to",
                    [("name", (name).to_string().into())],
                ))
                .map(|value| value.trim().to_owned())
            })
            .collect(),
        Ask::Never => {
            let mut input = io::stdin().lock();
            names
                .iter()
                .map(|_| read_line(&mut input).map(|line| line.trim().to_owned()))
                .collect()
        }
    }
}

fn prompt(text: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    write!(out, "{text}")?;
    out.flush()
}

/// One line of at most [`LINE_LIMIT`] bytes, without its line ending; a longer
/// line is refused rather than cut, since a cut key would be a wrong key.
fn read_line(input: &mut impl io::BufRead) -> io::Result<String> {
    let mut line = String::new();
    input.by_ref().take(LINE_LIMIT).read_line(&mut line)?;
    if line.len() as u64 >= LINE_LIMIT && !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            tr("setup-an-answer-is-longer-than-4200-characters"),
        ));
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::read_line;

    // AGENTS.md rule 7: answers and piped keys are read with a bound, line by
    // line, and an overlong one is refused rather than cut.
    #[test]
    fn piped_lines_are_bounded_and_kept_whole() -> Result<(), Box<dyn std::error::Error>> {
        let mut input = std::io::Cursor::new(b"first\r\nsecond\n".to_vec());
        assert_eq!(read_line(&mut input)?, "first");
        assert_eq!(read_line(&mut input)?, "second");
        assert_eq!(read_line(&mut input)?, "");
        let long = vec![b'k'; 5000];
        assert!(read_line(&mut std::io::Cursor::new(long)).is_err());
        Ok(())
    }
}
