//! Checks that pure mapping cannot bypass the LAW host custody boundary.
use super::*;

#[test]
fn adr0003_0010_law_status_mapping_alone_never_promises_aa()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut mapped = analyte_mapping::Output::default();
    mapped.status_notices = vec![analyte_mapping::law::StatusNotice {
        position: analyte_mapping::Position {
            record: 4,
            field: 2,
        },
        processing: analyte_model::ProcessingMode::Production,
        execution: analyte_model::AwosExecution::InProgress,
        order: analyte_model::SourceText::new("OBR||awos-1||test^Test^99LAB")?,
        specimen: analyte_model::SourceText::new("SPM|1")?,
        container: analyte_model::SourceText::new("SAC|||tube")?,
        control: analyte_model::SourceText::new("ORC|SC||||IP")?,
        estimate: analyte_mapping::law::AvailabilityEstimate::Absent,
    }];

    let reading = map_reading(Ok(mapped))?;
    assert!(!reading.accepted);
    assert!(matches!(reading.reading, Reading::Failed(_)));
    assert!(reading.corrections.is_empty());
    Ok(())
}
