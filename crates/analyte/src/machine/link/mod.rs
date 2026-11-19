//! Owns TCP, serial and folder links, the shared failed-session rule and recordings.
pub(crate) mod capture;
pub(crate) mod error;
pub(crate) mod folder;
pub(crate) mod serial;
pub(crate) mod status;
pub(crate) mod tcp;

use analyte_transport::{Backoff, Limits, connection};
use std::{fmt::Display, time::Duration};
use tokio::sync::watch;

/// Caps reconnection churn while keeping cancellation responsive.
const RECONNECT_MIN: Duration = Duration::from_millis(100);

/// Caps exponential transport reconnection backoff.
const RECONNECT_MAX: Duration = Duration::from_secs(5);

/// The site's fixed network endpoint as a socket address. A USB serial adapter or a
/// spelling that is not a socket address is refused: no session can start on it.
pub(in crate::machine) fn socket_address(
    machine: &crate::installation::config::Machine,
) -> Result<std::net::SocketAddr, error::Error> {
    Ok(machine
        .snapshot
        .effective()
        .site()
        .endpoint
        .fixed()?
        .parse()?)
}

/// Names the session loop in diagnostics; never carries an endpoint or payload.
#[derive(Clone, Copy)]
pub(in crate::machine) enum Kind {
    TcpServer,
    TcpClient,
    Serial,
    LawInbound,
}

impl Kind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TcpServer => "tcp_server",
            Self::TcpClient => "tcp_client",
            Self::Serial => "serial",
            Self::LawInbound => "law_inbound",
        }
    }
}

/// What a transport owner does after one session ends.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::machine) enum Next {
    /// Serve the next session on the same port or listener.
    Serve,
    /// Stop was requested by the time the session ended; the session ran its own close stage.
    Stop,
    /// Stop arrived during the backoff; no session is open, so the owner runs the close stage.
    StopIdle,
}

/// The one failed-session rule for every session loop (review Y1).
///
/// A session or its per-session transport failing never makes the port or listener
/// unusable: both errors are logged with structured fields and no payload, the owner
/// waits the shared reconnect backoff and serves the next session. Only stop ends the
/// loop here; owner-level failures are decided by each loop and say why. Durable
/// commits already happened inside the session, so nothing is retried or replayed.
/// Cancelling the backoff leaves no state behind.
pub(in crate::machine) async fn session_ended<S: Display, T: Display>(
    machine: &str,
    link: Kind,
    session: Result<(), S>,
    transport: Result<(), T>,
    stop: &mut watch::Receiver<bool>,
) -> Next {
    let failed = report(machine, link, session, transport);
    if *stop.borrow() {
        return Next::Stop;
    }
    if failed && !backoff(stop).await {
        return Next::StopIdle;
    }
    Next::Serve
}

/// Logs a session's failures with structured fields and no payload; `true` if any failed.
pub(in crate::machine) fn report<S: Display, T: Display>(
    machine: &str,
    link: Kind,
    session: Result<(), S>,
    transport: Result<(), T>,
) -> bool {
    let link = link.as_str();
    let mut failed = false;
    if let Err(error) = transport {
        failed = true;
        tracing::error!(machine, link, error_kind = %error, "session transport stopped");
    }
    if let Err(error) = session {
        failed = true;
        tracing::error!(machine, link, error_kind = %error, "machine session stopped");
    }
    failed
}

/// Waits the shared reconnect backoff; `false` means stop arrived first.
pub(in crate::machine) async fn backoff(stop: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        biased;
        _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => false,
        () = tokio::time::sleep(RECONNECT_MIN) => true,
    }
}

/// Builds the transport's own reconnect policy from the shared bounds.
pub(in crate::machine) fn retry() -> Result<Backoff, analyte_transport::Error> {
    Backoff::new(RECONNECT_MIN, RECONNECT_MAX, 1)
}

/// Creates one connection, reports its openings and reads to the machine's status,
/// and attaches the optional recorder to its I/O side.
pub(in crate::machine) fn recorded_connection(
    limits: Limits,
    capture: Option<&capture::Capture>,
    status: &status::Reporter,
) -> (analyte_transport::Connection, analyte_transport::IoSide) {
    let (conn, side) = connection(limits);
    let side = side.with_observer(status.observer());
    let side = match capture {
        Some(capture) => capture.attach(side),
        None => side,
    };
    (conn, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Review Y1: a failure backs off once, a clean end does not, and stop always wins.
    // Review finding Y1
    #[tokio::test(start_paused = true)]
    async fn failed_session_backs_off_and_stop_wins() {
        let (stop, mut stopping) = watch::channel(false);
        let failed = || Err::<(), _>("synthetic session fault");
        let clean = || Ok::<(), &str>(());
        let started = tokio::time::Instant::now();
        let next = session_ended("m", Kind::Serial, failed(), clean(), &mut stopping).await;
        assert_eq!(next, Next::Serve);
        assert_eq!(started.elapsed(), RECONNECT_MIN);
        let started = tokio::time::Instant::now();
        let next = session_ended("m", Kind::TcpServer, clean(), clean(), &mut stopping).await;
        assert_eq!(next, Next::Serve);
        assert_eq!(started.elapsed(), Duration::ZERO);
        let (next, ()) = tokio::join!(
            session_ended("m", Kind::TcpClient, clean(), failed(), &mut stopping),
            async {
                stop.send_replace(true);
            }
        );
        assert_eq!(next, Next::StopIdle);
        let next = session_ended("m", Kind::LawInbound, failed(), clean(), &mut stopping).await;
        assert_eq!(next, Next::Stop);
    }
}
