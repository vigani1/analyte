//! Tests untrusted app diagnostics without stopping delivery ownership.
use super::*;

// ADR 0008
#[tokio::test]
async fn app_diagnostics_park_and_worker_continues() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for status in [400, 409, 422] {
        for detail in [
            "\u{e9}".repeat(800),
            "line one\nline two".into(),
            "control\u{1}\u{7f}\tend".into(),
        ] {
            let directory = tempfile::tempdir()?;
            let mut store = store(&directory.path().join("store.db")).await?;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let destinations = [destination(&format!("http://{}", listener.local_addr()?))?];
            let secrets = Secrets("key");
            let worker = Worker::for_destination(&store, &destinations[0], &secrets);
            for run in ["rejected", "next"] {
                let metadata = receipt()?;
                let result = make_result(metadata.id, run)?;
                let delivery = encode_delivery(
                    &result,
                    &metadata,
                    &model()?,
                    Identifier::new("app")?,
                    jiff::Timestamp::now(),
                )?;
                enqueue(&store, metadata, result, delivery).await?;
            }
            let server = async {
                for code in [status, 200] {
                    let (mut stream, _) = listener.accept().await?;
                    let mut headers = Vec::new();
                    while !headers.ends_with(b"\r\n\r\n") {
                        if headers.len() > 8192 {
                            return Err("headers too large".into());
                        }
                        headers.push(stream.read_u8().await?);
                    }
                    let headers = std::str::from_utf8(&headers)?;
                    let length: usize = headers
                        .lines()
                        .filter_map(|s| s.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .ok_or("length")?
                        .1
                        .trim()
                        .parse()?;
                    let mut payload = vec![0; length];
                    stream.read_exact(&mut payload).await?;
                    let body = if code == 200 {
                        serde_json::json!({"contract_version":1,"status":"accepted"})
                    } else {
                        serde_json::json!({"contract_version":1,"error":"conflict","detail":detail})
                    }
                    .to_string();
                    stream.write_all(format!("HTTP/1.1 {code} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await?;
                }
                Ok::<_, Box<dyn std::error::Error>>(())
            };
            let client =
                async {
                    assert_eq!(worker.deliver_once().await?, DeliveryOutcome::Parked);
                    let parked = store.request(until()?, |s| s.parked(0)).await?;
                    let expected = crate::delivery::reason(&detail);
                    assert!(parked.iter().any(|p| p.evidence.contains(
                        &serde_json::to_string(expected.as_str()).expect("string JSON")
                    )));
                    assert_eq!(
                        worker.deliver_once().await?,
                        DeliveryOutcome::Done(analyte_contract::DeliveryStatus::Accepted)
                    );
                    Ok::<_, Box<dyn std::error::Error>>(())
                };
            let (server, client) =
                tokio::time::timeout(HANG_GUARD, async { tokio::join!(server, client) }).await?;
            server?;
            client?;
            store.shutdown(until()?).await?;
        }
    }
    Ok(())
}

// ADR 0008
#[test]
fn diagnostic_text_is_bounded_with_visible_controls() {
    let reason = crate::delivery::reason(&"\u{e9}".repeat(800));
    assert!(reason.as_str().len() <= 1024);
    assert!(reason.as_str().ends_with("\u{2026} [truncated]"));
    assert!(reason.as_str().starts_with("\u{e9}"));
    assert_eq!(
        crate::delivery::reason("a\n\t\u{7f}").as_str(),
        r"a\n\t\u{7f}"
    );
    assert!(!reason.as_str().chars().any(char::is_control));
    assert_eq!(
        crate::delivery::reason(&"x".repeat(1024)).as_str().len(),
        1024
    );
    assert!(!crate::delivery::reason("").as_str().is_empty());
}
