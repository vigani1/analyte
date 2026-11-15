//! Native volume probing through bounded operating-system utilities.
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) struct Volume;

impl analyte_store::FreeSpace for Volume {
    fn available(&self, path: &Path) -> io::Result<u64> {
        let path = path.canonicalize()?;
        #[cfg(windows)]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("powershell.exe");
            // Pass the path as data, never as part of a script.
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Volume -FilePath $env:ANALYTE_VOLUME_PATH -ErrorAction Stop).SizeRemaining",
            ])
            .env("ANALYTE_VOLUME_PATH", path)
            .creation_flags(0x08000000);
            c
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("df");
            c.args(["--output=avail", "--block-size=1", "--"])
                .arg(path)
                .env("LC_ALL", "C");
            c
        };
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()?;
        let until = Instant::now()
            .checked_add(Duration::from_secs(3))
            .unwrap_or(Instant::now());
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= until {
                child.kill()?;
                child.wait()?;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "volume probe timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if !status.success() {
            return Err(io::Error::other("volume probe failed"));
        }
        let mut output = String::new();
        child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("volume probe output missing"))?
            .take(4097)
            .read_to_string(&mut output)?;
        if output.len() > 4096 {
            return Err(io::Error::other("volume probe output exceeds bound"));
        }
        #[cfg(windows)]
        let available = output.trim().parse::<u64>().map_err(io::Error::other)?;
        #[cfg(not(windows))]
        let available = output
            .lines()
            .nth(1)
            .ok_or_else(|| io::Error::other("invalid df output"))?
            .trim()
            .parse::<u64>()
            .map_err(io::Error::other)?;
        Ok(available)
    }
}
