//! Reviewer message builders. XML is escaped and checked before it can be sent.
use crate::{ControlId, Header, Limits, Message, MessageError};

/// Application error classification from Appendix B section 5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationError {
    /// Objects are missing or in the wrong sequence.
    ObjectSequence,
    /// A required field is absent.
    RequiredField,
    /// A field has an invalid type.
    DataType,
    /// A code is absent from its table.
    TableValue,
    /// A field value is unsupported.
    UnsupportedValue,
    /// The message version is unsupported.
    UnsupportedVersion,
    /// An internal processing failure occurred.
    Internal,
}

impl ApplicationError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ObjectSequence => "100",
            Self::RequiredField => "101",
            Self::DataType => "102",
            Self::TableValue => "103",
            Self::UnsupportedValue => "200",
            Self::UnsupportedVersion => "201",
            Self::Internal => "202",
        }
    }
}

/// Protocol-level reason to stop a topic, distinct from an application error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeReason {
    /// A reason not represented by another standard code.
    Other,
    /// Topic is unsupported.
    UnsupportedTopic,
    /// Topic cannot be completed now, for example because storage failed.
    CannotComplete,
}

impl EscapeReason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Other => "OTH",
            Self::UnsupportedTopic => "TOP",
            Self::CannotComplete => "CNC",
        }
    }
}

/// Why the reviewer is terminating a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    /// All selected topics are complete.
    Normal,
    /// Protocol, storage or timeout failure.
    Abnormal,
    /// Explicit operator request.
    User,
    /// No source reason is available.
    Unknown,
}

impl TerminationReason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Normal => "NRM",
            Self::Abnormal => "ABN",
            Self::User => "USR",
            Self::Unknown => "UNK",
        }
    }
}

/// The reviewer's mandatory Basic Profile sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outbound {
    /// Request the device's new observations.
    RequestObservations,
    /// End the conversation; the peer must acknowledge this message.
    Terminate(TerminationReason),
    /// Accept custody of a referenced message.
    Accept(ControlId),
    /// Report an application error without ending the current topic.
    ApplicationError {
        /// Message whose processing failed.
        reference: ControlId,
        /// Typed error classification.
        error: ApplicationError,
    },
    /// Stop a topic for a protocol-level reason.
    Escape {
        /// Message associated with the protocol failure.
        reference: ControlId,
        /// Topic-level reason.
        reason: EscapeReason,
    },
}

/// Builds a reviewer message and checks it with the inbound structural validator.
/// The caller assigns one control ID per send, and supplies a source clock value.
/// # Errors
/// Rejects missing UTC offset, XML-invalid values, structural errors and output
/// exceeding configured limits. No unchecked byte payload can be returned.
pub fn build(header: &Header, body: &Outbound, limits: Limits) -> Result<Message, MessageError> {
    if header.created.offset_seconds().is_none() {
        return Err(MessageError::Data(crate::DataError::Time));
    }
    let (root, object, fields) = fields(body);
    let mut all = vec![
        ("HDR.control_id", header.control_id.as_str()),
        ("HDR.version_id", "POCT01"),
        ("HDR.creation_dttm", header.created.original()),
    ];
    all.extend(fields.iter().copied());
    // Count the exact escaped size before allocating. This keeps even
    // caller-supplied timestamps inside the same message allocation boundary.
    let mut length = root
        .len()
        .checked_mul(2)
        .and_then(|n| object.len().checked_mul(2).and_then(|o| n.checked_add(o)))
        .and_then(|n| n.checked_add(21))
        .ok_or(crate::Error::MessageLimit)?;
    for (name, value) in &all {
        length = length
            .checked_add(name.len())
            .and_then(|n| {
                escaped_length(value)
                    .and_then(|v| n.checked_add(v))
                    .and_then(|n| n.checked_add(8))
            })
            .ok_or(crate::Error::MessageLimit)?;
    }
    if length > limits.message_bytes {
        return Err(crate::Error::MessageLimit.into());
    }
    let mut xml = format!("<{root}><HDR>");
    for (name, value) in all.iter().take(3) {
        append_field(&mut xml, name, value);
    }
    xml.push_str(&format!("</HDR><{object}>"));
    for (name, value) in fields {
        append_field(&mut xml, name, value);
    }
    xml.push_str(&format!("</{object}></{root}>"));
    Message::parse(xml.as_bytes(), limits)
}

fn append_field(xml: &mut String, name: &str, value: &str) {
    xml.push_str(&format!(
        "<{name} V=\"{}\"/>",
        quick_xml::escape::escape(value)
    ));
}

type Fields<'a> = (&'static str, &'static str, Vec<(&'static str, &'a str)>);

fn fields(body: &Outbound) -> Fields<'_> {
    match body {
        Outbound::RequestObservations => ("REQ.R01", "REQ", vec![("REQ.request_cd", "ROBS")]),
        Outbound::Terminate(reason) => ("END.R01", "TRM", vec![("TRM.reason_cd", reason.code())]),
        Outbound::Accept(reference) => (
            "ACK.R01",
            "ACK",
            vec![
                ("ACK.type_cd", "AA"),
                ("ACK.ack_control_id", reference.as_str()),
            ],
        ),
        Outbound::ApplicationError { reference, error } => (
            "ACK.R01",
            "ACK",
            vec![
                ("ACK.type_cd", "AE"),
                ("ACK.ack_control_id", reference.as_str()),
                ("ACK.error_detail_cd", error.code()),
            ],
        ),
        Outbound::Escape { reference, reason } => (
            "ESC.R01",
            "ESC",
            vec![
                ("ESC.esc_control_id", reference.as_str()),
                ("ESC.detail_cd", reason.code()),
            ],
        ),
    }
}

fn escaped_length(value: &str) -> Option<usize> {
    value.chars().try_fold(0usize, |n, c| {
        n.checked_add(match c {
            '&' => 5,
            '<' | '>' => 4,
            '\'' | '"' => 6,
            _ => c.len_utf8(),
        })
    })
}
