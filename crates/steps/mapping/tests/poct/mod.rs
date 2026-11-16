//! Tests poct01 b 5 15 mapping keeps written values codes units and comparators; poct01 b 6 10 nonpatient qc and null are never patient or zero.
#[path = "messages.rs"]
mod messages;
use analyte_mapping::poct::map_observations;
use analyte_model::{Comparator, IdentityStatus, QcClass, ResultStatus, ValueInterpretation};
use analyte_poct::{Message, Nullable, Role, Value};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn poct01_b_5_15_mapping_keeps_written_values_codes_units_and_comparators() -> TestResult {
    let raw = messages::observation("o", true, "<OBS.value V=\"001.2300\" U=\"OriginalUnit\"/>");
    let message = Message::parse(raw.as_bytes(), messages::limits())?;
    let received = "2026-03-12T09:50:00Z".parse()?;
    let mapped = map_observations(&message, received)?;
    let item = mapped.first().ok_or("missing mapped result")?;
    let value = item
        .part
        .values
        .as_slice()
        .first()
        .ok_or("missing result value")?;
    assert_eq!(value.original.as_str(), "001.2300");
    assert!(
        matches!(&value.parsed,ValueInterpretation::Numeric(n) if n.number.as_str() == "001.2300" && n.comparator == Some(Comparator::Less))
    );
    assert_eq!(
        item.part
            .units
            .as_ref()
            .ok_or("missing units")?
            .original_text()
            .as_str(),
        "OriginalUnit"
    );
    assert_eq!(
        item.source_code.system_name.as_deref(),
        Some("synthetic-system")
    );
    assert_eq!(
        item.part
            .flags
            .as_slice()
            .first()
            .ok_or("missing flag")?
            .as_str(),
        "<"
    );
    assert_eq!(item.part.status, ResultStatus::Unknown);
    assert_eq!(item.part.qc_class, QcClass::Patient);
    assert_eq!(
        item.patient_id.as_ref().ok_or("missing patient")?.as_str(),
        "000xyz"
    );
    assert!(item.sample.is_none());
    assert!(matches!(item.identity, IdentityStatus::Unresolved(_)));
    let source_time = &item
        .part
        .times
        .as_slice()
        .first()
        .ok_or("missing time")?
        .time;
    assert_eq!(source_time.original.as_str(), "2026-03-12T09:35:00+01:00");
    assert_eq!(source_time.offset.map(i32::from), Some(3600));
    assert_eq!(item.part.received_at, received);
    Ok(())
}

#[test]
fn poct01_b_6_10_nonpatient_qc_and_null_are_never_patient_or_zero() -> TestResult {
    let raw = messages::observation("o", false, "<OBS.value NULL=\"UNK\"/>");
    let message = Message::parse(raw.as_bytes(), messages::limits())?;
    let mapped = map_observations(&message, "2026-03-12T09:50:00Z".parse()?)?;
    let item = mapped.first().ok_or("missing mapped result")?;
    assert_eq!(item.role, Role::LiquidControl);
    assert_eq!(item.part.qc_class, QcClass::Control);
    assert!(item.patient_id.is_none());
    assert!(matches!(
        item.source_value,
        Value::Quantity(Nullable::Null(_))
    ));
    assert!(
        item.part
            .values
            .as_slice()
            .first()
            .ok_or("missing value")?
            .parsed
            .is_unsupported()
    );
    Ok(())
}

#[test]
fn poct01_b_5_22_specimen_id_preserves_case_and_leading_zeroes() -> TestResult {
    let raw = messages::observation("o",true,"<OBS.qualitative_value V=\"positive\" SN=\"source-system\" SV=\"v2\" DN=\"Source label\"/>").replace("</SVC>","<SPC><SPC.specimen_id V=\"000AbC\" SN=\"tube-system\"/><SPC.specimen_dttm V=\"2026-03-12T09:30:00+01:00\"/></SPC></SVC>");
    let message = Message::parse(raw.as_bytes(), messages::limits())?;
    let mapped = map_observations(&message, "2026-03-12T09:50:00Z".parse()?)?;
    let item = mapped.first().ok_or("missing mapped result")?;
    assert_eq!(
        item.sample.as_ref().ok_or("missing sample")?.id.as_str(),
        "000AbC"
    );
    assert!(
        matches!(&item.source_value,Value::Code(Nullable::Value(c)) if c.version.as_deref() == Some("v2") && c.display.as_deref() == Some("Source label"))
    );
    assert!(
        matches!(&item.part.values.as_slice().first().ok_or("missing value")?.parsed,ValueInterpretation::Coded(c) if c.code.namespace.as_str() == "source-system" && c.code.value.as_str() == "positive")
    );
    for raw in [messages::hello("h"), messages::status("s", "0")] {
        assert!(
            map_observations(
                &Message::parse(raw.as_bytes(), messages::limits())?,
                "2026-03-12T09:50:00Z".parse()?
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn poct01_b_6_10_source_paths_keep_distinct_real_element_positions() -> TestResult {
    let raw = messages::observation("o", true, "<OBS.value V=\"1.00\" U=\"unit\"/>");
    let (_, service) = raw.split_once("<SVC>").ok_or("missing synthetic service")?;
    let service = service
        .strip_suffix("</OBS.R01>")
        .ok_or("missing synthetic root close")?;
    let repeated = raw.replace("</OBS.R01>", &format!("<SVC>{service}</OBS.R01>"));
    let message = Message::parse(repeated.as_bytes(), messages::limits())?;
    let mapped = map_observations(&message, "2026-03-12T09:50:00Z".parse()?)?;
    assert_eq!(mapped.len(), 2);
    let mut observed_positions = Vec::new();
    for result in mapped {
        for node in result.part.source_path.as_slice() {
            let index = node
                .occurrence
                .checked_sub(1)
                .ok_or("zero source position")?;
            assert_eq!(
                message
                    .document()
                    .elements()
                    .get(index)
                    .ok_or("invalid source position")?
                    .name(),
                node.kind.as_str()
            );
            if node.kind.as_str() == "OBS" {
                observed_positions.push(node.occurrence);
            }
        }
    }
    assert_ne!(observed_positions.first(), observed_positions.get(1));
    Ok(())
}
