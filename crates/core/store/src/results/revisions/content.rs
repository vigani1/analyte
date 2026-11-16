//! Hashes clinical content for immutable revision comparisons.
use super::*;
pub(super) fn clinical_digest(result: &ResultContent) -> Result<analyte_model::Digest, Error> {
    let mut parts = serde_json::to_value(&result.parts)?;
    if let Some(parts) = parts.as_array_mut() {
        for part in parts {
            if let Some(fields) = part.as_object_mut() {
                fields.remove("received_at");
                // Source offsets locate immutable evidence, not clinical content.
                // An optional resend field can shift every later XML/record offset.
                // Keep source kinds, sequences and statuses in the comparison.
                if let Some(path) = fields
                    .get_mut("source_path")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for node in path {
                        if let Some(node) = node.as_object_mut() {
                            node.remove("occurrence");
                        }
                    }
                }
            }
        }
    }
    // JSON objects use sorted keys. These are typed structures, not arbitrary source JSON.
    Ok(digest(&serde_json::to_vec(&(
        &result.sample,
        result.processing,
        parts,
    ))?))
}
