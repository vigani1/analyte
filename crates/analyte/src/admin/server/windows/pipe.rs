//! Windows-only: the sign-in pipe the engine holds for its whole life, answering one caller at a time within about 3 s each (ADR 0021 §1–§4).
use super::{
    AccountName, Sid, Status, WindowsAccount,
    codes::Codes,
    line::{self, Answer, Code, Next, Refusal, Request},
    rule::{self, Decision, Group, Membership, Rule},
};
use crate::runtime::service::caller::{self, Dacl};
use std::{io, thread::JoinHandle, time::Duration};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::windows::named_pipe::NamedPipeServer,
    sync::watch,
    time::Instant,
};

use super::{Found, GROUP};
/// How long a caller has to send its request line.
const READ_BUDGET: Duration = Duration::from_secs(2);
/// How long the engine has, once the request is in, to decide, write the
/// answer and see the caller close: 3 s per connection in all.
const ANSWER_BUDGET: Duration = Duration::from_secs(1);
/// The part of [`ANSWER_BUDGET`] for reading the token and issuing a code.
const DECIDE_BUDGET: Duration = Duration::from_millis(750);
/// The part of [`DECIDE_BUDGET`] the account name lookup may take; without a
/// name the caller is shown by SID, verified and never invented.
const NAME_WAIT: Duration = Duration::from_millis(500);
/// Identification threads that may still be finishing a name lookup; past
/// this the engine answers "unavailable" until they end.
const THREADS: usize = 4;
/// The pause after a failed accept, and how many in a row end the pipe.
const RETRY: Duration = Duration::from_millis(100);
const FAILURES: u32 = 16;

/// A verified Windows account, from its token and never from the caller's word.
pub(in crate::admin) struct Account {
    /// The user's SID.
    pub(in crate::admin) sid: Sid,
    /// `DOMAIN\name`, when Windows found it in time.
    pub(in crate::admin) name: Option<AccountName>,
}

/// Gives an allowed account a one-time code for `next`: the sign-in page's
/// [`Codes`] in the engine, a recorder in tests.
pub(in crate::admin) trait Issue {
    /// A code for `account`, or why it gets none.
    /// # Errors
    /// The [`Refusal`] the caller is sent instead.
    fn issue(&self, account: &Account, next: Next) -> Result<Code, Refusal>;
}

/// The sign-in page's codes: an allowed account gets one for 20 s.
impl Issue for Codes {
    fn issue(&self, account: &Account, next: Next) -> Result<Code, Refusal> {
        let kept = WindowsAccount::new(account.sid.clone(), account.name.clone()).map_err(|error| {
            tracing::warn!(sid = account.sid.as_str(), error = %error, "windows sign-in refused");
            Refusal::Unidentified
        })?;
        self.keep(kept, next).map_err(|error| {
            tracing::warn!(error = %error, "windows sign-in gave no code");
            Refusal::Unavailable
        })
    }
}

/// Whether the engine serves Windows sign-in.
pub(in crate::admin) enum Start {
    /// The pipe is held and ready to serve.
    On(Pipe),
    /// Off for this run; the admin key still works.
    Off(Off),
}

impl Start {
    /// What the pages say about Windows sign-in after this start.
    pub(in crate::admin) fn status(&self) -> Status {
        match self {
            Self::On(pipe) if pipe.rule.has_users_group() => Status::On,
            Self::On(_) => Status::AdministratorsOnly,
            Self::Off(reason) => Status::Off(reason.to_string()),
        }
    }
}

/// Why Windows sign-in is off for this run.
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum Off {
    /// Another program held the pipe's name when the engine started.
    #[error("another program already holds the sign-in pipe {name}")]
    Taken {
        /// The pipe's name.
        name: String,
        /// What Windows said.
        #[source]
        source: io::Error,
    },
    /// The pipe could not be made.
    #[error("the sign-in pipe {name} could not be created")]
    Failed {
        /// The pipe's name.
        name: String,
        /// Why.
        #[source]
        source: caller::Error,
    },
}

/// Takes the sign-in pipe for the admin pages on `port`, with the production
/// access list and the "Analyte users" group as found now.
pub(in crate::admin) fn start(port: u16) -> Start {
    open(
        format!(r"\\.\pipe\analyte-sign-in-{port}"),
        Rule::new(users_group()),
        Dacl::Production,
    )
}

fn open(name: String, rule: Rule, dacl: Dacl) -> Start {
    match caller::create_pipe(&name, dacl) {
        Ok(server) => Start::On(Pipe { server, rule }),
        Err(caller::Error::Taken(source)) => Start::Off(Off::Taken { name, source }),
        Err(source) => Start::Off(Off::Failed { name, source }),
    }
}

/// The SID of the local "Analyte users" group; none, with a warning, when
/// there is no such local group.
fn users_group() -> Option<Sid> {
    let only = "only Administrators may sign in with Windows";
    match super::users_group() {
        Found::Group(sid) => Some(sid),
        Found::NoComputerName => {
            tracing::warn!("COMPUTERNAME is not set; {only}");
            None
        }
        Found::NotAGroup => {
            tracing::warn!("the account {GROUP} is not a local group; {only}");
            None
        }
        Found::Missing => {
            tracing::warn!("the local group {GROUP} does not exist; {only}");
            None
        }
        Found::Failed(error) => {
            tracing::warn!(error = %error, "the local group {GROUP} could not be looked up; {only}");
            None
        }
    }
}

/// Why the pipe stopped serving before shutdown.
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum Stopped {
    /// Windows refused to drop a caller, so the instance cannot be reused.
    #[error("the sign-in pipe could not drop its caller")]
    Disconnect(#[source] io::Error),
    /// Accepting failed [`FAILURES`] times in a row.
    #[error("the sign-in pipe failed to accept callers {FAILURES} times in a row")]
    Accept(#[source] io::Error),
}

/// The engine's only instance of the sign-in pipe, held until it is dropped.
pub(in crate::admin) struct Pipe {
    server: NamedPipeServer,
    rule: Rule,
}

impl Pipe {
    /// Serves callers one at a time until `stop` turns true or its sender is
    /// dropped, then returns at once. The instance is disconnected and
    /// connected again per caller, so its name is never free while the engine
    /// runs. Cancelling or stopping leaves at most [`THREADS`] identification
    /// threads finishing a name lookup: they have already stopped
    /// impersonating, hold no caller data that reaches anyone, and end with
    /// the lookup or the process.
    /// # Errors
    /// [`Stopped`] when the instance can no longer serve.
    pub(in crate::admin) async fn serve(
        mut self,
        issuer: &impl Issue,
        mut stop: watch::Receiver<bool>,
    ) -> Result<(), Stopped> {
        let stopping = async move {
            let _ = stop.wait_for(|&stopped| stopped).await;
        };
        tokio::pin!(stopping);
        let mut threads = Vec::new();
        let mut failures = 0u32;
        loop {
            let connected = tokio::select! {
                () = &mut stopping => return Ok(()),
                connected = self.server.connect() => connected,
            };
            match connected {
                Ok(()) => {
                    failures = 0;
                    tokio::select! {
                        () = &mut stopping => return Ok(()),
                        () = self.one(issuer, &mut threads) => {}
                    }
                    self.server.disconnect().map_err(Stopped::Disconnect)?;
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    if failures >= FAILURES {
                        return Err(Stopped::Accept(error));
                    }
                    tracing::debug!(error = %error, "sign-in pipe accept failed");
                    // A caller that left before the accept leaves the instance
                    // to be reset; with nobody connected Windows refuses, and
                    // the next accept says whether the instance still works.
                    if let Err(error) = self.server.disconnect() {
                        tracing::debug!(error = %error, "sign-in pipe reset after a failed accept");
                    }
                    tokio::select! {
                        () = &mut stopping => return Ok(()),
                        () = tokio::time::sleep(RETRY) => {}
                    }
                }
            }
        }
    }

    /// Answers one connected caller within [`READ_BUDGET`] plus [`ANSWER_BUDGET`].
    async fn one(&mut self, issuer: &impl Issue, threads: &mut Vec<JoinHandle<()>>) {
        let line = match tokio::time::timeout(READ_BUDGET, read_line(&mut self.server)).await {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                tracing::debug!(error = %error, "sign-in caller left before asking");
                return;
            }
            // A silent caller gets nothing and is dropped.
            Err(_) => return,
        };
        let answering = Instant::now();
        let answer = match line::request(&line) {
            Ok(request) => {
                tokio::time::timeout(DECIDE_BUDGET, self.decide(request, issuer, threads))
                    .await
                    .unwrap_or(refused(Refusal::Unavailable, None))
            }
            Err(error) => {
                tracing::debug!(error = %error, "sign-in request refused");
                refused(Refusal::BadRequest, None)
            }
        };
        let reply = line::answer(&answer);
        let left = ANSWER_BUDGET.saturating_sub(answering.elapsed());
        let sent = tokio::time::timeout(left, async {
            self.server.write_all(&reply).await?;
            // Disconnecting discards an unread answer, so wait for the caller
            // to read it and close.
            let mut rest = [0u8; 64];
            while self.server.read(&mut rest).await? != 0 {}
            Ok::<_, io::Error>(())
        })
        .await;
        if let Ok(Err(error)) = sent {
            tracing::debug!(error = %error, "sign-in answer not delivered");
        }
    }

    /// Reads the caller from Windows, decides and, when allowed, asks `issuer`.
    async fn decide(
        &self,
        request: Request,
        issuer: &impl Issue,
        threads: &mut Vec<JoinHandle<()>>,
    ) -> Answer {
        for done in threads.extract_if(.., |thread| thread.is_finished()) {
            if done.join().is_err() {
                tracing::error!("a sign-in identification thread panicked");
            }
        }
        if threads.len() >= THREADS {
            tracing::warn!("sign-in identification threads are all busy");
            return refused(Refusal::Unavailable, None);
        }
        let identifying = match caller::identify(&self.server) {
            Ok(identifying) => identifying,
            Err(error) => {
                tracing::warn!(error = %error, "windows sign-in could not start reading the caller");
                return refused(Refusal::Unavailable, None);
            }
        };
        threads.push(identifying.thread);
        let identity = match identifying.identity.await {
            Ok(Ok(identity)) => identity,
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "windows sign-in could not read the caller");
                return refused(Refusal::Unidentified, None);
            }
            Err(_) => {
                tracing::error!("a sign-in identification thread ended without an answer");
                return refused(Refusal::Unidentified, None);
            }
        };
        let Some((sid, groups)) = typed(identity) else {
            tracing::warn!("windows gave the sign-in caller a SID that is not canonical");
            return refused(Refusal::Unidentified, None);
        };
        let name = match tokio::time::timeout(NAME_WAIT, identifying.name).await {
            Ok(Ok(Ok(text))) => AccountName::new(text).ok(),
            Ok(Ok(Err(error))) => {
                tracing::debug!(error = %error, "sign-in account name not found");
                None
            }
            Ok(Err(_)) | Err(_) => None,
        };
        let shown = name
            .as_ref()
            .map_or_else(|| sid.as_str(), AccountName::as_str)
            .to_owned();
        match rule::allowed(&groups, &self.rule) {
            Decision::NotAllowed => {
                tracing::info!(sid = sid.as_str(), "windows sign-in refused");
                refused(Refusal::NotAllowed, Some(shown))
            }
            Decision::Allowed => match issuer.issue(&Account { sid, name }, request.next) {
                Ok(code) => Answer::Code {
                    code,
                    account: shown,
                },
                Err(refusal) => refused(refusal, Some(shown)),
            },
        }
    }
}

fn refused(refused: Refusal, account: Option<String>) -> Answer {
    Answer::Refused { refused, account }
}

/// The token's facts as typed values; none when any SID is not canonical.
fn typed(identity: caller::Identity) -> Option<(Sid, Vec<Group>)> {
    let user = Sid::new(identity.user).ok()?;
    let groups = identity
        .groups
        .into_iter()
        .map(|(sid, attributes)| {
            Sid::new(sid).ok().map(|sid| Group {
                sid,
                membership: Membership::of(attributes),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((user, groups))
}

/// Reads up to the first newline, at most [`line::LIMIT`] + 1 bytes: a longer
/// or unterminated line is handed on whole for [`line::request`] to refuse.
async fn read_line(pipe: &mut NamedPipeServer) -> io::Result<Vec<u8>> {
    let mut buffer = [0u8; line::LIMIT + 1];
    let mut filled = 0;
    loop {
        let Some(free) = buffer.get_mut(filled..).filter(|free| !free.is_empty()) else {
            return Ok(buffer.to_vec());
        };
        let read = pipe.read(free).await?;
        let received = buffer
            .get(..filled.saturating_add(read))
            .unwrap_or_default();
        if read == 0 {
            return Ok(received.to_vec());
        }
        if let Some(end) = received.iter().skip(filled).position(|&b| b == b'\n') {
            let end = filled.saturating_add(end).saturating_add(1);
            return Ok(received.get(..end).unwrap_or_default().to_vec());
        }
        filled = received.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, ServerOptions};

    /// Everyone: enabled in every ordinary token.
    const EVERYONE: &str = "S-1-1-0";
    /// A group nobody is in.
    const NOBODY: &str = "S-1-5-21-0-0-0-1009";

    type Failure = Box<dyn std::error::Error + Send + Sync>;

    /// Records who was given a code and for which page.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String)>>);

    impl Issue for Recorder {
        fn issue(&self, account: &Account, next: Next) -> Result<Code, Refusal> {
            if let Ok(mut issued) = self.0.lock() {
                issued.push((account.sid.as_str().to_owned(), next.as_str().to_owned()));
            }
            assert!(
                account
                    .name
                    .as_ref()
                    .is_none_or(|n| n.as_str().contains('\\'))
            );
            Ok(Code::new("test-code".into()))
        }
    }

    fn sid(text: &str) -> Result<Sid, Failure> {
        Ok(Sid::new(text.to_owned())?)
    }

    /// A served test pipe: its name, issuer, stop switch and owner task.
    struct Served {
        name: String,
        issued: Arc<Recorder>,
        stop: watch::Sender<bool>,
        task: tokio::task::JoinHandle<Result<(), Stopped>>,
    }

    fn serve(rule: Rule) -> Result<Served, Failure> {
        let name = format!(r"\\.\pipe\analyte-sign-in-test-{}", uuid::Uuid::now_v7());
        let Start::On(pipe) = open(name.clone(), rule, Dacl::AlsoProcessAccount) else {
            return Err("the test pipe was not created".into());
        };
        let issued = Arc::new(Recorder::default());
        let (stop, stopped) = watch::channel(false);
        let recorder = issued.clone();
        let task = tokio::spawn(async move { pipe.serve(&*recorder, stopped).await });
        Ok(Served {
            name,
            issued,
            stop,
            task,
        })
    }

    /// Opens the pipe, waiting while the single instance is between callers.
    async fn connect(name: &str) -> Result<NamedPipeClient, Failure> {
        let until = Instant::now()
            .checked_add(Duration::from_secs(5))
            .ok_or("deadline overflow")?;
        loop {
            match ClientOptions::new().open(name) {
                Ok(client) => return Ok(client),
                Err(error)
                    if error.raw_os_error()
                        == i32::try_from(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY).ok()
                        && Instant::now() < until =>
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Sends `request` and reads the answer line, as the app does.
    async fn ask(name: &str, request: &[u8]) -> Result<serde_json::Value, Failure> {
        let mut client = connect(name).await?;
        client.write_all(request).await?;
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        tokio::time::timeout(Duration::from_secs(5), async {
            while client.read(&mut byte).await? == 1 {
                answer.extend_from_slice(&byte);
                if byte == *b"\n" {
                    break;
                }
            }
            Ok::<_, io::Error>(())
        })
        .await??;
        assert!(answer.ends_with(b"\n") && answer.len() <= line::LIMIT);
        Ok(serde_json::from_slice(&answer)?)
    }

    const REQUEST: &[u8] = b"{\"v\":1,\"next\":\"/parked/12\"}\n";

    // ADR 0021 §2: the caller is identified from its token as this process's
    // account; one instance serves callers one after another (tokio connects
    // again after disconnect on the same instance).
    #[tokio::test(flavor = "multi_thread")]
    async fn caller_is_identified_as_the_process_account() -> Result<(), Failure> {
        let served = serve(Rule::without_administrators(sid(EVERYONE)?))?;
        for _ in 0..3 {
            let answer = ask(&served.name, REQUEST).await?;
            assert_eq!(answer["code"], "test-code", "{answer}");
            assert!(answer["account"].as_str().is_some_and(|a| !a.is_empty()));
        }
        let me = caller::process_account()?;
        let issued = served.issued.0.lock().map_err(|_| "poisoned")?.clone();
        assert_eq!(issued, vec![(me, "/parked/12".to_owned()); 3]);
        Ok(())
    }

    // ADR 0021 §3: a caller outside the rule's groups gets no code.
    #[tokio::test(flavor = "multi_thread")]
    async fn caller_outside_the_rule_is_refused() -> Result<(), Failure> {
        let served = serve(Rule::without_administrators(sid(NOBODY)?))?;
        let answer = ask(&served.name, REQUEST).await?;
        assert_eq!(answer["refused"], "not-allowed", "{answer}");
        assert!(answer["account"].as_str().is_some_and(|a| !a.is_empty()));
        assert!(answer.get("code").is_none());
        assert!(served.issued.0.lock().map_err(|_| "poisoned")?.is_empty());
        Ok(())
    }

    // ADR 0021 §2: a malformed or oversized request is refused with a fixed
    // word, and the pipe keeps serving.
    #[tokio::test(flavor = "multi_thread")]
    async fn bad_requests_are_refused() -> Result<(), Failure> {
        let served = serve(Rule::without_administrators(sid(EVERYONE)?))?;
        let oversized = [b'a'; 2 * line::LIMIT];
        for request in [&b"{\"v\":1,\"next\":\"/\",\"as\":\"x\"}\n"[..], &oversized] {
            let answer = ask(&served.name, request).await?;
            assert_eq!(answer, serde_json::json!({"refused": "bad-request"}));
        }
        assert_eq!(ask(&served.name, REQUEST).await?["code"], "test-code");
        assert!(served.issued.0.lock().map_err(|_| "poisoned")?.len() == 1);
        Ok(())
    }

    // ADR 0021 §1: while the engine holds the pipe nobody can take its name or
    // add an instance, and a taken name turns Windows sign-in off.
    #[tokio::test(flavor = "multi_thread")]
    async fn held_pipe_cannot_be_taken_or_doubled() -> Result<(), Failure> {
        let served = serve(Rule::new(None))?;
        let again = open(served.name.clone(), Rule::new(None), Dacl::Production);
        assert!(
            matches!(&again, Start::Off(Off::Taken { name, .. }) if *name == served.name),
            "a second first instance was allowed"
        );
        assert!(ServerOptions::new().create(&served.name).is_err());
        Ok(())
    }

    // ADR 0021 §2: a caller that says nothing is dropped after 2 s, and the
    // next caller is served.
    #[tokio::test(flavor = "multi_thread")]
    async fn silent_caller_is_dropped_after_two_seconds() -> Result<(), Failure> {
        let served = serve(Rule::without_administrators(sid(EVERYONE)?))?;
        let mut silent = connect(&served.name).await?;
        let started = Instant::now();
        let mut byte = [0u8; 1];
        let ended = tokio::time::timeout(Duration::from_secs(4), silent.read(&mut byte)).await?;
        let waited = started.elapsed();
        assert!(matches!(ended, Ok(0) | Err(_)), "{ended:?}");
        assert!(
            waited >= Duration::from_millis(1900) && waited < Duration::from_millis(3500),
            "{waited:?}"
        );
        assert_eq!(ask(&served.name, REQUEST).await?["code"], "test-code");
        Ok(())
    }

    // ADR 0021 §1 and AGENTS.md (bounded shutdown): stop ends the loop at once,
    // even with a caller connected.
    #[tokio::test(flavor = "multi_thread")]
    async fn stop_ends_the_loop_at_once() -> Result<(), Failure> {
        let served = serve(Rule::new(None))?;
        let _silent = connect(&served.name).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        served.stop.send_replace(true);
        tokio::time::timeout(Duration::from_millis(500), served.task).await???;
        Ok(())
    }
}
