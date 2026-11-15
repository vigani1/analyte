//! ADR 0008: the reference app retains optional order context without normalization.
use analyte_contract::{Order, SigningSecret, decode};
use analyte_receiver::Receiver;

#[tokio::test]
async fn adr0008_seeded_patient_specimen_and_position_survive_the_feed()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let receiver = Receiver::open(
        &temp.path().join("receiver.db"),
        "synthetic-api".into(),
        SigningSecret::parse("whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=")?,
    )?;
    let bytes = include_bytes!("../../../docs/contract/vectors/order-context.json");
    receiver.seed(vec![decode::<Order>(bytes)?]).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/orders", listener.local_addr()?);
    let router = receiver.router();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let response = client
        .get(url)
        .bearer_auth("synthetic-api")
        .send()
        .await?
        .bytes()
        .await?;
    let page: serde_json::Value = serde_json::from_slice(&response)?;
    assert_eq!(
        page.pointer("/orders/0"),
        Some(&serde_json::from_slice::<serde_json::Value>(bytes)?)
    );
    stop.send(()).map_err(|()| "receiver stopped")?;
    task.await??;
    receiver
        .shutdown()
        .map_err(|_| "receiver worker panicked")?;
    Ok(())
}
