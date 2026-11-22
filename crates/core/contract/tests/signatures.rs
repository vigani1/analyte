//! Pins the signing shared by Analyte and the contract checker to fixed, published signatures.
#![forbid(unsafe_code)]
use analyte_contract::{MAX_DOCUMENT_BYTES, SignatureError, SigningSecret};
use serde_json::Value;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// One fixed example: secret, message ID, timestamp, body and the expected header value.
struct Example {
    secret: &'static str,
    id: &'static str,
    timestamp: &'static str,
    body: Vec<u8>,
    signature: &'static str,
}

/// Both signing entry points produce exactly the expected value, and it verifies.
fn assert_signs(example: &Example) -> TestResult {
    let key = SigningSecret::parse(example.secret)?;
    let (id, timestamp, body) = (example.id, example.timestamp, example.body.as_slice());
    assert_eq!(key.sign(id, timestamp, body)?, example.signature);
    assert_eq!(key.sign_unbounded(id, timestamp, body)?, example.signature);
    key.verify(id, timestamp, body, example.signature, timestamp.parse()?)?;
    Ok(())
}

// Standard Webhooks reference libraries (github.com/standard-webhooks/standard-webhooks):
// the fixed sign examples in libraries/rust/src/lib.rs (test_sign) and
// libraries/javascript/src/webhook.test.ts ("sign function works" and the binary
// payload example). The specification text gives no symmetric secret for its own example.
#[test]
fn signing_matches_standard_webhooks_examples() -> TestResult {
    for example in [
        Example {
            secret: "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD",
            id: "msg_27UH4WbU6Z5A5EzD8u03UvzRbpk",
            timestamp: "1649367553",
            body: br#"{"email":"test@example.com","username":"test_user"}"#.to_vec(),
            signature: "v1,tZ1I4/hDygAJgO5TYxiSd6Sd0kDW6hPenDe+bTa3Kkw=",
        },
        Example {
            secret: "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
            id: "msg_p5jXN8AQM9LWM0D4loKWxJek",
            timestamp: "1614265330",
            body: br#"{"test": 2432232314}"#.to_vec(),
            signature: "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=",
        },
        Example {
            secret: "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
            id: "msg_p5jXN8AQM9LWM0D4loKWxJek",
            timestamp: "1614265330",
            // Every byte value once, so the body is not valid UTF-8.
            body: (0..=255).collect(),
            signature: "v1,sEcZ042G8Crqgxbybb3y0eDxQR3TcfYsk11I1XULhSI=",
        },
    ] {
        assert_signs(&example)?;
    }
    Ok(())
}

// Contract v1 "HTTP and authentication": signature.json and rotated-signature.json carry
// these exact signatures; the files and the shared signing code must both keep them.
#[test]
fn signing_matches_published_contract_vectors() -> TestResult {
    const SIGNATURE: &str = "v1,bffasF61ooo78bQc8csSKFKardsXGOuZ5cd+DQRkwLA=";
    const OTHER: &str = "v1,VSnpJyaSTvVqbV13FU64fiAcWbxWt1LA+kEkyZQV1JU=";
    let signed: Value = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/signature.json"
    ))?;
    let rotated: Value = serde_json::from_str(include_str!(
        "../../../../docs/contract/vectors/rotated-signature.json"
    ))?;
    let body = signed["body"].as_str().ok_or("body")?.as_bytes().to_vec();
    assert_eq!(signed["signature"], SIGNATURE);
    assert_eq!(rotated["signature_header"], format!("{OTHER} {SIGNATURE}"));
    for (secret, signature) in [
        (
            "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            SIGNATURE,
        ),
        ("whsec_ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=", OTHER),
    ] {
        assert_signs(&Example {
            secret,
            id: "019ab000-0000-7000-8000-000000000001",
            timestamp: "1794744000",
            body: body.clone(),
            signature,
        })?;
    }
    Ok(())
}

// Contract v1 "HTTP and authentication": only the unbounded entry point signs a body over
// the document limit; metadata rules still apply to it.
#[test]
fn only_unbounded_signing_takes_an_oversized_body() -> TestResult {
    let key = SigningSecret::parse("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw")?;
    let over = vec![b'x'; MAX_DOCUMENT_BYTES + 1];
    assert!(matches!(
        key.sign("id", "1", &over),
        Err(SignatureError::Metadata)
    ));
    let signature = key.sign_unbounded("id", "1", &over)?;
    assert!(signature.starts_with("v1,"));
    assert!(matches!(
        key.verify("id", "1", &over, &signature, 1),
        Err(SignatureError::Metadata)
    ));
    assert!(matches!(
        key.sign_unbounded("a.b", "1", &over),
        Err(SignatureError::Metadata)
    ));
    Ok(())
}
