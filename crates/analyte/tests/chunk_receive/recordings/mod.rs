//! Generates synthetic protocol conversations and replays them through the program.
use super::*;
use analyte_transport::recording::{self, Direction, Event, Evidence, Exception, Header, Link};

mod scenarios;
use scenarios::{Case, Protocol, Step};

fn directory() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testing/sim/recordings")
}

fn path(protocol: Protocol, case: Case) -> std::path::PathBuf {
    directory().join(format!("{}-{}.rec.jsonl", protocol.key(), case.key()))
}

fn event(
    direction: Direction,
    bytes: Vec<u8>,
    link: Option<Link>,
    index: usize,
) -> Result<Event, Error> {
    Ok(Event {
        direction,
        time_ms: u64::try_from(index)?
            .checked_mul(10)
            .ok_or("event time overflow")?,
        bytes,
        link,
    })
}

fn exceptions(events: &[Event], protocol: Protocol) -> Result<Vec<Exception>, Error> {
    let mut result = Vec::new();
    for (event, item) in events.iter().enumerate() {
        if item.direction != Direction::HostToMachine || item.link.is_some() {
            continue;
        }
        let text = std::str::from_utf8(&item.bytes)?;
        if protocol == Protocol::Poct {
            for field in ["HDR.control_id", "HDR.creation_dttm"] {
                let prefix = format!("<{field} V=\"");
                let start = text
                    .find(&prefix)
                    .ok_or("missing POCT host header")?
                    .checked_add(prefix.len())
                    .ok_or("header offset overflow")?;
                let end = start
                    .checked_add(
                        text.get(start..)
                            .ok_or("header suffix")?
                            .find('"')
                            .ok_or("header attribute end")?,
                    )
                    .ok_or("attribute offset overflow")?;
                result.push(Exception {
                    name: format!("generated {field}"),
                    event,
                    bytes: start..end,
                });
            }
        } else if protocol == Protocol::Law {
            let mut offset: usize = 0;
            for (field, value) in text.split('|').take(10).enumerate() {
                if field == 6 || field == 9 {
                    let end = offset
                        .checked_add(value.len())
                        .ok_or("MSH offset overflow")?;
                    result.push(Exception {
                        name: format!(
                            "generated MSH-{}",
                            field.checked_add(1).ok_or("field overflow")?
                        ),
                        event,
                        bytes: offset..end,
                    });
                }
                offset = offset
                    .checked_add(value.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or("MSH offset overflow")?;
            }
        }
    }
    Ok(result)
}

async fn host(peer: &mut TcpStream, step: &Step) -> Result<Vec<u8>, Error> {
    match step {
        Step::Host(expected) => {
            let mut bytes = vec![0; expected.len()];
            tokio::time::timeout(HANG_GUARD, peer.read_exact(&mut bytes)).await??;
            assert_eq!(&bytes, expected);
            Ok(bytes)
        }
        Step::Xml | Step::Escape | Step::End => {
            let bytes = xml(peer).await?;
            let text = std::str::from_utf8(&bytes)?;
            match step {
                Step::Escape => assert!(
                    matches!(analyte_poct::Message::parse(&bytes, poct_peer::limits())?.body(), analyte_poct::Body::Escape { reference, .. } if reference.as_str() == "observation")
                ),
                Step::End => assert!(matches!(
                    analyte_poct::Message::parse(&bytes, poct_peer::limits())?.body(),
                    analyte_poct::Body::Terminate(_)
                )),
                Step::Xml => assert!(
                    text.contains("ACK.type_cd V=\"AA\"") || text.starts_with("<REQ.R01>"),
                    "{text}"
                ),
                Step::Machine(_) | Step::Host(_) | Step::Mllp | Step::Disconnect => {
                    return Err("unexpected XML step".into());
                }
            }
            Ok(bytes)
        }
        Step::Mllp => {
            let mut bytes = vec![byte(peer).await?];
            assert_eq!(bytes.first(), Some(&11));
            while !bytes.ends_with(&[28, 13]) {
                assert!(bytes.len() < 65536);
                bytes.push(byte(peer).await?);
            }
            let text = std::str::from_utf8(&bytes)?;
            assert!(text.contains("MSA|AA|recording"), "{text}");
            Ok(bytes)
        }
        Step::Machine(_) | Step::Disconnect => Err("not a host step".into()),
    }
}

async fn eof(peer: &mut TcpStream, deadline: Duration) -> Result<(), Error> {
    let mut bytes = [0];
    assert_eq!(
        tokio::time::timeout(deadline, peer.read(&mut bytes)).await??,
        0,
        "closed lost-ACK session must not emit another command or acceptance"
    );
    Ok(())
}

async fn check(run: Running, protocol: Protocol, case: Case) -> Result<(), Error> {
    if protocol == Protocol::Autolyser && case == Case::LostAck {
        return run.close().await;
    }
    if case == Case::Disconnect {
        let reason = match protocol {
            Protocol::Law | Protocol::Autolyser => "incomplete",
            Protocol::Poct => "POCT dialog closure",
            Protocol::Vidas => "VIDAS transfer incomplete",
        };
        run.parked(reason).await?;
        let prefix = scenarios::steps(protocol, case)?
            .into_iter()
            .rev()
            .find_map(|step| match step {
                Step::Machine(bytes) => Some(bytes),
                Step::Host(_)
                | Step::Xml
                | Step::Mllp
                | Step::Escape
                | Step::End
                | Step::Disconnect => None,
            })
            .ok_or("disconnect prefix missing")?;
        assert!(
            run.database()?.query_row(
                "SELECT count(*) FROM receipts WHERE raw=?1",
                [&prefix],
                |row| row.get::<_, i64>(0)
            )? > 0,
            "incomplete result bytes must remain in protected custody"
        );
        return run.close().await;
    }
    let (raw, driver, reason, value) = match protocol {
        Protocol::Law => {
            // Wait for the same single delivery after an ACK-loss retransmission.
            poct_deliveries(&run, 1).await?;
            return run.close_with_deliveries(1).await;
        }
        Protocol::Poct => (
            scenarios::observation(),
            "analyte-poct",
            "POCT service sequence may wrap",
            "12.30",
        ),
        Protocol::Vidas => (
            scenarios::vidas()?,
            "analyte-vidas",
            "VIDAS Literal lacks trusted result/run identity",
            "<0.1",
        ),
        Protocol::Autolyser => (
            report()?,
            "analyte-autolyser",
            "Autolyser Rev18 has no immutable run identity",
            "<0.1   ",
        ),
    };
    run.parked(reason).await?;
    run.stored(&raw, driver)?;
    run.result_value(value)?;
    run.close().await
}

// ADR 0009; sources and deliberate faults are also named in every generated header.
#[tokio::test]
#[ignore = "explicit fixture authoring: writes synthetic recordings into the repository"]
async fn generate_protocol_recordings() -> Result<(), Error> {
    for protocol in Protocol::ALL {
        for case in Case::ALL {
            eprintln!("{} {}", protocol.key(), case.key());
            let mut run = Running::start(protocol.definition()).await?;
            let program = Program::load(&run.path)?;
            let stack = program
                .machines()
                .first()
                .ok_or("machine missing")?
                .snapshot()
                .effective()
                .stack();
            let deadline = Duration::from_millis(u64::from(stack.operation_timeout_ms()));
            let mut events = vec![event(
                Direction::MachineToHost,
                Vec::new(),
                Some(Link::Connect),
                0,
            )?];
            for step in scenarios::steps(protocol, case)? {
                let (direction, bytes, link) = match &step {
                    Step::Machine(bytes) => {
                        run.peer.write_all(bytes).await?;
                        (Direction::MachineToHost, bytes.clone(), None)
                    }
                    Step::Disconnect => {
                        run.peer.shutdown().await?;
                        (Direction::MachineToHost, Vec::new(), Some(Link::Disconnect))
                    }
                    Step::Host(_) | Step::Xml | Step::Mllp | Step::Escape | Step::End => (
                        Direction::HostToMachine,
                        host(&mut run.peer, &step).await?,
                        None,
                    ),
                };
                events.push(event(direction, bytes, link, events.len())?);
            }
            if (protocol == Protocol::Autolyser || protocol == Protocol::Vidas)
                && case == Case::LostAck
            {
                eof(&mut run.peer, deadline).await?;
            }
            let snapshot = Program::load(&run.path)?;
            let stack = snapshot
                .machines()
                .first()
                .ok_or("machine missing")?
                .snapshot()
                .effective()
                .stack()
                .layer_names()
                .map(str::to_owned);
            let header = Header {
                version: 1,
                stack,
                machine: Some("synthetic".into()),
                software: None,
                evidence: Evidence::Synthetic,
                de_identification: "Invented synthetic data only; no patient or machine captures."
                    .into(),
                source: format!(
                    "{}; generated by generate_protocol_recordings using repository encoders and a local program peer; {}",
                    protocol.source(),
                    case.description(protocol)
                ),
                exceptions: exceptions(&events, protocol)?,
            };
            check(run, protocol, case).await?;
            let mut output = Vec::new();
            recording::write_header(&mut output, &header, recording::Limits::default())?;
            for item in &events {
                recording::write_event(&mut output, item, recording::Limits::default())?;
            }
            let (read_header, read_events) =
                recording::read(&output[..], recording::Limits::default())?;
            assert_eq!(read_header, header);
            assert_eq!(read_events, events);
            std::fs::write(path(protocol, case), output)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn adr0009_protocol_recordings_replay_exact_host_bytes_and_durable_outcomes()
-> Result<(), Error> {
    for protocol in Protocol::ALL {
        for case in Case::ALL {
            let (header, events) = recording::read(
                std::fs::File::open(path(protocol, case))?,
                recording::Limits::default(),
            )?;
            assert_eq!(header.evidence, Evidence::Synthetic);
            assert_eq!(header.exceptions, exceptions(&events, protocol)?);
            eprintln!("{} {}", protocol.key(), case.key());
            let mut run = Running::start(protocol.definition()).await?;
            let program = Program::load(&run.path)?;
            let stack = program
                .machines()
                .first()
                .ok_or("machine missing")?
                .snapshot()
                .effective()
                .stack();
            let deadline = Duration::from_millis(u64::from(stack.operation_timeout_ms()));
            assert_eq!(header.stack, stack.layer_names().map(str::to_owned));
            analyte_sim::replay(&mut run.peer, &events, &header.exceptions, deadline).await?;
            if (protocol == Protocol::Autolyser || protocol == Protocol::Vidas)
                && case == Case::LostAck
            {
                eof(&mut run.peer, deadline).await?;
            }
            check(run, protocol, case).await?;
        }
    }
    Ok(())
}
