//! Proves stop handling, parking and store-before-ACK on the real VIDAS and POCT hosts.
use super::super::{Flow, Protocol as _, Session, protocol::feed};
use crate::intake::{self, ByteReason};
use crate::machine::{poct, settings::StreamSession};
use analyte_fixtures::poct as peer;
use analyte_store::{Handle, OperatorError, Reading};
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::watch;

/// Every receipt in the store: its raw bytes and, when parked, its reason.
async fn journal(store: &Handle) -> Result<Vec<(Vec<u8>, Option<String>)>, crate::Error> {
    Ok(store
        .request::<_, OperatorError>(crate::deadline()?, |s| {
            let mut receipts = Vec::new();
            for metadata in s.receipts("")? {
                let raw = s
                    .receipt(metadata.id)?
                    .ok_or(OperatorError::Corruption)?
                    .raw;
                let reason = match s.interpretation(metadata.id, 1)?.map(|e| e.outcome) {
                    Some(Reading::Failed(reason)) => Some(reason.as_str().to_owned()),
                    Some(Reading::Results(_)) | None => None,
                };
                receipts.push((raw, reason));
            }
            Ok(receipts)
        })
        .await?)
}

/// A program with one synthetic machine of `definition` and its open store.
async fn program(
    temp: &std::path::Path,
    definition: &str,
) -> Result<(crate::installation::config::Program, Handle), crate::Error> {
    let path = analyte_fixtures::write_machine(temp, definition, "127.0.0.1:5600")?;
    let program = crate::installation::config::Program::load(&path)?;
    let store =
        crate::runtime::service::open_store(&program, Box::new(analyte_fixtures::UnlimitedSpace))
            .await?;
    Ok((program, store))
}

/// Risk 1 of the one-runner step: a chunk that reached the host before a stop
/// is journaled once before the dialog sees it (so before its ACK), and the stop
/// then reaches the dialog as a disconnect that parks the open transfer.
/// Source: AGENTS.md safety rules 1 and 2; ADR 0003; VIDAS Literal ENQ/ACK.
#[tokio::test(flavor = "current_thread")]
async fn adr0003_vidas_chunk_before_stop_is_journaled_then_the_stop_parks_the_transfer()
-> Result<(), crate::Error> {
    let temp = tempfile::tempdir()?;
    let (program, mut store) = program(
        temp.path(),
        include_str!("../../../../../core/config/examples/vidas_literal.toml"),
    )
    .await?;
    let machine = program.machines().first().ok_or("synthetic machine")?;
    let analyte_config::Stack::Vidas(settings) = machine.snapshot.effective().stack() else {
        return Err("VIDAS settings".into());
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut peer = tokio::net::TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut conn, side) =
        analyte_transport::connection(analyte_transport::Limits::new(4, Duration::from_secs(5))?);
    let cancel = conn.cancellation();
    let (signal, stop) = watch::channel(false);
    let session = async {
        let result = crate::machine::session(
            machine,
            StreamSession::Vidas(settings),
            &store,
            &mut conn,
            stop,
        )
        .await;
        cancel.cancel();
        result
    };
    let machine_side = async {
        peer.write_all(&[5]).await?;
        let mut ack = [0u8; 1];
        peer.read_exact(&mut ack).await?;
        signal.send(true)?;
        let mut rest = Vec::new();
        peer.read_to_end(&mut rest).await?;
        Ok::<_, crate::Error>((ack, rest))
    };
    let (result, pumped, replies) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
        tokio::join!(
            session,
            analyte_transport::tcp::pump(stream, side),
            machine_side
        )
    })
    .await?;
    pumped?;
    let (ack, rest) = replies?;
    assert_eq!(ack, [6], "ENQ is acknowledged");
    assert!(rest.is_empty(), "nothing is sent after the stop: {rest:?}");
    let error = result
        .err()
        .ok_or("a parked transfer ends the session with its cause")?;
    assert!(
        error.to_string().contains("VIDAS transfer parked"),
        "{error}"
    );
    let mut receipts = journal(&store).await?;
    receipts.sort();
    assert_eq!(
        receipts,
        [
            (Vec::new(), Some(ByteReason::VidasIncomplete.to_string())),
            (vec![5], None),
        ],
        "the ENQ chunk is journaled once and the stop parks the open transfer"
    );
    store.shutdown(crate::deadline()?).await?;
    Ok(())
}

/// Feeds one transport chunk through the POCT host's byte loop.
async fn feed_poct(
    host: &mut poct::Host,
    bytes: &[u8],
    session: &mut Session<'_, '_, analyte_config::PoctStack>,
) -> Result<Flow, poct::Error> {
    feed(host, session, bytes.to_vec()).await
}

/// Ends a POCT host the way the runner does after a stop.
async fn close_poct(
    host: &mut poct::Host,
    session: &mut Session<'_, '_, analyte_config::PoctStack>,
) -> Result<(), poct::Error> {
    let mut result = Ok(());
    host.close(session, &mut result).await?;
    result
}

/// Risks 2 and 3 of the one-runner step: a stop seen between two bytes of one
/// chunk parks exactly the dialog's partial input with the cancelled reason,
/// stops before the next byte, and never stores or acknowledges a message whose
/// last byte arrived with the stop. The connection never opened, so any write
/// attempt would fail the feed instead of returning `Stop`.
/// Source: AGENTS.md safety rules 1 and 2; ADR 0003; POCT01-A2 dialog custody.
#[tokio::test]
async fn adr0003_poct_stop_mid_chunk_parks_partial_input_and_never_acknowledges()
-> Result<(), crate::Error> {
    let hello = peer::hello("hello").into_bytes();
    let middle = hello.len() / 2;
    let last = hello.len() - 1;
    // (bytes fed before the stop, chunk fed after it, expected parked bytes)
    let cases = [
        (
            hello.get(..middle).ok_or("split")?.to_vec(),
            hello.get(middle..).ok_or("split")?.to_vec(),
            hello.get(..=middle).ok_or("split")?.to_vec(),
        ),
        (
            hello.get(..last).ok_or("split")?.to_vec(),
            [hello.get(last..).ok_or("split")?, b"<DST.R01>"].concat(),
            hello.clone(),
        ),
    ];
    for (before, after, parked) in cases {
        let temp = tempfile::tempdir()?;
        let (program, mut store) = program(
            temp.path(),
            include_str!("../../../../../core/config/examples/poct.toml"),
        )
        .await?;
        let machine = program.machines().first().ok_or("synthetic machine")?;
        let connection = intake::ConnectionEvidence::new(machine)?;
        let analyte_config::Stack::Poct(settings) = machine.snapshot.effective().stack() else {
            return Err("POCT settings".into());
        };
        let (mut conn, _io) = analyte_transport::connection(analyte_transport::Limits::new(
            4,
            Duration::from_secs(1),
        )?);
        let (signal, stop) = watch::channel(false);
        let mut host = poct::Host::open(settings)?;
        let mut session = Session::new(&connection, settings.as_ref(), &store, &mut conn, stop);
        let flow = feed_poct(&mut host, &before, &mut session).await?;
        assert_eq!(flow, Flow::Continue);
        assert!(
            journal(&store).await?.is_empty(),
            "a partial message is not parked"
        );
        signal.send(true)?;
        let flow = feed_poct(&mut host, &after, &mut session).await?;
        assert_eq!(flow, Flow::Stop, "the stop ends the chunk without a write");
        close_poct(&mut host, &mut session).await?;
        assert_eq!(
            journal(&store).await?,
            [(parked, Some(ByteReason::PoctCancelled.to_string()))],
            "only the fed input is parked, once, and nothing is stored as accepted"
        );
        store.shutdown(crate::deadline()?).await?;
    }
    Ok(())
}
