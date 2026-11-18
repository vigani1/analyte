//! Embeds the published contract v1 vectors the checker runs, so the program is self-contained.

macro_rules! vector {
    ($name:ident, $file:literal) => {
        #[doc = concat!("`docs/contract/vectors/", $file, "`.")]
        pub(crate) const $name: &[u8] =
            include_bytes!(concat!("../../../../docs/contract/vectors/", $file));
    };
}

vector!(DELIVERY, "delivery.json");
vector!(EVENT, "event.json");
vector!(INVALID_VERSION, "invalid-version.json");
vector!(INVALID_SIGNATURE, "invalid-signature.json");
vector!(INVALID_DOCUMENTS, "invalid-documents.json");
vector!(UNKNOWN_FIELDS, "unknown-fields.json");
vector!(SOURCE_PATH_OCCURRENCE, "source-path-occurrence.json");
vector!(AUTHENTICATION, "authentication.json");
vector!(ROTATED_SIGNATURE, "rotated-signature.json");
vector!(IDEMPOTENCY, "idempotency.json");
vector!(LIMITS, "limits.json");
