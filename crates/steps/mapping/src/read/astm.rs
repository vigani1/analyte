//! Reads ASTM syntax with installed policy and maps its exact fields.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: crate::Input,
    payload: &[u8],
    syntax: &analyte_config::AstmSyntax,
) -> Result<crate::Output, Error> {
    let messages = analyte_astm::parse_with(payload, syntax.parser_limits(), |bytes| match syntax
        .encoding()
    {
        analyte_config::Encoding::Ascii => bytes
            .is_ascii()
            .then(|| String::from_utf8_lossy(bytes).into_owned()),
        analyte_config::Encoding::Latin1 => analyte_astm::latin1(bytes),
        analyte_config::Encoding::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_owned),
    })?;
    // A transfer may contain multiple messages; the host must retain every one.
    if messages.len() != 1 {
        return Err(Error::Context(super::Context::AstmMessageCount));
    }
    Ok(crate::map_astm(
        messages
            .first()
            .ok_or_else(|| Error::Context(super::Context::AstmMessageMissing))?,
        config,
        input,
    )?)
}
