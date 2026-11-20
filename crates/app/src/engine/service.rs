//! Asks Windows' service manager about the Analyte service, and to start it, through `sc.exe` with a time limit.
//!
//! Starting is the only change the app can ask for, and only because the
//! installer gives signed-in users the right to start (never to stop or
//! change) the service. The app never runs the engine itself. `sc.exe`
//! answers with fixed English state names and Windows error numbers, which
//! is what is read here.
use std::time::Duration;

/// The service's name, as `install.ps1` registers it.
pub const NAME: &str = "Analyte";

/// How long one `sc.exe` call may take before it is stopped. Starting only
/// asks the service manager; it does not wait for the engine.
pub const LIMIT: Duration = Duration::from_secs(10);

/// The most of `sc.exe`'s output that is read.
const OUTPUT_LIMIT: usize = 16 * 1024;

/// What the service manager said about the service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Service {
    /// The service is stopped.
    Stopped,
    /// The service is starting.
    Starting,
    /// The service runs.
    Running,
    /// The service is stopping.
    Stopping,
    /// Paused, or a state this app does not name.
    Other,
    /// No service with this name is installed.
    Missing,
    /// The service manager could not be asked, or its answer not read.
    Unreadable(String),
}

impl Service {
    /// What the app observed, in plain words.
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::Stopped => "The Analyte service is stopped.".into(),
            Self::Starting => "The Analyte service is starting.".into(),
            Self::Running => "The Analyte service runs, but its admin pages do not answer.".into(),
            Self::Stopping => "The Analyte service is stopping.".into(),
            Self::Other => "The Analyte service is neither running nor stopped.".into(),
            Self::Missing => "No Analyte service is installed on this computer.".into(),
            Self::Unreadable(why) => {
                format!("The app could not ask Windows about the Analyte service: {why}")
            }
        }
    }
}

/// What a start request came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Started {
    /// The service manager accepted the request; the engine answers once it is up.
    Requested,
    /// It was already running.
    AlreadyRunning,
    /// This Windows account may not start it.
    NotAllowed,
    /// No service with this name is installed.
    Missing,
    /// Anything else, with what `sc.exe` said.
    Failed(String),
}

impl Started {
    /// What happened, in plain words.
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::Requested => {
                "Windows was asked to start the engine. This screen changes once it answers.".into()
            }
            Self::AlreadyRunning => "Windows says the engine is already running.".into(),
            Self::NotAllowed => concat!(
                "Your Windows account may not start the engine. Ask an administrator ",
                "to start the Analyte service, or to install Analyte again, which lets ",
                "signed-in users start it."
            )
            .into(),
            Self::Missing => "No Analyte service is installed on this computer.".into(),
            Self::Failed(why) => format!("The engine could not be started: {why}"),
        }
    }
}

/// Asks for the service's state.
pub async fn query() -> Service {
    match sc(&["query", NAME]).await {
        Ok((code, text)) => read_query(code, &text),
        Err(why) => Service::Unreadable(why),
    }
}

/// Asks the service manager to start the service.
pub async fn start() -> Started {
    match sc(&["start", NAME]).await {
        Ok((code, text)) => read_start(code, &text),
        Err(why) => Started::Failed(why),
    }
}

/// Windows error numbers `sc.exe` exits with.
const ACCESS_DENIED: i32 = 5;
const ALREADY_RUNNING: i32 = 1056;
const NOT_INSTALLED: i32 = 1060;

fn read_query(code: Option<i32>, text: &str) -> Service {
    match code {
        Some(0) => {}
        Some(NOT_INSTALLED) => return Service::Missing,
        _ => return Service::Unreadable(summary(code, text)),
    }
    // The STATE line reads like "STATE : 4  RUNNING"; the names are not translated.
    let state = text
        .lines()
        .filter(|line| line.contains(':'))
        .flat_map(str::split_whitespace)
        .find_map(|word| match word {
            "STOPPED" => Some(Service::Stopped),
            "START_PENDING" => Some(Service::Starting),
            "RUNNING" => Some(Service::Running),
            "STOP_PENDING" => Some(Service::Stopping),
            "PAUSED" | "PAUSE_PENDING" | "CONTINUE_PENDING" => Some(Service::Other),
            _ => None,
        });
    state.unwrap_or_else(|| Service::Unreadable("sc.exe named no state".into()))
}

fn read_start(code: Option<i32>, text: &str) -> Started {
    match code {
        Some(0) => Started::Requested,
        Some(ALREADY_RUNNING) => Started::AlreadyRunning,
        Some(ACCESS_DENIED) => Started::NotAllowed,
        Some(NOT_INSTALLED) => Started::Missing,
        _ => Started::Failed(summary(code, text)),
    }
}

/// `sc.exe`'s exit code and last non-empty line, for a failure nobody named.
fn summary(code: Option<i32>, text: &str) -> String {
    let last = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("no output");
    match code {
        Some(code) => format!("sc.exe exit code {code}: {last}"),
        None => format!("sc.exe stopped without an exit code: {last}"),
    }
}

/// Runs `%SystemRoot%\System32\sc.exe` with `arguments`, hidden, within
/// [`LIMIT`]; gives its exit code and output. A run past the limit is
/// stopped, and the error says so.
async fn sc(arguments: &[&str]) -> Result<(Option<i32>, String), String> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let program = std::path::Path::new(&root).join("System32").join("sc.exe");
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // No console window flashes up for a tray app.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = match tokio::time::timeout(LIMIT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("sc.exe could not run: {error}")),
        Err(_) => return Err(format!("sc.exe took longer than {} s", LIMIT.as_secs())),
    };
    let shown = output.stdout.get(..OUTPUT_LIMIT).unwrap_or(&output.stdout);
    Ok((
        output.status.code(),
        String::from_utf8_lossy(shown).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: &str = "\r\nSERVICE_NAME: Analyte \r\n        TYPE               : 10  WIN32_OWN_PROCESS  \r\n        STATE              : 4  RUNNING \r\n                                (STOPPABLE, NOT_PAUSABLE, ACCEPTS_SHUTDOWN)\r\n        WIN32_EXIT_CODE    : 0  (0x0)\r\n";

    // Microsoft sc.exe query: the STATE line names the state; exit 1060 means
    // no such service.
    #[test]
    fn query_reads_the_state_line() {
        assert_eq!(read_query(Some(0), RUNNING), Service::Running);
        assert_eq!(
            read_query(Some(0), &RUNNING.replace("4  RUNNING", "1  STOPPED")),
            Service::Stopped
        );
        assert_eq!(
            read_query(Some(0), &RUNNING.replace("4  RUNNING", "2  START_PENDING")),
            Service::Starting
        );
        assert_eq!(read_query(Some(1060), "FAILED 1060"), Service::Missing);
        assert!(matches!(
            read_query(Some(0), "nothing useful"),
            Service::Unreadable(_)
        ));
        assert!(matches!(
            read_query(Some(5), "FAILED 5"),
            Service::Unreadable(_)
        ));
    }

    // Microsoft sc.exe start: Windows error numbers for already running,
    // access denied and no such service.
    #[test]
    fn start_reads_the_exit_code() {
        assert_eq!(read_start(Some(0), ""), Started::Requested);
        assert_eq!(read_start(Some(1056), ""), Started::AlreadyRunning);
        assert_eq!(read_start(Some(5), ""), Started::NotAllowed);
        assert_eq!(read_start(Some(1060), ""), Started::Missing);
        assert_eq!(
            read_start(
                Some(1053),
                "[SC] StartService FAILED 1053:\r\n\r\nThe service did not respond.\r\n"
            ),
            Started::Failed("sc.exe exit code 1053: The service did not respond.".into())
        );
    }
}
