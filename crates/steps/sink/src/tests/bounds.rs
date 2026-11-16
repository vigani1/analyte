//! Bounds regression scenarios.
use super::*;

#[test]
fn adr0008_build_refuses_nonproduction_and_parked_evidence() -> TestResult {
    let receipt = receipt()?;
    let mut result = make_result(receipt.id, "build-validation-run")?;
    for mode in [
        ProcessingMode::Training,
        ProcessingMode::Debug,
        ProcessingMode::QualityControl,
    ] {
        result.processing = mode;
        assert!(matches!(
            build_delivery(
                &result,
                &receipt,
                Identifier::new("app")?,
                jiff::Timestamp::now()
            ),
            Err(Error::ProcessingMode)
        ));
    }
    result.processing = ProcessingMode::Production;
    result.sample = None;
    assert!(matches!(
        build_delivery(
            &result,
            &receipt,
            Identifier::new("app")?,
            jiff::Timestamp::now()
        ),
        Err(Error::RequiresParking)
    ));
    Ok(())
}

#[tokio::test]
async fn mutation_contract_v1_rule7_response_body_ceiling_is_inclusive() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for length in [
        analyte_contract::MAX_DOCUMENT_BYTES,
        analyte_contract::MAX_DOCUMENT_BYTES + 1,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = async {
            let (mut stream, _) = listener.accept().await?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() > 8192 {
                    return Err(std::io::Error::other("test headers too large"));
                }
                headers.push(stream.read_u8().await?);
            }
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await?;
            stream.write_all(&vec![b'x'; length]).await?;
            Ok::<_, std::io::Error>(())
        };
        let client = async {
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()?
                .get(format!("http://{address}"))
                .send()
                .await?;
            let result = body(response).await;
            if length == analyte_contract::MAX_DOCUMENT_BYTES {
                assert_eq!(result?.len(), length);
            } else {
                assert!(matches!(result, Err(Error::ResponseTooLarge)));
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (server, client) =
            tokio::time::timeout(HANG_GUARD, async { tokio::join!(server, client) }).await?;
        server?;
        client?;
    }
    Ok(())
}

#[test]
fn mutation_rule7_contract_v1_destination_limits_validate_each_independent_boundary() -> TestResult
{
    let app = Identifier::new("app")?;
    let secret = Identifier::new("secret")?;
    let construct = |limits| {
        Destination::new(
            app.clone(),
            "https://example.invalid/",
            secret.clone(),
            limits,
        )
    };
    for (field, values) in [
        (0, vec![Duration::ZERO, Duration::from_secs(301)]),
        (1, vec![Duration::ZERO, Duration::from_secs(301)]),
        (2, vec![Duration::from_millis(999)]),
        (3, vec![Duration::ZERO, Duration::from_secs(86401)]),
        (4, vec![Duration::from_millis(999)]),
        (5, vec![Duration::ZERO, Duration::from_secs(86401)]),
    ] {
        for value in values {
            let mut l = limits();
            match field {
                0 => l.request_timeout = value,
                1 => l.store_timeout = value,
                2 => l.backoff_min = value,
                3 => l.backoff_max = value,
                4 => l.freshness = value,
                5 => l.poll_interval = value,
                _ => unreachable!("fixed test fields"),
            };
            assert!(matches!(construct(l), Err(Error::Configuration)));
        }
    }
    for size in [0, 101] {
        let mut l = limits();
        l.page_size = size;
        assert!(matches!(construct(l), Err(Error::Configuration)));
    }
    let mut l = limits();
    l.max_pages = 0;
    assert!(matches!(construct(l), Err(Error::Configuration)));
    let mut l = limits();
    l.request_timeout = Duration::from_secs(300);
    l.store_timeout = Duration::from_secs(300);
    l.backoff_max = Duration::from_secs(86400);
    l.poll_interval = Duration::from_secs(86400);
    l.page_size = 100;
    assert_eq!(construct(l)?.id().as_str(), "app");
    let mut equal = limits();
    equal.backoff_max = equal.backoff_min;
    assert!(construct(equal).is_ok());
    Ok(())
}

#[test]
fn mutation_contract_v1_rule7_credentials_reject_each_invalid_component() -> TestResult {
    struct Supplied {
        key: String,
        count: usize,
    }
    impl SecretsSource for Supplied {
        fn credentials(&self, _: &Identifier) -> Result<Credentials, Error> {
            Ok(Credentials {
                api_key: self.key.clone(),
                signing_secrets: vec![analyte_contract::SigningSecret::parse(SECRET)?; self.count],
            })
        }
    }
    let d = destination("http://127.0.0.1:1234")?;
    for (key, count) in [
        (String::new(), 1),
        ("x".repeat(4097), 1),
        ("é".into(), 1),
        ("a\nb".into(), 1),
        ("key".into(), 0),
        ("key".into(), 9),
    ] {
        assert!(matches!(
            d.credentials(&Supplied { key, count }),
            Err(Error::Secrets)
        ));
    }
    for count in [1, 8] {
        let c = d.credentials(&Supplied {
            key: "x".repeat(4096),
            count,
        })?;
        assert_eq!(c.api_key.len(), 4096);
        assert_eq!(c.signing_secrets.len(), count);
    }
    Ok(())
}

// ADR 0003 retention; contract v1 delivery responses preserve the app's reason.
#[test]
fn mutation_rule7_destination_url_byte_ceiling_is_inclusive() -> TestResult {
    for n in [4096, 4097] {
        let prefix = "https://example.invalid/";
        let url = format!("{prefix}{}", "x".repeat(n - prefix.len()));
        assert_eq!(
            Destination::new(
                Identifier::new("app")?,
                &url,
                Identifier::new("key")?,
                limits()
            )
            .is_ok(),
            n == 4096
        );
    }
    // The original fixture combined username and password, masking either check.
    for url in [
        "https://user@example.invalid/",
        "https://:password@example.invalid/",
    ] {
        assert!(matches!(
            Destination::new(
                Identifier::new("app")?,
                url,
                Identifier::new("key")?,
                limits()
            ),
            Err(Error::Configuration)
        ));
    }
    Ok(())
}

// Keep paused Tokio time fixed while the dedicated storage thread completes I/O.
#[tokio::test]
async fn mutation_adr0004_contract_v1_pagination_progress_and_page_bounds() -> TestResult {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    for case in 0..4 {
        let directory = tempfile::tempdir()?;
        let mut store = store(&directory.path().join("store.db")).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut destination = destination(&format!("http://{}", listener.local_addr()?))?;
        destination.limits.max_pages = 2;
        let secrets = Secrets("key");
        let poller = Poller::new(&store, &destination, &secrets);
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let server = async {
            for index in 0..3 {
                let accept = tokio::select! {biased; _=&mut stopped=>return Ok(()), accepted=listener.accept()=>accepted};
                let (mut stream, _) = accept?;
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    if headers.len() > 8192 {
                        return Err(std::io::Error::other("test headers too large"));
                    }
                    headers.push(stream.read_u8().await?);
                }
                let cursor = if case == 2 {
                    format!("cursor-{index}")
                } else {
                    "cursor".into()
                };
                let complete = (case == 1 && index == 1) || case == 3;
                let orders = if case == 3 {
                    serde_json::json!(vec![
                        serde_json::from_slice::<serde_json::Value>(
                            include_bytes!("../../../../../docs/contract/vectors/order.json")
                        )?;
                        2
                    ])
                } else {
                    serde_json::json!([])
                };
                let body=serde_json::json!({"contract_version":1,"orders":orders,"next_cursor":cursor,"complete":complete}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            }
            Ok::<_, std::io::Error>(())
        };
        let poll = async {
            let result = poller.poll_once().await;
            let _ = stop.send(());
            result
        };
        let (server, result) =
            tokio::time::timeout(HANG_GUARD, async { tokio::join!(server, poll) }).await?;
        server?;
        if case == 1 {
            result?;
        } else {
            assert!(matches!(result, Err(Error::FeedProgress)));
        }
        let health = destination
            .health(&store, jiff::Timestamp::now().as_second())
            .await?;
        assert_eq!(health.cache_trusted, case == 1);
        store.shutdown(until()?).await?;
    }
    Ok(())
}
