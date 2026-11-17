//! Defines shared persistence recovery cases and cause-preserving error conversions.
macro_rules! storage_boundary {
    ($(#[$meta:meta])* pub enum $name:ident { $($variants:tt)* }) => {
        $(#[$meta])*
        pub enum $name {
            /// SQLite corruption retains the original cause from shared primitives.
            #[error("stored evidence is corrupt: {0}")]
            StorageCorruption(#[source] rusqlite::Error),
            /// The configured storage reserve was reached.
            #[error("storage capacity reserve reached")]
            Capacity,
            /// SQLite or stored evidence failed an integrity check.
            #[error("stored evidence is corrupt")]
            Corruption,
            /// SQLite reported a full disk.
            #[error("disk full")]
            DiskFull(#[source] rusqlite::Error),
            /// A configured input bound was exceeded before storage.
            #[error("storage input exceeds {what}")]
            LimitExceeded {
                /// Configured bound that refused admission.
                what: &'static str,
            },
            /// SQLite refused execution because another connection holds a lock.
            /// This failure did not commit; retry with the original identity.
            #[error("SQLite is busy: {0}")]
            Busy(#[source] rusqlite::Error),
            /// SQLite refused a constraint; this is not an application identity conflict.
            #[error("SQLite constraint violation: {0}")]
            ConstraintViolation(#[source] rusqlite::Error),
            /// Bounded model text could not be constructed; the exact reason is retained.
            #[error("invalid bounded storage text: {0}")]
            Text(#[source] analyte_model::TextError),
            /// A filesystem operation failed.
            #[error("filesystem operation failed")]
            Io(#[from] std::io::Error),
            /// SQLite failed; its original cause is preserved.
            #[error("SQLite operation failed: {0}")]
            Sqlite(#[source] rusqlite::Error),
            /// Typed evidence could not be encoded or decoded.
            #[error("evidence encoding failed: {0}")]
            Json(#[from] serde_json::Error),
            /// The worker could not establish completion of this boundary's operation.
            #[error("{0}")]
            Worker(#[from] crate::WorkerError),
            $($variants)*
        }
        impl From<rusqlite::Error> for $name {
            fn from(error: rusqlite::Error) -> Self {
                crate::StorageError::from(error).into()
            }
        }
        impl From<crate::StorageError> for $name {
            fn from(error: crate::StorageError) -> Self {
                match error {
                    crate::StorageError::Worker(cause) => Self::Worker(cause),
                    crate::StorageError::Corruption(cause) => Self::StorageCorruption(cause),
                    crate::StorageError::Capacity => Self::Capacity,
                    crate::StorageError::DiskFull(cause) => Self::DiskFull(cause),
                    crate::StorageError::LimitExceeded { what } => Self::LimitExceeded { what },
                    crate::StorageError::Busy(cause) => Self::Busy(cause),
                    crate::StorageError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
                    crate::StorageError::Text(cause) => Self::Text(cause),
                    crate::StorageError::Io(cause) => Self::Io(cause),
                    crate::StorageError::Sqlite(cause) => Self::Sqlite(cause),
                    crate::StorageError::Json(cause) => Self::Json(cause),
                }
            }
        }
    };
}
