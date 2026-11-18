//! Preserves capacity, encoding, filesystem and SQLite failures shared by store boundaries.
/// Failures of bounded persistence primitives, independent of application operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The worker could not establish completion of the persistence operation.
    #[error(transparent)]
    Worker(#[from] crate::WorkerError),
    /// The host reserve or database size limit refused admission.
    #[error("storage capacity reserve reached")]
    Capacity,
    /// A configured record bound refused admission.
    #[error("storage input exceeds {what}")]
    LimitExceeded {
        /// Name of the configured bound.
        what: &'static str,
    },
    /// SQLite refused a lock; this execution did not commit.
    #[error("SQLite is busy: {0}")]
    Busy(#[source] rusqlite::Error),
    /// SQLite detected damaged evidence; its diagnostic cause is retained.
    #[error("stored evidence is corrupt: {0}")]
    Corruption(#[source] rusqlite::Error),
    /// SQLite refused a constraint rather than an application identity rule.
    #[error("SQLite constraint violation: {0}")]
    ConstraintViolation(#[source] rusqlite::Error),
    /// SQLite reported a full disk.
    #[error("disk full")]
    DiskFull(#[source] rusqlite::Error),
    /// Other SQLite failure with its original cause.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[source] rusqlite::Error),
    /// The filesystem probe failed; admission is refused.
    #[error("filesystem operation failed")]
    Io(#[from] std::io::Error),
    /// Protected evidence could not be encoded or decoded. The message names
    /// only the failure category: serde_json's own text can quote stored
    /// values, which must stay out of ordinary logs (rule 10).
    #[error("evidence encoding failed: {:?} error", .0.classify())]
    Json(#[from] serde_json::Error),
    /// Protected evidence could not fit its validated model type.
    #[error("invalid bounded storage text: {0}")]
    Text(#[source] analyte_model::TextError),
}
impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "External SQLite codes retain their cause when no recovery classification is known"
            )]
            match code.code {
                rusqlite::ErrorCode::DiskFull => return Self::DiskFull(error),
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    return Self::Corruption(error);
                }
                rusqlite::ErrorCode::ConstraintViolation => {
                    return Self::ConstraintViolation(error);
                }
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    return Self::Busy(error);
                }
                _ => {}
            }
        }
        Self::Sqlite(error)
    }
}

#[cfg(test)]
mod tests {
    // AGENTS rule 10: no patient data in ordinary logs.
    #[test]
    fn json_error_text_carries_no_stored_value() -> Result<(), &'static str> {
        let cause = serde_json::from_str::<u64>(r#""SAMPLE-0042""#)
            .err()
            .ok_or("a string is not a number")?;
        let text = super::StorageError::from(cause).to_string();
        assert!(!text.contains("SAMPLE-0042"));
        assert_eq!(text, "evidence encoding failed: Data error");
        Ok(())
    }
}
