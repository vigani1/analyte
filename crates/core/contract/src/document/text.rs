//! Defines version-1 text evidence types and validates their wire values.
use serde::{Deserialize, Serialize};
/// Failure to construct bounded source text.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    /// A required identifier or reason is empty.
    #[error("required text is empty")]
    Empty,
    /// The UTF-8 byte limit was exceeded.
    #[error("text exceeds {limit} bytes")]
    TooLong {
        /// Maximum permitted bytes.
        limit: usize,
    },
    /// Identifiers cannot contain control characters.
    #[error("identifier contains a control character")]
    Control,
}

macro_rules! text_type {
    ($name:ident, $limit:expr, $required:expr, $controls:expr, $doc:literal) => {
        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                crate::schema::text($limit, $required, $controls, $doc)
            }
        }
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            /// Copies text exactly after checking its byte bound and character rules.
            ///
            /// # Errors
            /// Returns the violated text constraint; never trims or normalizes.
            pub fn new(value: &str) -> Result<Self, TextError> {
                if $required && value.is_empty() {
                    return Err(TextError::Empty);
                }
                if value.len() > $limit {
                    return Err(TextError::TooLong { limit: $limit });
                }
                if !$controls && value.chars().any(char::is_control) {
                    return Err(TextError::Control);
                }
                Ok(Self(value.to_owned()))
            }
            /// Returns the exact supplied text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str($doc)
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        $name::new(value).map_err(E::custom)
                    }
                }
                // Validate before the model allocates its owned copy. The format
                // reader must also bound its input document before deserializing.
                d.deserialize_str(Visitor)
            }
        }
        impl TryFrom<String> for $name {
            type Error = TextError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(&value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

text_type!(
    SampleId,
    256,
    true,
    false,
    "A sample ID retaining leading zeros and case; at most 256 UTF-8 bytes."
);

text_type!(
    Identifier,
    256,
    true,
    false,
    "An identifier or assigning system, nonempty and at most 256 UTF-8 bytes."
);

text_type!(
    SourceText,
    16384,
    false,
    true,
    "Unmodified source text, including controls, bounded to 16 KiB."
);

text_type!(
    Reason,
    1024,
    true,
    false,
    "An explicit nonempty reason, bounded to 1024 UTF-8 bytes."
);

impl From<&analyte_model::SampleId> for SampleId {
    fn from(v: &analyte_model::SampleId) -> Self {
        Self(v.as_str().to_owned())
    }
}

impl From<&analyte_model::Identifier> for Identifier {
    fn from(v: &analyte_model::Identifier) -> Self {
        Self(v.as_str().to_owned())
    }
}

impl From<&analyte_model::SourceText> for SourceText {
    fn from(v: &analyte_model::SourceText) -> Self {
        Self(v.as_str().to_owned())
    }
}

impl From<&analyte_model::Reason> for Reason {
    fn from(v: &analyte_model::Reason) -> Self {
        Self(v.as_str().to_owned())
    }
}
