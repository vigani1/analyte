//! Parses the required site retention period without a fallback policy.
use serde::{Deserialize, Serialize};

/// Validated retention policy (ADR 0013 item 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Value", into = "Value")]
pub enum Retention {
    /// Never purge retained evidence.
    Keep,
    /// Purge only eligible evidence after this validated number of days.
    Days(Days),
}

/// A retention period between 1 and 3650 days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Days(u16);
impl Days {
    /// Returns the validated number of days.
    #[must_use]
    pub fn get(self) -> u16 {
        self.0
    }
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Value {
    Days(u16),
    Text(String),
}
impl TryFrom<Value> for Retention {
    type Error = &'static str;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Days(days @ 1..=3650) => Ok(Self::Days(Days(days))),
            Value::Text(text) if text == "keep" => Ok(Self::Keep),
            Value::Days(_) | Value::Text(_) => Err("retention must be 1 to 3650 days or keep"),
        }
    }
}
impl From<Retention> for Value {
    fn from(value: Retention) -> Self {
        match value {
            Retention::Keep => Self::Text("keep".into()),
            Retention::Days(days) => Self::Days(days.get()),
        }
    }
}
pub(crate) fn load_site(path: &str, text: &str) -> Result<crate::Site, crate::Error> {
    if text.len() <= crate::MAX_FILE_BYTES {
        let table: toml::Table = toml::from_str(text).map_err(|source| crate::Error::Schema {
            path: path.into(),
            source,
        })?;
        if !table.contains_key("retention") {
            return Err(crate::invalid(
                path,
                "retention",
                "required site retention is missing; choose 1 to 3650 days or keep",
            ));
        }
    }
    crate::load(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0013_item1_required_period_has_no_default_and_checks_bounds() -> Result<(), crate::Error>
    {
        let site = include_str!("../examples/site.toml");
        for period in ["1", "90", "3650", "\"keep\""] {
            load_site("site", &site.replace("\"keep\"", period))?;
        }
        for period in ["0", "3651", "-1", "\"forever\""] {
            {
                let Err(refusal) = load_site("site", &site.replace("\"keep\"", period)) else {
                    panic!("input must be refused")
                };
                assert!(
                    matches!(&refusal, crate::Error::Schema { .. }),
                    "unexpected refusal: {refusal:?}"
                );
            }
        }
        let missing = site.replace("retention = \"keep\"", "");
        assert!(
            matches!(load_site("site", &missing), Err(crate::Error::Invalid { key, .. }) if key == "retention")
        );
        Ok(())
    }
}
