//! Reads retained receipts per protocol and names each reading layer's failures.
//! `machine::protocols` selects the reader from the installed stack.
/// Specific failures from each owned reading layer; callers retain and park the receipt.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A record-only connection kept this receipt as a journaled chunk; it is
    /// never read (ADR 0034).
    #[error("this message was kept by a record-only connection and is never read")]
    RecordedOnly,
    /// LAW result mapping refused retained evidence.
    #[error(transparent)]
    LawMapping(#[from] analyte_mapping::law::Error),
    /// ASTM record syntax was rejected.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    /// HL7 segment syntax was rejected.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW profile requirements were violated.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Failure),
    /// POCT message grammar was rejected.
    #[error(transparent)]
    Poct(#[from] analyte_poct::MessageError),
    /// VIDAS record syntax was rejected.
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// Autolyser record syntax was rejected.
    #[error(transparent)]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    /// ASTM frame integrity or size checks failed.
    #[error(transparent)]
    AstmFrame(#[from] analyte_framing::astm::Error),
    /// Retained ASTM frames could not form a complete message.
    #[error(transparent)]
    AstmReceipt(#[from] analyte_framing::astm::ReceiptError),
    /// MLLP envelope framing failed.
    #[error(transparent)]
    MllpFrame(#[from] analyte_framing::hl7::Error),
    /// Retained MLLP bytes could not form a complete message.
    #[error(transparent)]
    MllpReceipt(#[from] analyte_framing::hl7::ReceiptError),
    /// VIDAS envelope integrity checks failed.
    #[error(transparent)]
    VidasFrame(#[from] analyte_framing::vidas::Error),
    /// Autolyser envelope integrity checks failed.
    #[error(transparent)]
    AutolyserFrame(#[from] analyte_framing::autolyser::Error),
    /// Receipt text is not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Configured field extraction failed.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::Error),
    /// Result identity or value evidence could not be constructed.
    #[error(transparent)]
    Evidence(#[from] analyte_mapping::ResultMappingError),
    /// Required receipt or configuration context is unavailable.
    #[error("{0}")]
    Context(Context),
}

/// Specific missing or ambiguous retained-receipt context.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Context {
    /// ASTM transfer does not contain exactly one message; operator review required.
    #[error("ASTM transfer does not contain exactly one message; operator review required")]
    AstmMessageCount,
    /// ASTM message missing.
    #[error("ASTM message missing")]
    AstmMessageMissing,
}
#[test]
fn context_spellings_are_pinned() {
    assert_eq!(
        Error::Context(Context::AstmMessageCount).to_string(),
        "ASTM transfer does not contain exactly one message; operator review required"
    );
    assert_eq!(
        Error::Context(Context::AstmMessageMissing).to_string(),
        "ASTM message missing"
    );
}

impl Error {
    pub(crate) fn hl7_mapping_error(
        &self,
    ) -> Option<Result<&analyte_hl7::Error, &analyte_mapping::Error>> {
        use Error as Read;
        match self {
            Read::Hl7(error) => Some(Ok(error)),
            Read::Mapping(error) => Some(Err(error)),
            Read::LawMapping(error) => match error {
                analyte_mapping::law::Error::Mapping(error) => Some(Err(error)),
                analyte_mapping::law::Error::MissingHeader
                | analyte_mapping::law::Error::NoticeOrder
                | analyte_mapping::law::Error::NoticePosition => None,
            },
            Read::Astm(_)
            | Read::Law(_)
            | Read::Poct(_)
            | Read::Vidas(_)
            | Read::Autolyser(_)
            | Read::AstmFrame(_)
            | Read::AstmReceipt(_)
            | Read::MllpFrame(_)
            | Read::MllpReceipt(_)
            | Read::VidasFrame(_)
            | Read::AutolyserFrame(_)
            | Read::Encoding(_)
            | Read::Evidence(_)
            | Read::Context(_)
            | Read::RecordedOnly => None,
        }
    }
}

/// Reads ASTM syntax with installed policy and maps its exact fields.
pub(crate) fn astm(
    input: analyte_mapping::Input<'_>,
    payload: &[u8],
    syntax: &analyte_config::AstmSyntax,
    mappings: &analyte_config::Mappings,
) -> Result<analyte_mapping::Output, Error> {
    let messages = analyte_astm::parse_with(
        payload,
        crate::machine::astm::settings::parser_limits(syntax),
        |bytes| match syntax.encoding() {
            analyte_config::Encoding::Ascii => analyte_astm::ascii(bytes),
            analyte_config::Encoding::Latin1 => analyte_astm::latin1(bytes),
            analyte_config::Encoding::Utf8 => analyte_astm::utf8(bytes),
        },
    )?;
    // A transfer may contain multiple messages; the host must retain every one.
    if messages.len() != 1 {
        return Err(Error::Context(Context::AstmMessageCount));
    }
    Ok(analyte_mapping::astm::map(
        &crate::machine::astm::settings::mapping(syntax, mappings),
        messages
            .first()
            .ok_or_else(|| Error::Context(Context::AstmMessageMissing))?,
        input,
    )?)
}

/// Reads HL7 syntax with installed policy and maps its exact fields.
pub(crate) fn hl7(
    config: &analyte_config::Validated,
    input: analyte_mapping::Input<'_>,
    payload: &[u8],
    syntax: &analyte_config::Hl7Syntax,
    mappings: &analyte_config::Mappings,
) -> Result<analyte_mapping::Output, Error> {
    let message =
        analyte_hl7::Message::parse(payload, crate::machine::hl7::settings::options(syntax))?;
    Ok(analyte_mapping::hl7::map(
        &crate::machine::hl7::settings::mapping(
            syntax,
            mappings,
            &config.effective().machine().deviations,
        ),
        &message,
        input,
    )?)
}

/// Reads retained LAW results through the configured HL7 grammar and LAW mapper.
pub(crate) fn law(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::LawStack,
    site: &analyte_config::Site,
) -> Result<analyte_mapping::Output, Error> {
    let message = analyte_hl7::law::parse_message(
        raw,
        crate::machine::law::settings::options(settings, site),
    )?;
    let results = analyte_hl7::law::receive_results(&message)?;
    Ok(analyte_mapping::law::map(settings, &results, input)?)
}

/// Reads POCT observations with durable source Hello context and installed bounds.
pub(crate) fn poct(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::PoctStack,
    hello: Option<&[u8]>,
) -> Result<analyte_mapping::Output, Error> {
    let hello = hello
        .map(|raw| {
            analyte_poct::Message::parse(raw, crate::machine::poct::settings::limits(settings))
        })
        .transpose()?;
    let device = hello.as_ref().and_then(|hello| match hello.body() {
        analyte_poct::Body::Hello { device_id, .. } => Some(device_id.as_str()),
        analyte_poct::Body::Status { .. }
        | analyte_poct::Body::PatientObservations(_)
        | analyte_poct::Body::NonPatientObservations(_)
        | analyte_poct::Body::EndTopic(_)
        | analyte_poct::Body::Terminate(_)
        | analyte_poct::Body::Acknowledgement { .. }
        | analyte_poct::Body::Escape { .. }
        | analyte_poct::Body::KeepAlive
        | analyte_poct::Body::Request(_) => None,
    });
    let message =
        analyte_poct::Message::parse(raw, crate::machine::poct::settings::limits(settings))?;
    Ok(analyte_mapping::poct::map(
        settings,
        &message,
        analyte_mapping::Input { device, ..input },
    )?)
}

/// Reads VIDAS packets using the installed framing and sample identity policy.
pub(crate) fn vidas(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::VidasStack,
) -> Result<analyte_mapping::Output, Error> {
    let packet =
        analyte_framing::vidas::decode(raw, crate::machine::vidas::settings::framing(settings)?)?;
    let limit = crate::machine::vidas::settings::sample_limit(settings);
    let message = analyte_vidas::syntax::Message::parse(std::str::from_utf8(&packet.body)?, limit)?;
    Ok(analyte_mapping::vidas::map(settings, &message, input)?)
}

/// Verifies Autolyser receipt checksums before syntax and mapping.
pub(crate) fn autolyser(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::AutolyserStack,
) -> Result<analyte_mapping::Output, Error> {
    let body = analyte_framing::autolyser::decode(raw)?;
    let report = analyte_autolyser::syntax::Report::parse(body, settings.policy().controls)?;
    Ok(analyte_mapping::autolyser::map(settings, &report, input)?)
}
