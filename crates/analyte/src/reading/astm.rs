//! Reads ASTM syntax with installed policy and maps its exact fields.
use super::Error;
pub(super) fn read(
    input: analyte_mapping::Input,
    payload: &[u8],
    syntax: &analyte_config::AstmSyntax,
    mappings: &analyte_config::Mappings,
) -> Result<analyte_mapping::Output, Error> {
    let messages = analyte_astm::parse_with(payload, syntax.parser_limits(), |bytes| match syntax
        .encoding()
    {
        analyte_config::Encoding::Ascii => analyte_astm::ascii(bytes),
        analyte_config::Encoding::Latin1 => analyte_astm::latin1(bytes),
        analyte_config::Encoding::Utf8 => analyte_astm::utf8(bytes),
    })?;
    // A transfer may contain multiple messages; the host must retain every one.
    if messages.len() != 1 {
        return Err(Error::Context(super::Context::AstmMessageCount));
    }
    Ok(analyte_mapping::map_astm(
        messages
            .first()
            .ok_or_else(|| Error::Context(super::Context::AstmMessageMissing))?,
        syntax,
        mappings,
        input,
    )?)
}
