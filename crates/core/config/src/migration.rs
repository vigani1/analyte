//! Migrates machine schema 1 by checking and removing its redundant layer selectors.
use crate::{Error, invalid};

pub(crate) fn normalize(value: &mut toml::Table, path: &str) -> Result<(), Error> {
    let version = value
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    if !matches!(version, Some(1 | 2)) {
        return Err(invalid(
            path,
            "schema_version",
            "use supported machine schema_version = 1 or 2",
        ));
    }
    if version == Some(2) {
        for section in ["framing", "dialog", "syntax"] {
            if value.get(section).and_then(|v| v.get("kind")).is_some() {
                return Err(invalid(
                    path,
                    &format!("{section}.kind"),
                    "schema 2 derives this selector; remove the key",
                ));
            }
        }
        return Ok(());
    }
    let protocol = value
        .get("protocol")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| invalid(path, "protocol", "required protocol key"))?;
    let transport = value
        .get("transport")
        .and_then(|v| v.get("kind"))
        .cloned()
        .ok_or_else(|| invalid(path, "transport.kind", "required transport kind"))?;
    let transport = transport.try_into().map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })?;
    let choice = crate::selection::derive(protocol, transport, path)?;
    let [_, framing, dialog, syntax] = choice.layer_names();
    for (section, expected) in [("framing", framing), ("dialog", dialog), ("syntax", syntax)] {
        let key = format!("{section}.kind");
        let table = value
            .get_mut(section)
            .and_then(toml::Value::as_table_mut)
            .ok_or_else(|| invalid(path, section, "required settings section"))?;
        if table.get("kind").and_then(toml::Value::as_str) != Some(expected) {
            return Err(invalid(
                path,
                &key,
                "legacy selector disagrees with protocol and transport",
            ));
        }
        table.remove("kind");
    }
    value.insert("schema_version".into(), toml::Value::Integer(2));
    Ok(())
}

// Recording headers retain these historical labels as well as schema 1 input.
impl crate::StackChoice {
    pub(crate) const fn layer_names(self) -> [&'static str; 4] {
        match self {
            Self::AstmLis02 => ["astm", "astm_lis01", "astm_link", "astm_lis02"],
            Self::Hl7V2 => ["hl7", "mllp", "hl7_receiver", "hl7_v2"],
            Self::FileAstm => ["astm", "none", "file_pickup", "astm_lis02"],
            Self::FileHl7 => ["hl7", "none", "file_pickup", "hl7_v2"],
            Self::LawManager => ["law", "mllp", "law_manager", "hl7_v2"],
            Self::Poct => ["poct", "poct", "poct", "poct"],
            Self::VidasLiteral => ["vidas", "vidas_literal", "vidas_literal", "vidas_literal"],
            Self::Autolyser => ["autolyser", "autolyser", "autolyser", "autolyser"],
        }
    }
}
