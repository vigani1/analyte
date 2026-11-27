//! Prints `analyte logs`: where the engine's log is and its last lines, read from at most the file's last 1 MiB.
//!
//! The Windows service writes `<data_directory>/analyte.log`; on Linux the
//! service writes to the system journal. Logs hold no patient data (safety
//! rule 10: names, counts and IDs, never messages or values), so printing
//! them is safe; they stay as protected as the data directory itself.
use crate::admin::Error;
use crate::installation::config::Program;
use analyte_i18n::{message as tr_args, text as tr};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

/// The most bytes read from the end of the log: plenty for 1000 lines.
const TAIL_BYTES: u64 = 1024 * 1024;

/// The log file the Windows service writes.
pub(super) fn path(program: &Program) -> PathBuf {
    program.data_directory().join("analyte.log")
}

pub(super) async fn run(program: &Program, tail: usize) -> Result<(), Error> {
    let path = path(program);
    let read = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || last_lines(&path, tail)).await?
    };
    match read {
        Ok(lines) => {
            super::say(&format!("Log: {}", path.display()))?;
            for line in lines {
                super::say(&line)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            super::say(&tr_args(
                "cli-no-log-file-at",
                [("path", (path.display()).to_string().into())],
            ))?;
            super::say(tr("cli-the-windows-service-writes-it-while-it-runs-on"))?;
            super::say(&format!("journalctl -u analyte -n {tail}"))?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// The last `count` lines of the file at `path`, reading at most
/// [`TAIL_BYTES`] from its end; a line cut by that limit is left out.
fn last_lines(path: &Path, count: usize) -> std::io::Result<Vec<String>> {
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(count);
    Ok(lines.into_iter().skip(skip).map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Safety rule 7: the read is bounded and keeps only the last lines.
    #[test]
    fn last_lines_keep_the_newest_and_drop_a_cut_line() -> Result<(), crate::Error> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("analyte.log");
        std::fs::write(&path, "one\ntwo\nthree\n")?;
        assert_eq!(last_lines(&path, 2)?, ["two", "three"]);
        assert_eq!(last_lines(&path, 10)?, ["one", "two", "three"]);
        let long = format!("{}\nlast\n", "x".repeat(usize::try_from(TAIL_BYTES)? + 10));
        std::fs::write(&path, long)?;
        assert_eq!(last_lines(&path, 5)?, ["last"]);
        Ok(())
    }
}
