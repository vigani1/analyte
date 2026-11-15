//! Structural rules derived from Appendix B sections 6, 8 and 12.
//! Rules are executable checks, not copied schema documents.
use crate::{
    Document, Element,
    data::{self, Kind, Value},
    message::MessageError,
};

pub(crate) struct Rule {
    name: &'static str,
    minimum: usize,
    many: bool,
}

const fn one(name: &'static str) -> Rule {
    Rule {
        name,
        minimum: 1,
        many: false,
    }
}
const fn optional(name: &'static str) -> Rule {
    Rule {
        name,
        minimum: 0,
        many: false,
    }
}
const fn repeated(name: &'static str) -> Rule {
    Rule {
        name,
        minimum: 0,
        many: true,
    }
}
const fn several(name: &'static str) -> Rule {
    Rule {
        name,
        minimum: 1,
        many: true,
    }
}

pub(crate) fn children(
    document: &Document,
    parent: usize,
) -> impl Iterator<Item = (usize, &Element)> {
    document
        .elements()
        .iter()
        .enumerate()
        .filter(move |(_, e)| e.parent() == Some(parent))
}

pub(crate) fn child(
    document: &Document,
    parent: usize,
    name: &'static str,
) -> Result<usize, MessageError> {
    children(document, parent)
        .find(|(_, e)| e.name() == name)
        .map(|(i, _)| i)
        .ok_or(MessageError::Missing(name))
}

pub(crate) fn literal(
    document: &Document,
    parent: usize,
    name: &'static str,
) -> Result<String, MessageError> {
    let i = child(document, parent, name)?;
    data::attr(
        document.elements().get(i).ok_or(MessageError::Structure)?,
        "V",
    )
    .map(str::to_owned)
    .ok_or(MessageError::MissingValue(name))
}

pub(crate) fn validate(document: &Document) -> Result<Vec<Option<Value>>, MessageError> {
    let root = document
        .elements()
        .first()
        .ok_or(MessageError::Structure)?
        .name();
    let patient = root == "OBS.R01";
    let mut values = Vec::with_capacity(document.elements().len());
    for (index, element) in document.elements().iter().enumerate() {
        if let Some(rules) = object_rules(element.name(), patient) {
            if !element.attributes().is_empty() || !element.text().trim().is_empty() {
                return Err(MessageError::Structure);
            }
            validate_children(document, index, &rules)?;
            values.push(None);
        } else {
            let kind = field_kind(element.name()).ok_or(MessageError::UnsupportedElement)?;
            validate_field_children(
                document,
                index,
                kind,
                element.name() == "OBS.observation_id"
                    || element.name() == "ORD.universal_service_id"
                    || matches!(
                        element.name(),
                        "SPC.source_cd" | "SPC.type_cd" | "NOTE" | "TRANSLTN"
                    ),
            )?;
            if data::attr(element, "ORIGTXT").is_some()
                && children(document, index).any(|(_, e)| e.name() == "ORIGTXT")
            {
                return Err(MessageError::Structure);
            }
            let value = data::parse(element, kind).map_err(MessageError::Data)?;
            validate_codes(element)?;
            values.push(Some(value));
        }
    }
    // The prose tightens the DTD: exactly one result representation per OBS.
    for (i, e) in document
        .elements()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name() == "OBS")
    {
        let _ = e;
        let count = children(document, i)
            .filter(|(_, e)| matches!(e.name(), "OBS.value" | "OBS.qualitative_value"))
            .count();
        if count != 1 {
            return Err(MessageError::ObservationValue);
        }
    }
    Ok(values)
}

fn validate_children(
    document: &Document,
    parent: usize,
    rules: &[Rule],
) -> Result<(), MessageError> {
    let actual: Vec<&str> = children(document, parent).map(|(_, e)| e.name()).collect();
    let mut position = 0usize;
    for rule in rules {
        let mut count = 0usize;
        while actual.get(position).is_some_and(|name| *name == rule.name) {
            count = count.checked_add(1).ok_or(MessageError::Structure)?;
            position = position.checked_add(1).ok_or(MessageError::Structure)?;
            if !rule.many {
                break;
            }
        }
        if count < rule.minimum {
            return Err(MessageError::Missing(rule.name));
        }
    }
    if position != actual.len() {
        return Err(MessageError::Structure);
    }
    Ok(())
}

fn object_rules(name: &str, patient: bool) -> Option<Vec<Rule>> {
    Some(match name {
        "HEL.R01" => vec![one("HDR"), one("DEV"), optional("AP")],
        "DST.R01" => vec![one("HDR"), one("DST")],
        "OBS.R01" | "OBS.R02" => vec![one("HDR"), several("SVC")],
        "EOT.R01" => vec![one("HDR"), one("EOT")],
        "END.R01" => vec![one("HDR"), one("TRM")],
        "ACK.R01" => vec![one("HDR"), one("ACK")],
        "ESC.R01" => vec![one("HDR"), one("ESC")],
        "REQ.R01" => vec![one("HDR"), one("REQ")],
        "HDR" => vec![
            optional("HDR.message_type"),
            one("HDR.control_id"),
            one("HDR.version_id"),
            one("HDR.creation_dttm"),
            optional("HDR.encoding_chars"),
        ],
        "DEV" => vec![
            one("DEV.device_id"),
            optional("DEV.vendor_id"),
            optional("DEV.model_id"),
            optional("DEV.serial_id"),
            optional("DEV.manufacturer_name"),
            optional("DEV.hw_version"),
            optional("DEV.sw_version"),
            optional("DEV.device_name"),
            optional("DEV.vmd_name"),
            optional("DEV.vmd_id"),
            optional("DCP"),
            optional("DSC"),
        ],
        "DCP" => vec![
            one("DCP.application_timeout"),
            optional("DCP.vendor_specific"),
        ],
        "DSC" => vec![
            one("DSC.connection_profile_cd"),
            repeated("DSC.topics_supported_cd"),
            repeated("DSC.directives_supported_cd"),
            one("DSC.max_message_sz"),
        ],
        "AP" => vec![one("AP.ap_id"), one("AP.port_nbr")],
        "DST" => vec![
            one("DST.status_dttm"),
            one("DST.new_observations_qty"),
            optional("DST.new_events_qty"),
            one("DST.condition_cd"),
            optional("DST.observations_update_dttm"),
            optional("DST.events_update_dttm"),
            optional("DST.operators_update_dttm"),
            optional("DST.patients_update_dttm"),
        ],
        "SVC" if patient => vec![
            one("SVC.role_cd"),
            one("SVC.observation_dttm"),
            optional("SVC.status_cd"),
            optional("SVC.reason_cd"),
            optional("SVC.sequence_nbr"),
            one("PT"),
            optional("OPR"),
            optional("ORD"),
            optional("SPC"),
            repeated("RGT"),
            repeated("NTE"),
        ],
        "SVC" => vec![
            one("SVC.role_cd"),
            one("SVC.observation_dttm"),
            optional("SVC.status_cd"),
            optional("SVC.reason_cd"),
            optional("SVC.sequence_nbr"),
            one("CTC"),
            optional("OPR"),
            repeated("RGT"),
            repeated("NTE"),
        ],
        "PT" => vec![
            one("PT.patient_id"),
            optional("PT.location"),
            optional("PT.name"),
            optional("PT.birth_date"),
            optional("PT.gender_cd"),
            optional("PT.weight"),
            optional("PT.height"),
            several("OBS"),
        ],
        "CTC" => vec![
            one("CTC.name"),
            optional("CTC.lot_number"),
            optional("CTC.expiration_date"),
            optional("CTC.level_cd"),
            optional("CTC.cal-ver_repetition"),
            several("OBS"),
        ],
        "OBS" => vec![
            one("OBS.observation_id"),
            optional("OBS.value"),
            optional("OBS.qualitative_value"),
            one("OBS.method_cd"),
            optional("OBS.status_cd"),
            optional("OBS.interpretation_cd"),
            optional("OBS.normal_lo-hi_limit"),
            optional("OBS.critical_lo-hi_limit"),
            repeated("NTE"),
        ],
        "OPR" => vec![one("OPR.operator_id"), optional("OPR.name")],
        "ORD" => vec![
            one("ORD.universal_service_id"),
            optional("ORD.ordering_provider_id"),
            optional("ORD.order_id"),
        ],
        "SPC" => vec![
            optional("SPC.specimen_id"),
            one("SPC.specimen_dttm"),
            optional("SPC.source_cd"),
            optional("SPC.type_cd"),
        ],
        "RGT" => vec![
            one("RGT.name"),
            one("RGT.lot_number"),
            one("RGT.expiration_date"),
        ],
        "NTE" => vec![one("NTE.text")],
        "EOT" => vec![one("EOT.topic_cd"), optional("EOT.update_dttm")],
        "TRM" => vec![one("TRM.reason_cd"), optional("TRM.note_txt")],
        "ACK" => vec![
            one("ACK.type_cd"),
            one("ACK.ack_control_id"),
            optional("ACK.note_txt"),
            optional("ACK.error_detail_cd"),
        ],
        "ESC" => vec![
            one("ESC.esc_control_id"),
            one("ESC.detail_cd"),
            optional("ESC.note_txt"),
        ],
        "REQ" => vec![one("REQ.request_cd")],
        _ => return None,
    })
}

fn field_kind(name: &str) -> Option<Kind> {
    Some(match name {
        "DCP.vendor_specific" | "THUMBNAIL" | "ORIGTXT" => Kind::Encapsulated,
        "PT.name" | "OPR.name" => Kind::Person,
        "OBS.observation_id"
        | "OBS.qualitative_value"
        | "ORD.universal_service_id"
        | "ORD.order_id"
        | "SPC.specimen_id"
        | "SPC.source_cd"
        | "SPC.type_cd"
        | "CTC.lot_number"
        | "CTC.level_cd"
        | "DSC.topics_supported_cd"
        | "DSC.directives_supported_cd"
        | "DST.condition_cd"
        | "HDR.message_type"
        | "EOT.topic_cd"
        | "TRM.reason_cd"
        | "ACK.error_detail_cd"
        | "REQ.request_cd"
        | "TRANSLTN"
        | "NOTE"
        | "CONFID"
        | "RGT.lot_number" => Kind::Code,
        "DSC.connection_profile_cd"
        | "SVC.role_cd"
        | "SVC.status_cd"
        | "SVC.reason_cd"
        | "PT.gender_cd"
        | "OBS.method_cd"
        | "OBS.status_cd"
        | "OBS.interpretation_cd"
        | "ACK.type_cd"
        | "ESC.detail_cd" => Kind::SimpleCode,
        "OBS.value" | "PT.weight" | "PT.height" => Kind::Quantity,
        "OBS.normal_lo-hi_limit" | "OBS.critical_lo-hi_limit" => Kind::Interval,
        "HDR.creation_dttm"
        | "DST.status_dttm"
        | "DST.observations_update_dttm"
        | "DST.events_update_dttm"
        | "DST.operators_update_dttm"
        | "DST.patients_update_dttm"
        | "SVC.observation_dttm"
        | "PT.birth_date"
        | "SPC.specimen_dttm"
        | "CTC.expiration_date"
        | "RGT.expiration_date"
        | "EOT.update_dttm" => Kind::Time,
        "DCP.application_timeout" => Kind::Real,
        "DSC.max_message_sz"
        | "DST.new_observations_qty"
        | "DST.new_events_qty"
        | "AP.port_nbr"
        | "SVC.sequence_nbr"
        | "CTC.cal-ver_repetition" => Kind::Integer,
        "HDR.control_id"
        | "HDR.version_id"
        | "HDR.encoding_chars"
        | "DEV.device_id"
        | "DEV.vendor_id"
        | "DEV.model_id"
        | "DEV.serial_id"
        | "DEV.manufacturer_name"
        | "DEV.hw_version"
        | "DEV.sw_version"
        | "DEV.device_name"
        | "DEV.vmd_name"
        | "DEV.vmd_id"
        | "AP.ap_id"
        | "PT.patient_id"
        | "PT.location"
        | "CTC.name"
        | "OPR.operator_id"
        | "ORD.ordering_provider_id"
        | "RGT.name"
        | "NTE.text"
        | "TRM.note_txt"
        | "ACK.ack_control_id"
        | "ACK.note_txt"
        | "ESC.esc_control_id"
        | "ESC.note_txt"
        | "GIV"
        | "MID"
        | "FAM"
        | "PFX"
        | "SFX"
        | "DEL"
        | "REF" => Kind::Text,
        _ => return None,
    })
}

fn validate_field_children(
    document: &Document,
    parent: usize,
    kind: Kind,
    equivalents: bool,
) -> Result<(), MessageError> {
    let mut last = 0usize;
    let mut singleton = std::collections::BTreeSet::new();
    for (_, element) in children(document, parent) {
        if kind == Kind::Encapsulated {
            if matches!(element.name(), "REF" | "THUMBNAIL" | "NOTE" | "CONFID") {
                continue;
            }
            return Err(MessageError::Structure);
        }
        let (rank, many) = match element.name() {
            "ORIGTXT" if kind == Kind::Code => (0, false),
            "TRANSLTN" if equivalents => (1, true),
            "GIV" | "MID" | "FAM" | "PFX" | "SFX" | "DEL" if kind == Kind::Person => (0, true),
            "NOTE" => (2, kind == Kind::Text),
            "CONFID" => (3, kind == Kind::Text),
            _ => return Err(MessageError::Structure),
        };
        if (kind != Kind::Text && rank < last) || (!many && !singleton.insert(rank)) {
            return Err(MessageError::Structure);
        }
        last = rank;
    }
    if !matches!(kind, Kind::Text | Kind::Encapsulated)
        && !document
            .elements()
            .get(parent)
            .ok_or(MessageError::Structure)?
            .text()
            .trim()
            .is_empty()
    {
        return Err(MessageError::Structure);
    }
    Ok(())
}

fn validate_codes(element: &Element) -> Result<(), MessageError> {
    let Some(value) = data::attr(element, "V") else {
        return Ok(());
    };
    let legal = match element.name() {
        "ACK.type_cd" => matches!(value, "AA" | "AE"),
        "ESC.detail_cd" => matches!(value, "OTH" | "TOP" | "CNC"),
        "DSC.connection_profile_cd" => matches!(value, "SA" | "AA" | "CS"),
        "SVC.role_cd" => matches!(value, "UNK" | "OBS" | "LQC" | "EQC" | "CVR" | "CAL" | "PRF"),
        "SVC.status_cd" => matches!(value, "NRM" | "OVR" | "UNK"),
        "SVC.reason_cd" => matches!(value, "NEW" | "RES" | "EDT"),
        "OBS.method_cd" => matches!(value, "C" | "D" | "E" | "I" | "M" | "U"),
        "OBS.status_cd" => matches!(value, "A" | "D" | "U" | "X"),
        "PT.gender_cd" => matches!(value, "F" | "M" | "O" | "U" | "A" | "N"),
        "OBS.interpretation_cd" => matches!(
            value,
            "L" | "H" | "LL" | "HH" | "<" | ">" | "N" | "A" | "AA" | "U" | "D" | "B" | "W" | "null"
        ),
        _ => true,
    };
    if !legal {
        return Err(MessageError::Data(data::DataError::Code));
    }
    Ok(())
}
