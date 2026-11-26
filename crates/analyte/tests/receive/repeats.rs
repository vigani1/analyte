//! Verifies the site's repeated-results choice end to end over HL7 (ADR 0035, 2026-11-26).
use super::*;
use analyte_fixtures::ConfigEdit as _;

// ADR 0035 (2026-11-26); AGENTS.md clinical rule 4. The same ORU sent twice:
// "automatic" keeps the resend once with one delivery; "always ask me" stores
// it, holds it in Needs review and delivers nothing more. Either way the
// machine's first upload is delivered once.
#[tokio::test]
async fn a_resend_is_held_only_when_the_machine_always_asks() -> Result<(), Error> {
    for (choice, second_ack, open) in [
        ("", "MSA|AA|message1", 0),
        ("repeated_results = \"always_ask\"\n", "MSA|AE|message1", 1),
    ] {
        let temp = tempfile::tempdir()?;
        let path = config(temp.path(), false)?;
        let site = temp.path().join("site.toml");
        let text = std::fs::read_to_string(&site)?;
        std::fs::write(
            &site,
            text.rewrite("[destination]", &format!("{choice}[destination]")),
        )?;
        let program = Program::load(&path)?;
        let (shutdown, stop) = oneshot::channel();
        let service = tokio::spawn(serve(
            Program::load(&path)?,
            Box::new(Space(u64::MAX)),
            async {
                stop.await?;
                Ok(())
            },
        ));
        let mut peer = connect(&program).await?;
        let wire = analyte_framing::hl7::encode(ORU, analyte_framing::hl7::Limit::new(1048576)?)?;
        peer.write_all(&wire).await?;
        let first = String::from_utf8(read_ack(&mut peer).await?)?;
        assert!(first.contains("MSA|AA|message1"), "{choice}: {first}");
        peer.write_all(&wire).await?;
        let second = String::from_utf8(read_ack(&mut peer).await?)?;
        assert!(second.contains(second_ack), "{choice}: {second}");
        let store = stored(&program)?;
        assert_eq!(store.receipts("")?.len(), 2, "{choice}: both uploads kept");
        assert_eq!(store.deliveries(0)?.len(), 1, "{choice}: one delivery");
        assert_eq!(store.parked_counts()?.open, open, "{choice}");
        shutdown.send(()).map_err(|()| "shutdown")?;
        tokio::time::timeout(HANG_GUARD, service).await???;
    }
    Ok(())
}
