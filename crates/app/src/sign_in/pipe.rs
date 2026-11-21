//! Windows only: asks the engine's sign-in pipe once (ADR 0021 §2), waiting at most 3 s for its one instance and then at most 2 s for the answer.
//!
//! Cancelling [`ask`] (the app quits) drops the pipe, which closes it; an
//! engine that already made a code keeps it unused until it expires after
//! 20 s. A timeout never proves the engine made no code, and the app never
//! asks again on its own because of one: the guard in the parent module
//! decides that.
use super::line::{self, Answer, AnswerError, LIMIT};
use std::{io, time::Duration};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::time::Instant;

/// How long the app waits for each part of a sign-in.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// For the pipe's one instance while it serves someone else.
    pub connect: Duration,
    /// For sending the request and reading the answer once connected; the
    /// engine answers within 1 s of reading the request.
    pub exchange: Duration,
}

/// The app's limits: 3 s for a busy pipe, 2 s for the answer.
pub const LIMITS: Limits = Limits {
    connect: Duration::from_secs(3),
    exchange: Duration::from_secs(2),
};

/// The pause between tries while the instance is busy.
const RETRY: Duration = Duration::from_millis(50);

/// winerror.h `ERROR_PIPE_BUSY`: the one instance is serving another caller
/// or is between callers.
const PIPE_BUSY: i32 = 231;

/// winerror.h `ERROR_PIPE_NOT_CONNECTED`: the engine dropped the caller.
const PIPE_NOT_CONNECTED: i32 = 233;

/// winbase.h `SECURITY_IDENTIFICATION` (`SecurityIdentification << 16`): the
/// engine may learn who the app's user is, never act as them. It is also
/// tokio's default; it is set here so a change of that default cannot widen it.
const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;

/// Why the pipe gave no answer. Each is shown as the key form; the pipe being
/// absent is not a refusal, as the engine may simply have no Windows sign-in.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    /// No pipe of that name: the engine is stopped or has no Windows sign-in.
    #[error("the engine's sign-in pipe does not exist")]
    Absent,
    /// The one instance stayed busy for [`Limits::connect`].
    #[error("the engine's sign-in pipe stayed busy for {0:?}")]
    Busy(Duration),
    /// Windows refused to open the pipe for another reason.
    #[error("the engine's sign-in pipe could not be opened")]
    Open(#[source] io::Error),
    /// The request could not be written.
    #[error("the sign-in request could not be sent")]
    Write(#[source] io::Error),
    /// The answer could not be read.
    #[error("the sign-in answer could not be read")]
    Read(#[source] io::Error),
    /// The engine closed the pipe before a whole answer line.
    #[error("the engine closed the sign-in pipe without an answer")]
    Closed,
    /// No whole answer within [`Limits::exchange`].
    #[error("the engine gave no sign-in answer within {0:?}")]
    Silent(Duration),
    /// The answer line was refused.
    #[error(transparent)]
    Answer(#[from] AnswerError),
}

/// Asks the pipe `name` for a code to land on `next`.
///
/// # Errors
///
/// [`AskError`] when there is no pipe, it stays busy, it fails, it stays
/// silent past its limit or its answer is not a valid line.
pub async fn ask(name: &str, next: &str, limits: Limits) -> Result<Answer, AskError> {
    let pipe = connect(name, limits.connect).await?;
    let request = line::request(next);
    tokio::time::timeout(limits.exchange, exchange(&pipe, &request))
        .await
        .map_err(|_| AskError::Silent(limits.exchange))?
}

/// Opens the pipe at identification level, trying again while its one
/// instance is busy, for at most `budget`.
async fn connect(name: &str, budget: Duration) -> Result<NamedPipeClient, AskError> {
    let started = Instant::now();
    loop {
        match ClientOptions::new()
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(name)
        {
            Ok(pipe) => return Ok(pipe),
            Err(error) if error.raw_os_error() == Some(PIPE_BUSY) => {
                if started.elapsed().saturating_add(RETRY) > budget {
                    return Err(AskError::Busy(budget));
                }
                tokio::time::sleep(RETRY).await;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(AskError::Absent),
            Err(error) => return Err(AskError::Open(error)),
        }
    }
}

/// Writes `request` and reads one answer line of at most [`LIMIT`] bytes.
async fn exchange(pipe: &NamedPipeClient, request: &[u8]) -> Result<Answer, AskError> {
    let mut rest = request;
    while !rest.is_empty() {
        pipe.writable().await.map_err(AskError::Write)?;
        match pipe.try_write(rest) {
            Ok(written) => rest = rest.get(written..).unwrap_or_default(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(AskError::Write(error)),
        }
    }
    let mut answer = Vec::with_capacity(LIMIT);
    let mut chunk = [0u8; 256];
    loop {
        pipe.readable().await.map_err(AskError::Read)?;
        let read = match pipe.try_read(&mut chunk) {
            Ok(0) => return Err(AskError::Closed),
            Ok(read) => chunk.get(..read).unwrap_or_default(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            Err(error)
                if error.kind() == io::ErrorKind::BrokenPipe
                    || error.raw_os_error() == Some(PIPE_NOT_CONNECTED) =>
            {
                return Err(AskError::Closed);
            }
            Err(error) => return Err(AskError::Read(error)),
        };
        if let Some(end) = read.iter().position(|&byte| byte == b'\n') {
            answer.extend_from_slice(read.get(..=end).unwrap_or_default());
            return Ok(line::answer(&answer)?);
        }
        answer.extend_from_slice(read);
        if answer.len() > LIMIT {
            return Err(AnswerError::TooLong.into());
        }
    }
}

/// A fake engine end of the pipe, for the app's tests.
#[cfg(test)]
pub(super) mod fake {
    use super::Limits;
    use std::time::Duration;
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    /// Short limits, so the deadline tests stay quick.
    pub(in crate::sign_in) const QUICK: Limits = Limits {
        connect: Duration::from_millis(400),
        exchange: Duration::from_millis(400),
    };

    /// A pipe name no other test or engine uses.
    pub(in crate::sign_in) fn name(test: &str) -> String {
        format!(
            r"\\.\pipe\analyte-app-test-{test}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        )
        .replace([' ', ':', '{', '}', ','], "")
    }

    /// The one instance of a fake engine, as the engine makes its own.
    pub(in crate::sign_in) fn fake(name: &str) -> NamedPipeServer {
        ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(1)
            .create(name)
            .expect("a fake sign-in pipe")
    }

    /// Serves one caller: reads its request line, then writes `reply`, and
    /// gives back what the caller sent.
    pub(in crate::sign_in) async fn serve(server: NamedPipeServer, reply: Vec<u8>) -> Vec<u8> {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let mut server = server;
        server.connect().await.expect("a caller");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while server.read(&mut byte).await.expect("the request") == 1 {
            request.extend_from_slice(&byte);
            if byte == *b"\n" {
                break;
            }
        }
        server.write_all(&reply).await.expect("the answer is sent");
        // As the engine does: wait for the caller to read and close.
        let mut rest = [0u8; 16];
        while matches!(server.read(&mut rest).await, Ok(read) if read > 0) {}
        request
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{QUICK, fake, name, serve};
    use super::*;
    use crate::sign_in::line::Refusal;

    // ADR 0021 §2: the app sends one request line with its page and reads the
    // engine's code.
    #[tokio::test]
    async fn a_code_comes_back_for_the_page_asked() {
        let name = name("code");
        let server = tokio::spawn(serve(
            fake(&name),
            b"{\"code\":\"abc+/=\",\"account\":\"LAB-PC\\\\maria\"}\n".to_vec(),
        ));
        let answer = ask(&name, "/parked/12", QUICK).await.expect("an answer");
        assert!(matches!(answer, Answer::Code(code) if code.text() == "abc+/="));
        let request = server.await.expect("the fake engine");
        assert_eq!(request, b"{\"v\":1,\"next\":\"/parked/12\"}\n");
    }

    // ADR 0021 §2: a refusal comes back as its word.
    #[tokio::test]
    async fn a_refusal_comes_back_as_its_word() {
        let name = name("refusal");
        let server = tokio::spawn(serve(
            fake(&name),
            b"{\"refused\":\"not-allowed\",\"account\":\"LAB-PC\\\\bob\"}\n".to_vec(),
        ));
        let answer = ask(&name, "/", QUICK).await.expect("an answer");
        assert!(matches!(answer, Answer::Refused(Refusal::NotAllowed)));
        server.await.expect("the fake engine");
    }

    // ADR 0021 §2 and the design study's app tests: while the one instance
    // serves someone else the app tries again, and gives up after its limit.
    #[tokio::test]
    async fn a_busy_pipe_is_tried_again_until_its_limit() {
        let name = name("busy");
        let server = fake(&name);
        // Another caller holds the one instance.
        let holder = ClientOptions::new().open(&name).expect("the first caller");
        let started = std::time::Instant::now();
        let busy = ask(&name, "/", QUICK).await;
        assert!(matches!(busy, Err(AskError::Busy(_))), "{busy:?}");
        assert!(started.elapsed() >= QUICK.connect - RETRY, "gave up early");
        drop((holder, server));

        // Freed during the wait, the instance answers.
        let name = self::name("freed");
        let server = fake(&name);
        let holder = ClientOptions::new().open(&name).expect("the first caller");
        server
            .connect()
            .await
            .expect("the first caller is connected");
        let engine = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            drop(holder);
            server.disconnect().expect("the first caller is dropped");
            serve(server, b"{\"refused\":\"unavailable\"}\n".to_vec()).await
        });
        let answer = ask(&name, "/", QUICK).await.expect("an answer once free");
        assert!(matches!(answer, Answer::Refused(Refusal::Unavailable)));
        engine.await.expect("the fake engine");
    }

    // ADR 0021 §2: an engine that never answers is given up after its limit.
    #[tokio::test]
    async fn a_silent_engine_is_given_up_after_its_limit() {
        let name = name("silent");
        let server = fake(&name);
        let accepted = tokio::spawn(async move {
            server.connect().await.expect("a caller");
            tokio::time::sleep(QUICK.exchange * 3).await;
            drop(server);
        });
        let started = std::time::Instant::now();
        let silent = ask(&name, "/", QUICK).await;
        assert!(matches!(silent, Err(AskError::Silent(_))), "{silent:?}");
        assert!(started.elapsed() < QUICK.exchange * 2, "waited too long");
        accepted.await.expect("the fake engine");
    }

    // ADR 0021 §2: an answer past 1 KiB is refused without reading on.
    #[tokio::test]
    async fn garbage_past_one_kib_is_refused() {
        let name = name("garbage");
        let server = tokio::spawn(serve(fake(&name), vec![b'x'; LIMIT * 4]));
        let garbage = ask(&name, "/", QUICK).await;
        assert!(
            matches!(garbage, Err(AskError::Answer(AnswerError::TooLong))),
            "{garbage:?}"
        );
        drop(server);
    }

    // ADR 0021 §1: no pipe is not a refusal; the app says it is absent.
    #[tokio::test]
    async fn no_pipe_is_absent() {
        let absent = ask(&name("absent"), "/", QUICK).await;
        assert!(matches!(absent, Err(AskError::Absent)), "{absent:?}");
    }

    // ADR 0021 §2: an engine that closes without a whole line gave no answer.
    #[tokio::test]
    async fn a_closed_pipe_gave_no_answer() {
        let name = name("closed");
        let mut server = fake(&name);
        let engine = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt as _;
            server.connect().await.expect("a caller");
            server
                .write_all(b"{\"refused\"")
                .await
                .expect("half a line");
            server.disconnect().expect("the caller is dropped");
        });
        let closed = ask(&name, "/", QUICK).await;
        assert!(matches!(closed, Err(AskError::Closed)), "{closed:?}");
        engine.await.expect("the fake engine");
    }
}
