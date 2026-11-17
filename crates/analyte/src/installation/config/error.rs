//! Distinguishes installation input refusals from file, parsing and machine configuration failures.
/// Failures loading bounded installation files or validating their relationships.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A bounded configuration file is not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::string::FromUtf8Error),
    /// A required installation setting or relationship is refused.
    #[error("ASTM receive dialog currently supports strict checksum hex only")]
    AstmReceiveDialogCurrentlySupportsStrictChecksumHexOnly,
    /// A required installation setting or relationship is refused.
    #[error("LAW gates require injected loopback listener")]
    LawGatesRequireInjectedLoopbackListener,
    /// A required installation setting or relationship is refused.
    #[error("admin address must be 127.0.0.1")]
    AdminAddressNotLoopback,
    /// A required installation setting or relationship is refused.
    #[error("admin port must be between 1 and 65535")]
    AdminPortOutOfRange,
    /// A required installation setting or relationship is refused.
    #[error("capture machine ID must be one directory component")]
    CaptureMachineIdMustBeOneDirectoryComponent,
    /// A required installation setting or relationship is refused.
    #[error("closing hook requires a loopback TCP client")]
    ClosingHookRequiresALoopbackTcpClient,
    /// A required installation setting or relationship is refused.
    #[error("commit hook requires injected ASTM loopback listener")]
    CommitHookRequiresInjectedAstmLoopbackListener,
    /// A required installation setting or relationship is refused.
    #[error("commit hook requires injected LAW loopback listener")]
    CommitHookRequiresInjectedLawLoopbackListener,
    /// A required installation setting or relationship is refused.
    #[error("configuration exceeds 1 MiB")]
    ConfigurationTooLarge,
    /// A required installation setting or relationship is refused.
    #[error("configuration has no parent directory")]
    ConfigurationHasNoParentDirectory,
    /// A required installation setting or relationship is refused.
    #[error("duplicate machine identity")]
    DuplicateMachineIdentity,
    /// A required installation setting or relationship is refused.
    #[error("inconsistent shared destination")]
    InconsistentSharedDestination,
    /// A required installation setting or relationship is refused.
    #[error("invalid program version, machine count or storage limits")]
    InvalidProgramVersionMachineCountOrStorageLimits,
    /// A required installation setting or relationship is refused.
    #[error("invalid signing rotation references or count")]
    InvalidSigningRotationReferencesOrCount,
    /// A required installation setting or relationship is refused.
    #[error("receive transport operation timeout cannot exceed 60000 ms")]
    ReceiveTransportOperationTimeoutCannotExceed60000Ms,
    /// A required installation setting or relationship is refused.
    #[error("store record limit is smaller than machine message or effective config")]
    StoreRecordLimitIsSmallerThanMachineMessageOrEffectiveConfig,
    /// A required installation setting or relationship is refused.
    #[error("stuck shutdown hook requires injected loopback listeners")]
    StuckShutdownHookRequiresInjectedLoopbackListeners,
    /// A required installation setting or relationship is refused.
    #[error("test listener does not match configured loopback server")]
    TestListenerDoesNotMatchConfiguredLoopbackServer,
    /// A required installation setting or relationship is refused.
    #[error("test polling requires a loopback override and a bounded interval")]
    TestPollingRequiresALoopbackOverrideAndABoundedInterval,
    /// A required installation setting or relationship is refused.
    #[error("unknown test destination")]
    UnknownTestDestination,
    /// A required installation setting or relationship is refused.
    #[error("unknown test machine")]
    UnknownTestMachine,
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Sink(#[from] analyte_sink::Error),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Alerts(#[from] super::AlertConfigError),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Webhooks(#[from] super::WebhookConfigError),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Address(#[from] std::net::AddrParseError),
    /// Preserves the original loading or validation failure.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
}

// Existing unit assertions still inspect the original validation cause during API migration.
#[cfg(test)]
impl Error {
    pub(super) fn downcast_ref<T: std::error::Error + 'static>(&self) -> Option<&T> {
        let cause: &(dyn std::error::Error + 'static) = match self {
            Self::Encoding(cause) => cause,
            Self::Io(cause) => cause,
            Self::Parse(cause) => cause,
            Self::Config(cause) => cause,
            Self::Text(cause) => cause,
            Self::Sink(cause) => cause,
            Self::Alerts(cause) => cause,
            Self::Webhooks(cause) => cause,
            Self::Address(cause) => cause,
            Self::Integer(cause) => cause,
            Self::AstmReceiveDialogCurrentlySupportsStrictChecksumHexOnly
            | Self::LawGatesRequireInjectedLoopbackListener
            | Self::AdminAddressNotLoopback
            | Self::AdminPortOutOfRange
            | Self::CaptureMachineIdMustBeOneDirectoryComponent
            | Self::ClosingHookRequiresALoopbackTcpClient
            | Self::CommitHookRequiresInjectedAstmLoopbackListener
            | Self::CommitHookRequiresInjectedLawLoopbackListener
            | Self::ConfigurationTooLarge
            | Self::ConfigurationHasNoParentDirectory
            | Self::DuplicateMachineIdentity
            | Self::InconsistentSharedDestination
            | Self::InvalidProgramVersionMachineCountOrStorageLimits
            | Self::InvalidSigningRotationReferencesOrCount
            | Self::ReceiveTransportOperationTimeoutCannotExceed60000Ms
            | Self::StoreRecordLimitIsSmallerThanMachineMessageOrEffectiveConfig
            | Self::StuckShutdownHookRequiresInjectedLoopbackListeners
            | Self::TestListenerDoesNotMatchConfiguredLoopbackServer
            | Self::TestPollingRequiresALoopbackOverrideAndABoundedInterval
            | Self::UnknownTestDestination
            | Self::UnknownTestMachine => return None,
        };
        cause.downcast_ref::<T>()
    }
}
