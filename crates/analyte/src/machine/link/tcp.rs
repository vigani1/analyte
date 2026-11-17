//! Owns TCP server and client session loops and keeps listener retries bounded.
use super::{Kind, Next, backoff, capture, recorded_connection, retry, session_ended};
use crate::machine::{Closing, Error, Machine, drive, settings::StreamSession};
use analyte_store::Handle;
use analyte_transport::Limits;
use std::{future::Future, io};
use tokio::sync::watch;

/// Accepts one session at a time; a failed session never ends the listener.
#[expect(
    clippy::too_many_arguments,
    reason = "the adopted listener is the only input beyond the shared session-loop inputs"
)]
pub(in crate::machine) async fn server(
    listener: Option<std::net::TcpListener>,
    machine: &Machine,
    store: &Handle,
    session: StreamSession<'_>,
    limits: Limits,
    mut stop: watch::Receiver<bool>,
    mut closing: Closing,
    capture: Option<&capture::Capture>,
) -> Result<(), Error> {
    // A listener that cannot be adopted or bound is unusable: no session can start.
    let listener = match listener {
        Some(listener) => tokio::net::TcpListener::from_std(listener)?,
        None => {
            analyte_transport::tcp::bind(machine.snapshot.effective().site().endpoint.parse()?)
                .await?
        }
    };
    loop {
        let stream = match accept(|| listener.accept(), machine.id.as_str(), &mut stop).await {
            Ok(Some((stream, _))) => stream,
            Ok(None) => {
                closing.after_commits().await?;
                return Ok(());
            }
            // accept retries every recoverable fault; only an unusable listening socket returns.
            Err(error) => return Err(error.into()),
        };
        let (conn, side) = recorded_connection(limits, capture);
        let cancel = conn.cancellation();
        let processing = drive(machine, session, store, conn, stop.clone(), &mut closing);
        let pumped = analyte_transport::tcp::pump(stream, side);
        // Drain ordered received/closed events even if the I/O owner finishes first.
        let (result, pumped) = tokio::join!(processing, pumped);
        cancel.cancel();
        match session_ended(
            machine.id.as_str(),
            Kind::TcpServer,
            result,
            pumped,
            &mut stop,
        )
        .await
        {
            Next::Serve => {}
            Next::Stop => return Ok(()),
            Next::StopIdle => {
                closing.after_commits().await?;
                return Ok(());
            }
        }
    }
}

/// Dials one session at a time and redials after every session, failed or clean.
pub(in crate::machine) async fn client(
    machine: &Machine,
    store: &Handle,
    session: StreamSession<'_>,
    limits: Limits,
    mut stop: watch::Receiver<bool>,
    mut closing: Closing,
    capture: Option<&capture::Capture>,
) -> Result<(), Error> {
    // An endpoint that is not a socket address can never be dialled.
    let endpoint = machine.snapshot.effective().site().endpoint.parse()?;
    // Fixed bounds; invalid only if the constants are wrong, so no session can start.
    let retry = retry()?;
    loop {
        let (conn, side) = recorded_connection(limits, capture);
        let cancel = conn.cancellation();
        let dialled = analyte_transport::tcp::client_session(endpoint, side, retry.clone());
        let (result, dialled) = tokio::join!(
            drive(machine, session, store, conn, stop.clone(), &mut closing),
            dialled
        );
        // drive cancels the I/O owner and drops its consumer after session cleanup.
        // The client's final Closed event can therefore meet a closed queue; that
        // expected owner shutdown is a clean end, not a transport failure.
        let dialled = if matches!(dialled, Err(analyte_transport::Error::ConsumerClosed))
            && cancel.is_cancelled()
        {
            Ok(())
        } else {
            dialled
        };
        let clean = result.is_ok() && dialled.is_ok();
        match session_ended(
            machine.id.as_str(),
            Kind::TcpClient,
            result,
            dialled,
            &mut stop,
        )
        .await
        {
            Next::Serve => {}
            Next::Stop | Next::StopIdle => return Ok(()),
        }
        // A peer that accepts and closes at once must not make the owner spin.
        if clean && !backoff(&mut stop).await {
            return Ok(());
        }
    }
}

// The callback is a test seam for OS accept faults that cannot be induced portably.
pub(in crate::machine) async fn accept<T, F, A>(
    mut next: F,
    machine: &str,
    stop: &mut watch::Receiver<bool>,
) -> io::Result<Option<T>>
where
    F: FnMut() -> A,
    A: Future<Output = io::Result<T>>,
{
    loop {
        let accepted = tokio::select! {
            biased;
            _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(None),
            accepted = next() => accepted,
        };
        match accepted {
            Ok(connection) => return Ok(Some(connection)),
            // InvalidInput denotes an unusable listening socket (e.g. EINVAL).
            // Other faults, including resource exhaustion, can recover on this listener.
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => return Err(error),
            Err(error) => {
                tracing::error!(machine, error_kind = ?error.kind(),
                    os_error = error.raw_os_error(), "TCP accept failed; retrying");
                if !backoff(stop).await {
                    return Ok(None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    #[tokio::test]
    async fn accept_fault_then_later_connection_is_served() -> Result<(), Box<dyn std::error::Error>>
    {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (_sender, mut stop) = watch::channel(false);
        let mut first = true;
        let server = async {
            let (mut socket, _) = accept(
                || {
                    let fault = std::mem::take(&mut first);
                    let listener = &listener;
                    async move {
                        if fault {
                            Err(io::ErrorKind::ConnectionAborted.into())
                        } else {
                            listener.accept().await
                        }
                    }
                },
                "synthetic",
                &mut stop,
            )
            .await?
            .ok_or("stopped")?;
            let mut byte = [0];
            socket.read_exact(&mut byte).await?;
            assert_eq!(byte, [42]);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let peer = async {
            let mut socket = TcpStream::connect(address).await?;
            socket.write_all(&[42]).await
        };
        let (result, sent) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
            tokio::join!(server, peer)
        })
        .await?;
        result?;
        sent?;
        Ok(())
    }
    #[tokio::test]
    async fn early_peer_reset_then_later_connection_is_served() -> Result<(), crate::Error> {
        let temp = crate::admin::private_tempdir()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let path = analyte_fixtures::write_machine(
            temp.path(),
            include_str!("../../../../core/config/examples/hl7.toml"),
            &address.to_string(),
        )?;
        let mut program = crate::installation::config::Program::load(&path)?;
        let store = std::sync::Arc::new(
            crate::runtime::service::open_store(
                &program,
                Box::new(analyte_fixtures::UnlimitedSpace),
            )
            .await?,
        );
        let mut machine = program.machines.pop().ok_or("machine")?;
        machine.listener = Some(listener);
        let (stop, stopped) = watch::channel(false);
        let (ready, _ready) = watch::channel(false);
        let (_close, close) = watch::channel(true);
        let owner = crate::machine::run(
            machine,
            store.clone(),
            stopped,
            temp.path().into(),
            Closing { ready, close },
            None,
        );
        let peers = async {
            let socket = TcpStream::connect(address).await?;
            #[expect(
                deprecated,
                reason = "zero linger injects an immediate reset without a blocking linger wait"
            )]
            socket.set_linger(Some(std::time::Duration::ZERO))?;
            drop(socket);
            let mut socket = TcpStream::connect(address).await?;
            socket
                .write_all(
                    b"\x0bMSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|later|P|2.5.1\rPID|1\r\x1c\r",
                )
                .await?;
            let mut reply = [0];
            socket.read_exact(&mut reply).await?;
            assert_eq!(reply, [0x0b], "later peer receives an MLLP acknowledgment");
            stop.send_replace(true);
            Ok::<_, crate::Error>(())
        };
        let (result, peers) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
            tokio::join!(owner, peers)
        })
        .await?;
        result?;
        peers?;
        let mut store = std::sync::Arc::try_unwrap(store).map_err(|_| "store still owned")?;
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }
}
