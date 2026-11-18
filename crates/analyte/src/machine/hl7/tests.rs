//! Tests interrupted input custody under ADR 0003 and MLLP R1 section 1.2.
use super::*;
use crate::Error;
use crate::installation::config::Program;
use analyte_fixtures::UnlimitedSpace as Space;
use std::path::Path;

fn program(root: &Path, definition: &str, endpoint: &str) -> Result<Program, Error> {
    let path = analyte_fixtures::write_machine(root, definition, endpoint)?;
    Program::load(&path).map_err(Into::into)
}

// ADR 0003; HL7 MLLP R1 §1.2
#[tokio::test]
async fn coalesced_read_survives_ack_write_failure_and_cancel() -> Result<(), Error> {
    for orders in [false, true] {
        for cancel in [false, true] {
            let temp = crate::admin::private_tempdir()?;
            let mut definition =
                include_str!("../../../../core/config/examples/hl7.toml").to_owned();
            if orders {
                definition.push_str(include_str!(
                    "../../../../core/config/examples/hl7_orders.toml"
                ));
            }
            let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
            let machine = program.machines().first().ok_or("machine")?;
            let connection = intake::ConnectionEvidence::new(machine)?;
            let analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(settings)) =
                machine.snapshot.effective().stack()
            else {
                return Err("HL7 settings".into());
            };
            let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
            let mut state = Host::open(&connection, settings, &store)?;
            let first =
                b"\x0bMSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|one|P|2.5.1\rPID|1\r\x1c\r";
            let second =
                b"\x0bMSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|two|P|2.5.1\rPID|1\r\x1c\r";
            let bytes = [first.as_slice(), second.as_slice()].concat();
            // An offline write endpoint deterministically refuses the first ACK.
            let (mut conn, _io) = analyte_transport::connection(analyte_transport::Limits::new(
                4,
                Duration::from_secs(1),
            )?);
            let (_stop, mut stop) = watch::channel(false);
            if cancel {
                let receive = state.receive_bytes(bytes, &mut conn, &mut stop);
                tokio::pin!(receive);
                tokio::select! { biased; result = &mut receive => { result?; return Err("expected pending first commit".into()); }, () = std::future::ready(()) => {} }
            } else {
                assert!(
                    state
                        .receive_bytes(bytes, &mut conn, &mut stop)
                        .await
                        .is_err()
                );
            }
            assert_eq!(state.input_evidence.fed, first.len());
            let mut result = Ok(());
            state
                .close(
                    &mut Context {
                        conn: &mut conn,
                        stop: stop.clone(),
                    },
                    &mut result,
                )
                .await?;
            result?;
            let expected = second.to_vec();
            store
                .request::<_, analyte_store::OperatorError>(crate::deadline()?, move |s| {
                    let parked = s.parked(0)?;
                    assert!(parked.iter().any(|p| {
                        s.parked_receipt(p.id)
                            .and_then(|id| s.receipt(id))
                            .ok()
                            .flatten()
                            .is_some_and(|r| r.raw == expected)
                    }));
                    Ok::<_, analyte_store::OperatorError>(())
                })
                .await?;
            store.shutdown(crate::deadline()?).await?;
        }
    }
    Ok(())
}
