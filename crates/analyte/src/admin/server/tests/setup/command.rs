//! Tests `POST /command/lab-apps` through the real router: only the admin key in its header admits it, and it stores a lab app and its keys exactly as the Lab apps page does (ADR 0028).
use super::*;

/// Synthetic keys; neither may appear in an answer, a settings file or an audit row.
const COMMAND_API_KEY: &str = "synthetic-route-api-key-5RT";
const COMMAND_SIGNING_KEY: &str = analyte_fixtures::SECRET;

/// The command's request to add lab app `spare`, as `actor`, with `extra` fields.
fn body(actor: &str, extra: &str) -> String {
    format!(
        concat!(
            "{{\"actor\":\"{}\",\"change\":\"added\",\"name\":\"spare\",",
            "\"base_url\":\"https://spare.example.invalid\",\"freshness_seconds\":\"\",",
            "\"request_timeout_seconds\":\"\",\"api_key\":\"{}\",\"signing_key\":\"{}\"{}}}"
        ),
        actor, COMMAND_API_KEY, COMMAND_SIGNING_KEY, extra
    )
}

impl Fixture {
    /// Posts `json` to the command route with `headers` and nothing else.
    async fn command(&self, headers: &str, json: &str) -> Result<String, Error> {
        request(
            &self.host,
            "/command/lab-apps",
            &format!("{headers}Content-Type: application/json\r\n"),
            Some(json),
        )
        .await
    }

    fn stored(&self) -> Result<crate::installation::credentials::Stored, Error> {
        Ok(crate::installation::credentials::Locked::new(&self.data).read()?)
    }
}

// ADR 0028 §3: the route needs the admin key in the Authorization header. No
// key, a wrong key, or a browser's signed-in session alone is refused with 401,
// and a foreign Origin is refused even with the key; nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn the_command_route_takes_only_the_admin_key_in_its_header() -> Result<(), Error> {
    let fixture = start().await?;
    let json = body("it-test", "");
    let bearer = format!("Authorization: Bearer {}\r\n", fixture.bearer);
    for (headers, status) in [
        (String::new(), "HTTP/1.1 401"),
        ("Authorization: Bearer wrong\r\n".to_owned(), "HTTP/1.1 401"),
        (format!("Cookie: {}\r\n", fixture.cookie), "HTTP/1.1 401"),
        (
            format!("{bearer}Origin: http://evil.example.invalid\r\n"),
            "HTTP/1.1 401",
        ),
    ] {
        let answer = fixture.command(&headers, &json).await?;
        assert!(answer.starts_with(status), "{headers}: {answer}");
        assert!(!answer.contains(COMMAND_API_KEY) && !answer.contains(COMMAND_SIGNING_KEY));
    }
    assert!(!fixture.config.with_file_name("lab-apps.toml").exists());
    assert_eq!(fixture.stored()?.value("spare-api"), None);
    assert!(fixture.audited("lab-app-add").await?.is_empty());
    Ok(())
}

// ADR 0028 §1 and §4: with the key, the route runs the Lab apps page's add:
// the keys land in the locked file only, the settings in lab-apps.toml, and the
// audit row names the command's actor and the command line. The answer says
// which keys were stored, never a value. An unknown field, an invalid actor or
// a body over the bound is refused and changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_command_route_stores_keys_as_the_lab_apps_page_does() -> Result<(), Error> {
    let fixture = start().await?;
    let bearer = format!("Authorization: Bearer {}\r\n", fixture.bearer);
    let refused = [
        (body("it-test", ",\"extra\":\"x\""), "HTTP/1.1 400"),
        (body("admin-session-forged", ""), "HTTP/1.1 400"),
        (
            format!("{}{}", body("it-test", ""), " ".repeat(20_000)),
            "HTTP/1.1 413",
        ),
    ];
    for (json, status) in &refused {
        let answer = fixture.command(&bearer, json).await?;
        assert!(answer.starts_with(*status), "{answer}");
        assert!(!answer.contains(COMMAND_API_KEY) && !answer.contains(COMMAND_SIGNING_KEY));
    }
    assert!(!fixture.config.with_file_name("lab-apps.toml").exists());
    assert_eq!(fixture.stored()?.value("spare-api"), None);
    assert!(fixture.audited("lab-app-add").await?.is_empty());
    // ADR 0028: a valid request above the server's default 1 KiB limit still
    // fits the command route's larger bound.
    let json = format!("{}{}", body("it-test", ""), " ".repeat(2048));
    let answer = fixture.command(&bearer, &json).await?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    for part in [
        "\"saved\":{\"lines\":[\"spare is added.\"",
        "\"Key stored: spare-api\"",
        "\"Key stored: spare-signing\"",
        "\"audit_failed\":null",
    ] {
        assert!(answer.contains(part), "{part}\n{answer}");
    }
    assert!(!answer.contains(COMMAND_API_KEY) && !answer.contains(COMMAND_SIGNING_KEY));
    let stored = fixture.stored()?;
    assert_eq!(stored.value("spare-api"), Some(COMMAND_API_KEY));
    assert_eq!(stored.value("spare-signing"), Some(COMMAND_SIGNING_KEY));
    let lab_apps = std::fs::read_to_string(fixture.config.with_file_name("lab-apps.toml"))?;
    assert!(
        lab_apps.contains("https://spare.example.invalid"),
        "{lab_apps}"
    );
    assert!(!lab_apps.contains(COMMAND_API_KEY) && !lab_apps.contains(COMMAND_SIGNING_KEY));
    let audits = fixture.audited("lab-app-add").await?;
    assert_eq!(audits.len(), 1, "{audits:?}");
    let audit = audits.first().ok_or("no audit")?;
    assert!(
        audit.starts_with("it-test spare command line added spare: "),
        "{audit}"
    );
    assert!(!audit.contains(COMMAND_API_KEY) && !audit.contains(COMMAND_SIGNING_KEY));
    // The page's own refusal: the name is taken now, and nothing changes.
    let again = fixture.command(&bearer, &body("it-test", "")).await?;
    assert!(again.starts_with("HTTP/1.1 422"), "{again}");
    assert!(again.contains("\"refused\":{\"reason\":"), "{again}");
    assert!(again.contains("\"keys_not_stored\":false"), "{again}");
    assert_eq!(fixture.audited("lab-app-add").await?.len(), 1);
    Ok(())
}
