//! Keeps synthetic conversations alive across host-state waits and names unexpected closure.
use super::{Error, step};
use std::future::Future;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Runs complete peer exchanges while awaiting a durable host outcome.
/// The final exchange finishes before returning, so the next read starts at a boundary.
/// A failure or deadline can interrupt an exchange; abandon that socket on error.
/// # Errors
/// Returns the original outcome, exchange or bounded-wait failure.
pub async fn while_servicing<T>(
    outcome: impl Future<Output = Result<T, Error>>,
    mut exchange: impl AsyncFnMut() -> Result<(), Error>,
) -> Result<T, Error> {
    step("service peer until host outcome", async {
        tokio::pin!(outcome);
        loop {
            let exchange = exchange();
            tokio::pin!(exchange);
            tokio::select! {
                biased;
                result = &mut outcome => {
                    let value = result?;
                    exchange.await?;
                    return Ok::<_, Error>(value);
                },
                result = &mut exchange => result?,
            }
        }
    })
    .await
}

/// Requires a clean host close; an unexpected data byte is an assertion failure.
/// # Errors
/// Preserves read errors and names the close deadline.
/// # Panics
/// Fails if the host sends data instead of closing.
pub async fn closed(peer: &mut (impl AsyncRead + Unpin)) -> Result<(), Error> {
    assert_eq!(step("host closes peer", peer.read(&mut [0])).await?, 0);
    Ok(())
}

/// Reads required data, preserving EOF as an error with its conversation step.
/// # Errors
/// An EOF is never accepted as silence or successful receipt.
pub async fn byte(peer: &mut (impl AsyncRead + Unpin)) -> Result<u8, Error> {
    step(
        "required peer byte (host must remain connected)",
        peer.read_u8(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::HANG_GUARD;
    use tokio::io::AsyncWriteExt;

    #[tokio::test(start_paused = true)]
    async fn host_outcome_finishes_the_in_flight_exchange() -> Result<(), Error> {
        let (finish, finished) = tokio::sync::oneshot::channel();
        let mut finished = Some(finished);
        let service = while_servicing(async { Ok(42) }, async || {
            finished.take().ok_or("unexpected second exchange")?.await?;
            Ok(())
        });
        tokio::pin!(service);
        std::future::poll_fn(|cx| {
            assert!(service.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        finish.send(()).map_err(|()| "exchange cancelled")?;
        assert_eq!(tokio::time::timeout(HANG_GUARD, service).await??, 42);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn host_outcome_can_require_peer_progress() -> Result<(), Error> {
        let (sent, received) = tokio::sync::oneshot::channel();
        let mut sent = Some(sent);
        let value = while_servicing(async { Ok(received.await?) }, async || {
            if let Some(sent) = sent.take() {
                sent.send(42).map_err(|_| "outcome cancelled")?;
            }
            Ok(())
        })
        .await?;
        assert_eq!(value, 42);
        Ok(())
    }

    #[tokio::test]
    async fn eof_is_valid_only_when_close_is_expected() -> Result<(), Error> {
        let (mut host, mut peer) = tokio::io::duplex(16);
        host.write_all(b"x").await?;
        host.shutdown().await?;
        assert_eq!(byte(&mut peer).await?, b'x');
        let error = byte(&mut peer)
            .await
            .expect_err("required data cannot accept EOF");
        assert!(error.to_string().contains("required peer byte"));
        closed(&mut peer).await?;
        Ok(())
    }
}
