//! Supplies synthetic messages test fixtures and scenarios.
use analyte_poct::Limits;

pub(crate) fn limits() -> Limits {
    Limits {
        message_bytes: 65536,
        depth: 16,
        elements: 1024,
        attribute_bytes: 2048,
    }
}

pub(crate) fn field(name: &str, value: &str) -> String {
    format!("<{name} V=\"{}\"/>", quick_xml::escape::escape(value))
}

pub(crate) fn message(kind: &str, id: &str, body: &str) -> String {
    format!(
        "<{kind}><HDR>{}{}{}</HDR>{body}</{kind}>",
        field("HDR.control_id", id),
        field("HDR.version_id", "POCT01"),
        field("HDR.creation_dttm", "2026-03-12T09:41:02.120+01:00")
    )
}

pub(crate) fn hello(id: &str) -> String {
    message(
        "HEL.R01",
        id,
        &format!(
            "<DEV>{}<DCP>{}</DCP><DSC>{}{}<DSC.max_message_sz NULL=\"PINF\"/></DSC></DEV>",
            field("DEV.device_id", "A1-B2-C3-D4-E5-F6-07-08"),
            field("DCP.application_timeout", "12.5"),
            field("DSC.connection_profile_cd", "SA"),
            field("DSC.topics_supported_cd", "D_EV")
        ),
    )
}

pub(crate) fn status(id: &str, count: &str) -> String {
    message(
        "DST.R01",
        id,
        &format!(
            "<DST>{}{}{}</DST>",
            field("DST.status_dttm", "2026-03-12T09:40:00+01:00"),
            field("DST.new_observations_qty", count),
            field("DST.condition_cd", "R")
        ),
    )
}

pub(crate) fn observation(id: &str, patient: bool, value: &str) -> String {
    let (kind, subject, role) = if patient {
        (
            "OBS.R01",
            format!("<PT>{}", field("PT.patient_id", "000xyz")),
            "OBS",
        )
    } else {
        (
            "OBS.R02",
            format!("<CTC>{}", field("CTC.name", "synthetic-control")),
            "LQC",
        )
    };
    message(
        kind,
        id,
        &format!(
            "<SVC>{}{}{}{subject}<OBS><OBS.observation_id V=\"synthetic-analyte\" SN=\"synthetic-system\"><TRANSLTN V=\"alternate\" SN=\"alternate-system\"/></OBS.observation_id>{}{}{}</OBS></{}></SVC>",
            field("SVC.role_cd", role),
            field("SVC.observation_dttm", "2026-03-12T09:35:00+01:00"),
            field("SVC.sequence_nbr", "0007"),
            value,
            field("OBS.method_cd", "M"),
            field("OBS.interpretation_cd", "<"),
            if patient { "PT" } else { "CTC" }
        ),
    )
}
