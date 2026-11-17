//! Tests protected wire capture and delivery behavior.
use super::*;
use crate::support::HANG_GUARD;
use analyte_fixtures::ConfigEdit as _;
use analyte_transport::recording::{self, Direction, Evidence};
use tokio::io::AsyncWriteExt as _;

#[tokio::test]
async fn plan10_adr0009_capture_preserves_tcp_bytes_and_failure_does_not_change_delivery()
-> Result<(), Error> {
    for (client, enabled, fail) in [
        (false, true, false),
        (true, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        let temp = tempfile::tempdir()?;
        let path = config(temp.path(), false)?;
        let text = std::fs::read_to_string(&*path)?
            .rewrite("id = 'hl7'", &format!("id = 'hl7'\ncapture = {enabled}"));
        std::fs::write(&*path, text)?;
        let listener = capture_listener(temp.path(), &path, client).await?;
        let mut reference = Reference::start(
            &temp.path().join("receiver.db"),
            TcpListener::bind("127.0.0.1:0").await?,
            false,
        )
        .await?;
        let mut program = if client {
            Program::load(&path)?
        } else {
            program(&path, &reference.url)?
        };
        program.set_loopback_destination_for_test("demo", reference.url.clone())?;
        if fail {
            let directory = temp.path().join("data");
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
            }
            #[cfg(windows)]
            std::fs::create_dir(&directory)?;
            std::fs::write(
                directory.join("captures"),
                b"deliberately block capture directory",
            )?;
        }
        let running = Running::start_program(program, "synthetic-api").await?;
        let mut peer = if let Some(listener) = &listener {
            tokio::time::timeout(HANG_GUARD, listener.accept())
                .await??
                .0
        } else {
            connect(&path, "hl7").await?
        };
        let ack = hl7(&mut peer, ORU).await?;
        assert!(ack.contains("MSA|AA|message1"));
        let delivered = reference.event().await?;
        assert!(!delivered.bytes.is_empty());
        peer.shutdown().await?;
        drop(peer);
        let mut acknowledgments = vec![ack];
        if enabled && !fail {
            let mut peer = if let Some(listener) = &listener {
                tokio::time::timeout(HANG_GUARD, listener.accept())
                    .await??
                    .0
            } else {
                connect(&path, "hl7").await?
            };
            let ack = hl7(&mut peer, ORU).await?;
            assert!(ack.contains("MSA|AA|message1"));
            acknowledgments.push(ack);
            peer.shutdown().await?;
            drop(peer);
        }
        running.stop().await?;
        let directory = temp.path().join("data/captures/hl7");
        if enabled && !fail {
            assert_captured_conversations(temp.path(), client, acknowledgments)?;
        } else if fail {
            let program = Program::load(&path)?;
            let mut store = analyte::open_system_store(&program).await?;
            let lines = analyte::operator::status::snapshot(&program, &store).await?;
            store.shutdown(analyte_deadline()).await?;
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("capture=") && line.contains("failed:"))
            );
        } else {
            assert!(!directory.exists());
        }
        reference.close().await?;
    }
    Ok(())
}

fn analyte_deadline() -> std::time::Instant {
    std::time::Instant::now()
        .checked_add(HANG_GUARD)
        .unwrap_or_else(std::time::Instant::now)
}

async fn capture_listener(
    root: &Path,
    path: &Path,
    client: bool,
) -> Result<Option<TcpListener>, Error> {
    let listener = if client {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let definition = root.join("hl7.toml");
        std::fs::write(
            &definition,
            std::fs::read_to_string(&definition)?.rewrite("tcp_server", "tcp_client"),
        )?;
        let site = root.join("hl7-site.toml");
        let previous = Program::load(path)?
            .machines()
            .first()
            .ok_or("machine")?
            .snapshot()
            .effective()
            .site()
            .endpoint
            .clone();
        std::fs::write(
            &site,
            std::fs::read_to_string(&site)?.rewrite(&previous, &listener.local_addr()?.to_string()),
        )?;
        Some(listener)
    } else {
        None
    };
    Ok(listener)
}

fn assert_captured_conversations(
    root: &Path,
    client: bool,
    acknowledgments: Vec<String>,
) -> Result<(), Error> {
    let directory = root.join("data/captures/hl7");
    let mut files = std::fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.sort();
    assert_eq!(files.len(), 2);
    for (file, ack) in files.into_iter().zip(acknowledgments) {
        assert!(file.starts_with(root.join("data")));
        let (header, events) =
            recording::read(&std::fs::read(&file)?[..], recording::Limits::default())?;
        assert_eq!(header.evidence, Evidence::Machine);
        assert_eq!(header.de_identification, "NOT de-identified");
        assert_eq!(
            header.stack,
            [
                if client { "tcp_client" } else { "tcp_server" },
                "mllp",
                "hl7_receiver",
                "hl7_v2"
            ]
        );
        let bytes = |direction| {
            events
                .iter()
                .filter(|e| e.link.is_none() && e.direction == direction)
                .flat_map(|e| e.bytes.iter().copied())
                .collect::<Vec<_>>()
        };
        assert_eq!(bytes(Direction::HostToMachine), ack.as_bytes());
        let wire = analyte_framing::hl7::encode(ORU, analyte_framing::hl7::Limit::new(1048576)?)?;
        assert_eq!(bytes(Direction::MachineToHost), wire);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(file)?.permissions().mode() & 0o077, 0);
        }
    }
    Ok(())
}
