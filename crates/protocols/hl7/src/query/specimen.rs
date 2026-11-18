//! Reads configured specimen selectors without expanding query scope (HL7 2.3.1 section 2.18).
use super::Request;
/// Agreed query field and exact qualifiers; every unbound qualifier must be empty.
pub struct SpecimenSelector<'a> {
    /// QRD field containing the specimen selector.
    pub field: u16,
    /// Exact suffix to remove before decoding the scalar identifier.
    pub suffix: &'a [u8],
    /// Exact raw field qualifiers agreed with the peer.
    pub filters: &'a [(u16, &'a [u8])],
}
/// Returns a scalar specimen only when all query qualifiers match the agreement.
///
/// # Errors
/// Returns the original text decoding error; never substitutes a specimen.
pub fn specimen(
    request: Request<'_>,
    layout: &SpecimenSelector<'_>,
) -> Result<Option<String>, crate::Error> {
    if request.filter().is_some() {
        return Ok(None);
    }
    let definition = request.definition();
    for number in 5..=12 {
        if number == 7 || number == usize::from(layout.field) {
            continue;
        }
        let value = definition.field(number).map_or(b"".as_slice(), |f| f.raw());
        if let Some(expected) = layout
            .filters
            .iter()
            .find(|filter| usize::from(filter.0) == number)
        {
            if value != expected.1 {
                return Ok(None);
            }
        } else if !value.is_empty() {
            return Ok(None);
        }
    }
    let Some(field) = definition.field(usize::from(layout.field)) else {
        return Ok(None);
    };
    let Some(raw) = field.raw().strip_suffix(layout.suffix) else {
        return Ok(None);
    };
    let [_, component, repetition, _, subcomponent] = request.message().delimiters().bytes();
    if raw
        .iter()
        .any(|b| [component, repetition, subcomponent].contains(b))
    {
        return Ok(None);
    }
    let text = crate::decode_text(raw, request.message())?;
    if text == "ALL" || text.is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    // HL7 2.3.1 §2.18, QRD-8, §2.7
    #[test]
    fn qrd8_scalar_scope_and_escapes() -> Result<(), crate::Error> {
        let filters = [(9, b"ORD".as_slice()), (10, b"TEST".as_slice())];
        let selector = SpecimenSelector {
            field: 8,
            suffix: b"^^^^^^^LCL",
            filters: &filters,
        };
        for (value, expected) in [
            ("0001", Some("0001")),
            ("a\\F\\b", Some("a|b")),
            ("ALL", None),
            ("", None),
            ("a~b", None),
            ("a^b", None),
        ] {
            let raw = format!(
                "MSH|^~\\&|A|B|C|D|202611150900||QRY^Q02^QRY_Q02|q1|P|2.3.1\rQRD|20261115|D|D|tag|||5^LI|{value}^^^^^^^LCL|ORD|TEST\r"
            );
            let message = crate::Message::parse(raw.as_bytes(), crate::Options::default())?;
            let request = match Request::new(&message) {
                Ok(request) => request,
                Err(error) => {
                    assert!(expected.is_none(), "unexpected query refusal: {error}");
                    continue;
                }
            };
            assert_eq!(specimen(request, &selector)?.as_deref(), expected);
            assert!(
                specimen(
                    request,
                    &SpecimenSelector {
                        filters: &[],
                        ..selector
                    }
                )?
                .is_none()
            );
        }
        Ok(())
    }
}
