//! Supplies synthetic syntax test fixtures and scenarios.
//! Kept whole: Shared synthetic record fixtures and malformed grammar tables support the child scenarios.
use analyte_astm::{Delimiters, Error, Kind, Limits, Presence, ResultStatus, parse, parse_with};
use proptest::prelude::*;

fn check_header_fields_0(view: &analyte_astm::Header<'_>) {
    assert_eq!(
        view.control_id().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.password().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.sender().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.address().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.reserved().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.phone().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.characteristics().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.receiver().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.instructions().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.processing_id().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.version().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
    assert_eq!(
        view.timestamp().map(analyte_astm::Field::raw),
        Some(b"14".as_slice())
    );
}

fn check_header(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.first().and_then(analyte_astm::Header::new) {
        check_header_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_patient_fields_0(view: &analyte_astm::Patient<'_>) {
    assert_eq!(
        view.practice_id().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.laboratory_id().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.third_id().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.name().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.maiden_name().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.birthdate().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.sex().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.race().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.address().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.reserved().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.phone().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
    assert_eq!(
        view.physician().map(analyte_astm::Field::raw),
        Some(b"14".as_slice())
    );
}

fn check_patient_fields_1(view: &analyte_astm::Patient<'_>) {
    assert_eq!(
        view.special_1().map(analyte_astm::Field::raw),
        Some(b"15".as_slice())
    );
    assert_eq!(
        view.special_2().map(analyte_astm::Field::raw),
        Some(b"16".as_slice())
    );
    assert_eq!(
        view.height().map(analyte_astm::Field::raw),
        Some(b"17".as_slice())
    );
    assert_eq!(
        view.weight().map(analyte_astm::Field::raw),
        Some(b"18".as_slice())
    );
    assert_eq!(
        view.diagnosis().map(analyte_astm::Field::raw),
        Some(b"19".as_slice())
    );
    assert_eq!(
        view.medications().map(analyte_astm::Field::raw),
        Some(b"20".as_slice())
    );
    assert_eq!(
        view.diet().map(analyte_astm::Field::raw),
        Some(b"21".as_slice())
    );
    assert_eq!(
        view.practice_1().map(analyte_astm::Field::raw),
        Some(b"22".as_slice())
    );
    assert_eq!(
        view.practice_2().map(analyte_astm::Field::raw),
        Some(b"23".as_slice())
    );
    assert_eq!(
        view.admission_dates().map(analyte_astm::Field::raw),
        Some(b"24".as_slice())
    );
    assert_eq!(
        view.admission_status().map(analyte_astm::Field::raw),
        Some(b"25".as_slice())
    );
    assert_eq!(
        view.location().map(analyte_astm::Field::raw),
        Some(b"26".as_slice())
    );
}

fn check_patient_fields_2(view: &analyte_astm::Patient<'_>) {
    assert_eq!(
        view.diagnosis_scheme().map(analyte_astm::Field::raw),
        Some(b"27".as_slice())
    );
    assert_eq!(
        view.diagnosis_codes().map(analyte_astm::Field::raw),
        Some(b"28".as_slice())
    );
    assert_eq!(
        view.religion().map(analyte_astm::Field::raw),
        Some(b"29".as_slice())
    );
    assert_eq!(
        view.marital_status().map(analyte_astm::Field::raw),
        Some(b"30".as_slice())
    );
    assert_eq!(
        view.isolation().map(analyte_astm::Field::raw),
        Some(b"31".as_slice())
    );
    assert_eq!(
        view.language().map(analyte_astm::Field::raw),
        Some(b"32".as_slice())
    );
    assert_eq!(
        view.service().map(analyte_astm::Field::raw),
        Some(b"33".as_slice())
    );
    assert_eq!(
        view.institution().map(analyte_astm::Field::raw),
        Some(b"34".as_slice())
    );
    assert_eq!(
        view.dosage_category().map(analyte_astm::Field::raw),
        Some(b"35".as_slice())
    );
}

fn check_patient(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(1).and_then(analyte_astm::Patient::new) {
        check_patient_fields_0(&view);
        check_patient_fields_1(&view);
        check_patient_fields_2(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_order_fields_0(view: &analyte_astm::Order<'_>) {
    assert_eq!(
        view.specimen_id().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.instrument_specimen_id().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.test_id().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.priority().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.ordered_at().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.collected_at().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.collection_end().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.volume().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.collector().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.action().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.hazard().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
    assert_eq!(
        view.clinical_information().map(analyte_astm::Field::raw),
        Some(b"14".as_slice())
    );
}

fn check_order_fields_1(view: &analyte_astm::Order<'_>) {
    assert_eq!(
        view.received_at().map(analyte_astm::Field::raw),
        Some(b"15".as_slice())
    );
    assert_eq!(
        view.specimen_descriptor().map(analyte_astm::Field::raw),
        Some(b"16".as_slice())
    );
    assert_eq!(
        view.physician().map(analyte_astm::Field::raw),
        Some(b"17".as_slice())
    );
    assert_eq!(
        view.phone().map(analyte_astm::Field::raw),
        Some(b"18".as_slice())
    );
    assert_eq!(
        view.user_1().map(analyte_astm::Field::raw),
        Some(b"19".as_slice())
    );
    assert_eq!(
        view.user_2().map(analyte_astm::Field::raw),
        Some(b"20".as_slice())
    );
    assert_eq!(
        view.laboratory_1().map(analyte_astm::Field::raw),
        Some(b"21".as_slice())
    );
    assert_eq!(
        view.laboratory_2().map(analyte_astm::Field::raw),
        Some(b"22".as_slice())
    );
    assert_eq!(
        view.reported_at().map(analyte_astm::Field::raw),
        Some(b"23".as_slice())
    );
    assert_eq!(
        view.charge().map(analyte_astm::Field::raw),
        Some(b"24".as_slice())
    );
    assert_eq!(
        view.section().map(analyte_astm::Field::raw),
        Some(b"25".as_slice())
    );
    assert_eq!(
        view.report_type().map(analyte_astm::Field::raw),
        Some(b"26".as_slice())
    );
}

fn check_order_fields_2(view: &analyte_astm::Order<'_>) {
    assert_eq!(
        view.reserved().map(analyte_astm::Field::raw),
        Some(b"27".as_slice())
    );
    assert_eq!(
        view.collection_location().map(analyte_astm::Field::raw),
        Some(b"28".as_slice())
    );
    assert_eq!(
        view.infection().map(analyte_astm::Field::raw),
        Some(b"29".as_slice())
    );
    assert_eq!(
        view.service().map(analyte_astm::Field::raw),
        Some(b"30".as_slice())
    );
    assert_eq!(
        view.institution().map(analyte_astm::Field::raw),
        Some(b"31".as_slice())
    );
}

fn check_order(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(2).and_then(analyte_astm::Order::new) {
        check_order_fields_0(&view);
        check_order_fields_1(&view);
        check_order_fields_2(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_result_fields_0(view: &analyte_astm::Result<'_>) {
    assert_eq!(
        view.test_id().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.value().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.units().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.ranges().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.flags().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.abnormality_basis().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.status().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.normative_change().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.operator().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.started_at().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.completed_at().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
    assert_eq!(
        view.instrument().map(analyte_astm::Field::raw),
        Some(b"14".as_slice())
    );
}

fn check_result(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(3).and_then(analyte_astm::Result::new) {
        check_result_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_comment_fields_0(view: &analyte_astm::Comment<'_>) {
    assert_eq!(
        view.source().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.text().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.comment_type().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
}

fn check_comment(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(4).and_then(analyte_astm::Comment::new) {
        check_comment_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_manufacturer_fields_0(view: &analyte_astm::Manufacturer<'_>) {
    assert_eq!(
        view.field(3).map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
}

fn check_manufacturer(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(5).and_then(analyte_astm::Manufacturer::new) {
        check_manufacturer_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_query_fields_0(view: &analyte_astm::Query<'_>) {
    assert_eq!(
        view.range_start().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.range_end().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.test_id().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.time_basis().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.beginning().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.ending().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.physician().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.phone().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.user_1().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.user_2().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.status().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
}

fn check_query(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(6).and_then(analyte_astm::Query::new) {
        check_query_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_scientific_fields_0(view: &analyte_astm::Scientific<'_>) {
    assert_eq!(
        view.method().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
    assert_eq!(
        view.instrument().map(analyte_astm::Field::raw),
        Some(b"4".as_slice())
    );
    assert_eq!(
        view.reagents().map(analyte_astm::Field::raw),
        Some(b"5".as_slice())
    );
    assert_eq!(
        view.units().map(analyte_astm::Field::raw),
        Some(b"6".as_slice())
    );
    assert_eq!(
        view.qc().map(analyte_astm::Field::raw),
        Some(b"7".as_slice())
    );
    assert_eq!(
        view.specimen_descriptor().map(analyte_astm::Field::raw),
        Some(b"8".as_slice())
    );
    assert_eq!(
        view.reserved().map(analyte_astm::Field::raw),
        Some(b"9".as_slice())
    );
    assert_eq!(
        view.container().map(analyte_astm::Field::raw),
        Some(b"10".as_slice())
    );
    assert_eq!(
        view.specimen_id().map(analyte_astm::Field::raw),
        Some(b"11".as_slice())
    );
    assert_eq!(
        view.analyte().map(analyte_astm::Field::raw),
        Some(b"12".as_slice())
    );
    assert_eq!(
        view.result().map(analyte_astm::Field::raw),
        Some(b"13".as_slice())
    );
    assert_eq!(
        view.result_units().map(analyte_astm::Field::raw),
        Some(b"14".as_slice())
    );
}

fn check_scientific_fields_1(view: &analyte_astm::Scientific<'_>) {
    assert_eq!(
        view.collected_at().map(analyte_astm::Field::raw),
        Some(b"15".as_slice())
    );
    assert_eq!(
        view.result_at().map(analyte_astm::Field::raw),
        Some(b"16".as_slice())
    );
    assert_eq!(
        view.preprocessing().map(analyte_astm::Field::raw),
        Some(b"17".as_slice())
    );
    assert_eq!(
        view.diagnosis().map(analyte_astm::Field::raw),
        Some(b"18".as_slice())
    );
    assert_eq!(
        view.birthdate().map(analyte_astm::Field::raw),
        Some(b"19".as_slice())
    );
    assert_eq!(
        view.sex().map(analyte_astm::Field::raw),
        Some(b"20".as_slice())
    );
    assert_eq!(
        view.race().map(analyte_astm::Field::raw),
        Some(b"21".as_slice())
    );
}

fn check_scientific(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(7).and_then(analyte_astm::Scientific::new) {
        check_scientific_fields_0(&view);
        check_scientific_fields_1(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

fn check_terminator_fields_0(view: &analyte_astm::Terminator<'_>) {
    assert_eq!(
        view.code().map(analyte_astm::Field::raw),
        Some(b"3".as_slice())
    );
}

fn check_terminator(records: &[analyte_astm::Record]) -> Result<(), Error> {
    if let Some(view) = records.get(8).and_then(analyte_astm::Terminator::new) {
        check_terminator_fields_0(&view);
    } else {
        return Err(Error::RecordType { record: 0 });
    }
    Ok(())
}

mod scenarios_1;
