//! Checks that shipped definitions activate shared identity and software version handling.
use super::*;
use analyte_config::{ChoiceKind, ChoiceSelections, DefinitionDetails, Stack, VersionEvidence};
use analyte_model::{IdentityStatus, ObservationIdentity, ResultContent};
use analyte_store::{Interpretation, Reading, RevisionOutcome, reconciled_outcome};

fn shipped() -> Result<impl Iterator<Item = PathBuf>, Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
    Ok(definitions(&root)?
        .into_iter()
        .filter(|path| path.ends_with("machine.toml")))
}

fn answers(
    details: &DefinitionDetails,
    choices: &ChoiceSelections,
) -> Result<ReplaySettings, Error> {
    Ok(ReplaySettings {
        choices: Some(toml::to_string(choices)?.parse()?),
        autolyser_poll_seconds: details.asks_autolyser_poll.then_some(30),
        vidas_patient_namespace: details
            .asks_vidas_patient_namespace
            .then(|| "synthetic".into()),
        ..ReplaySettings::default()
    })
}

// ADR 0032: every offered combination must load as an explicit site choice.
#[test]
fn every_shipped_choice_combination_loads() -> Result<(), Error> {
    for path in shipped()? {
        let text = std::fs::read_to_string(&path)?;
        let details = DefinitionDetails::parse("machine.toml", &text)?;
        let mut combinations = vec![ChoiceSelections::new()];
        for choice in &details.choices {
            combinations = combinations
                .into_iter()
                .flat_map(|selected| {
                    choice.values.iter().map(move |value| {
                        let mut selected = selected.clone();
                        selected.insert(choice.name.clone(), value.value.clone());
                        selected
                    })
                })
                .collect();
        }
        for selected in combinations {
            let validated = ready_definition(&text, &answers(&details, &selected)?)
                .map_err(|error| format!("{} with {selected:?}: {error}", path.display()))?;
            for choice in validated.effective().choices() {
                assert_eq!(
                    selected.get(&choice.name),
                    Some(&choice.value),
                    "{}",
                    path.display()
                );
            }
            assert_eq!(
                validated.effective().choices().len(),
                selected.len(),
                "{}",
                path.display()
            );
        }
    }
    Ok(())
}

// ADRs 0032 and 0036: machines list warns only for the selected unchecked value.
#[tokio::test]
async fn shipped_software_choices_warn_only_when_unchecked() -> Result<(), Error> {
    for path in shipped()? {
        let text = std::fs::read_to_string(&path)?;
        let details = DefinitionDetails::parse("machine.toml", &text)?;
        let Some(software) = details
            .choices
            .iter()
            .find(|choice| choice.kind == ChoiceKind::SoftwareVersion)
        else {
            continue;
        };
        let mut selected = details
            .choices
            .iter()
            .map(|choice| (choice.name.clone(), choice.values[0].value.clone()))
            .collect::<ChoiceSelections>();
        let temp = tempfile::tempdir()?;
        let definition = analyte_fixtures::write_definition(temp.path(), "fixture", &text)?;
        let program = temp.path().join("program.toml");
        let mut entries = String::new();
        for (index, value) in software.values.iter().enumerate() {
            selected.insert(software.name.clone(), value.value.clone());
            let serial = details.kind == analyte_config::TransportKind::Serial;
            let endpoint = match details.kind {
                analyte_config::TransportKind::Serial => format!("COM{}", index + 3),
                analyte_config::TransportKind::WatchedFolder => {
                    let folder = temp.path().join(format!("incoming-{index}"));
                    std::fs::create_dir(&folder)?;
                    folder.to_string_lossy().into_owned()
                }
                _ => format!("127.0.0.1:{}", index + 5600),
            };
            let settings = site(&endpoint, &answers(&details, &selected)?, serial)?;
            std::fs::write(temp.path().join(format!("site-{index}.toml")), settings)?;
            entries.push_str(&format!(
                "[[machines]]\nid='fixture-{index}'\ndefinition='{definition}'\nsite='site-{index}.toml'\n"
            ));
        }
        std::fs::write(&program, analyte_fixtures::program(&entries))?;
        let output = tokio::time::timeout(
            HANG_GUARD,
            tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
                .arg("--config")
                .arg(&program)
                .args(["machines", "list", "--json"])
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let listed: List = serde_json::from_slice(&output.stdout)?;
        assert_eq!(
            listed.machines.len(),
            software.values.len(),
            "{}",
            path.display()
        );
        for (index, value) in software.values.iter().enumerate() {
            let machine = listed
                .machines
                .iter()
                .find(|machine| machine.id == format!("fixture-{index}"))
                .ok_or("software choice missing from machines list")?;
            assert_eq!(
                machine
                    .choices_words
                    .as_deref()
                    .ok_or("choice summary missing")?
                    .contains("Software version not checked against a manual"),
                value.version_evidence == VersionEvidence::Unchecked,
                "{}: {}",
                path.display(),
                value.value.as_str(),
            );
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct List {
    machines: Vec<ListedMachine>,
}

#[derive(Deserialize)]
struct ListedMachine {
    id: String,
    choices_words: Option<String>,
}

// ADR 0036: every exact band token is named by the definition's evidence.
#[test]
fn shipped_wire_version_bands_match_evidence() -> Result<(), Error> {
    for path in shipped()? {
        let raw: toml::Table = std::fs::read_to_string(&path)?.parse()?;
        let Some(bands) = raw
            .get("software_version")
            .and_then(|check| check.get("bands"))
            .and_then(toml::Value::as_table)
        else {
            continue;
        };
        let capabilities = raw["evidence"]["capabilities"]
            .as_array()
            .ok_or("missing evidence capabilities")?;
        for (band, tokens) in bands {
            for token in tokens.as_array().ok_or("expected wire token list")? {
                assert!(
                    capabilities
                        .iter()
                        .any(|capability| capability.get("firmware") == Some(token)),
                    "{}: token {token} for band {band} is missing from evidence",
                    path.display(),
                );
            }
        }
    }
    Ok(())
}

pub(super) fn has_recipe(text: &str) -> Result<bool, Error> {
    let raw: toml::Table = text.parse()?;
    let active = |mapping: &toml::Value| {
        mapping.get("identity").is_some_and(|identity| {
            identity.get("evidence").is_some()
                && identity
                    .get("run")
                    .and_then(toml::Value::as_array)
                    .is_some_and(|run| !run.is_empty())
        })
    };
    Ok(raw
        .get("codes")
        .is_some_and(|codes| codes.get("vidas_identity").is_some())
        || raw.get("mappings").is_some_and(active)
        || DefinitionDetails::parse("machine.toml", text)?
            .choices
            .iter()
            .any(|choice| {
                choice.values.iter().any(|value| {
                    value.mappings.as_ref().is_some_and(|mapping| {
                        mapping.identity.evidence.is_some() && !mapping.identity.run.is_empty()
                    })
                })
            }))
}

// ADR 0035: replay the same synthetic upload, then change only its result value.
// A repeat adds a receipt but no delivery. The edit keeps the claimed identity for review.
pub(super) async fn assert_repeat_uploads(
    path: &Path,
    events: &[Event],
    header: &Header,
    stack: &Stack,
    incoming: Option<&Path>,
    deadline: Duration,
) -> Result<bool, Error> {
    let db = database(path)?;
    if count(&db, "interpretations")? != 1 {
        return Ok(false);
    }
    let Some((first_receipt, first)) = latest_result(&db)? else {
        return Ok(false);
    };
    if first.requires_parking() {
        return Ok(false);
    }
    // A resolved key alone cannot authorize production delivery (ADR 0003).
    let deliveries = i64::from(first.may_deliver_to_production());
    assert_eq!(
        count(&db, "outbox")?,
        deliveries,
        "first upload delivery count"
    );
    let IdentityStatus::Resolved(identity) = first.identity else {
        return Err("a qualifying upload must have a resolved identity".into());
    };
    let [part] = first.parts.as_slice() else {
        return Ok(false);
    };
    let [value] = part.values.as_slice() else {
        return Ok(false);
    };
    let Some((changed, original)) = edit_value(stack, events, value.original.as_str())? else {
        return Ok(false);
    };
    let parked = count(&db, "parked")?;
    assert_eq!(
        count(&db, "observation_revisions")?,
        1,
        "first upload created one revision"
    );
    for (total, upload) in [(2, events), (3, changed.as_slice())] {
        if let Some(incoming) = incoming {
            // A repeated file has a fresh source name, even with identical contents.
            let bytes: Vec<_> = upload
                .iter()
                .flat_map(|event| event.bytes.iter().copied())
                .collect();
            std::fs::write(incoming.join(format!("repeat-{total}")), bytes)?;
        } else {
            let mut peer = connect(path, "fixture").await?;
            analyte_sim::replay(&mut peer, upload, &header.exceptions, deadline).await?;
            peer.shutdown().await?;
        }
        wait_database_count(path, "SELECT count(*) FROM interpretations", [], total).await?;
        assert_eq!(
            count(&db, "outbox")?,
            deliveries,
            "repeat created an extra delivery"
        );
        assert_eq!(
            count(&db, "observation_revisions")?,
            1,
            "repeat created an extra revision"
        );
        let (receipt, result) =
            latest_result(&db)?.ok_or("repeat must retain exactly one result")?;
        assert_ne!(
            receipt, first_receipt,
            "each upload must have its own receipt"
        );
        if total == 2 {
            assert_eq!(result.identity, IdentityStatus::Resolved(identity.clone()));
            assert_eq!(count(&db, "parked")?, parked, "exact resend was held");
        } else {
            let [part] = result.parts.as_slice() else {
                return Err("changed upload must have one result part".into());
            };
            let [value] = part.values.as_slice() else {
                return Err("changed upload must have one result value".into());
            };
            assert_eq!(value.original.as_str(), original);
            assert_conflict(&db, &receipt, &identity.observation)?;
        }
    }
    Ok(true)
}

fn count(db: &rusqlite::Connection, table: &str) -> Result<i64, Error> {
    Ok(
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}

fn latest_result(db: &rusqlite::Connection) -> Result<Option<(String, ResultContent)>, Error> {
    let (receipt, evidence): (String, String) = db.query_row(
        "SELECT receipt, evidence FROM interpretations ORDER BY rowid DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let evidence: Interpretation = serde_json::from_str(&evidence)?;
    if let Reading::Results(mut results) = evidence.outcome
        && results.len() == 1
        && let Some(result) = results.pop()
    {
        return Ok(Some((receipt, result)));
    }
    Ok(None)
}

fn assert_conflict(
    db: &rusqlite::Connection,
    receipt: &str,
    expected: &ObservationIdentity,
) -> Result<(), Error> {
    let (reason, claimed): (String, String) = db.query_row(
        "SELECT json_extract(evidence, '$.reason'), json_extract(evidence, '$.context.Result.claimed')
         FROM parked WHERE resolution IS NULL AND json_extract(evidence, '$.targets[0]') = ?1",
        [receipt.trim_matches('"')],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(reconciled_outcome(&reason), Some(RevisionOutcome::Conflict));
    assert_eq!(
        &serde_json::from_str::<ObservationIdentity>(&claimed)?,
        expected
    );
    Ok(())
}

fn edit_value(
    stack: &Stack,
    events: &[Event],
    value: &str,
) -> Result<Option<(Vec<Event>, String)>, Error> {
    let Some(offset) = value
        .bytes()
        .position(|byte| byte.is_ascii_digit())
        .or_else(|| value.bytes().position(|byte| byte.is_ascii_alphabetic()))
    else {
        return Ok(None);
    };
    let inbound =
        |event: &&Event| event.link.is_none() && event.direction == Direction::MachineToHost;
    let mut wire: Vec<_> = events
        .iter()
        .filter(inbound)
        .flat_map(|event| event.bytes.iter().copied())
        .collect();
    let mut matches = wire
        .windows(value.len())
        .enumerate()
        .filter(|(_, bytes)| *bytes == value.as_bytes());
    let (Some((start, _)), None) = (matches.next(), matches.next()) else {
        return Ok(None);
    };
    let at = start + offset;
    let old = wire[at];
    let new = match old {
        b'9' => b'0',
        b'z' => b'a',
        b'Z' => b'A',
        _ => old + 1,
    };
    wire[at] = new;
    // The byte count and event boundaries stay fixed. The additive checksums
    // change by the same amount as the one payload byte, regardless of scope.
    let checksum = match stack {
        Stack::Astm(analyte_config::RecordTransport::Stream(_)) => Some((&[3, 23][..], 0, true)),
        Stack::Vidas(vidas) => Some((
            &[29][..],
            vidas.policy().suffix.len(),
            vidas.policy().hex_case.into(),
        )),
        Stack::Astm(analyte_config::RecordTransport::Folder(_)) | Stack::Hl7(_) => None,
        Stack::Autolyser(_) | Stack::Law(_) | Stack::Poct(_) | Stack::Record(_) => {
            return Err("identity replay has no upload editor for this stack".into());
        }
    };
    if let Some((markers, suffix, uppercase)) = checksum {
        let end = wire[at..]
            .iter()
            .position(|byte| markers.contains(byte))
            .ok_or("checksum marker missing")?;
        let check = at + end + 1 + suffix;
        let bytes = wire.get_mut(check..check + 2).ok_or("checksum missing")?;
        let sum = u8::from_str_radix(std::str::from_utf8(bytes)?, 16)?
            .wrapping_sub(old)
            .wrapping_add(new);
        let hex = if uppercase {
            format!("{sum:02X}")
        } else {
            format!("{sum:02x}")
        };
        bytes.copy_from_slice(hex.as_bytes());
    }
    let mut changed = events.to_vec();
    let mut wire = wire.into_iter();
    for event in &mut changed {
        if inbound(&&*event) {
            let len = event.bytes.len();
            event.bytes = wire.by_ref().take(len).collect();
        }
    }
    let mut original = value.as_bytes().to_vec();
    original[offset] = new;
    Ok(Some((changed, String::from_utf8(original)?)))
}
