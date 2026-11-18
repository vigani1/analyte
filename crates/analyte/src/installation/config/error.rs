//! Distinguishes installation input refusals from file, parsing and machine configuration failures.
/// Failures loading bounded installation files or validating their relationships.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A bounded configuration file is not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::string::FromUtf8Error),
    /// A required installation setting or relationship is refused.
    #[error("ASTM receive dialog currently supports strict checksum hex only")]
    UnsupportedChecksumMode,
    /// A required installation setting or relationship is refused.
    #[error("LAW gates require injected loopback listener")]
    MissingLawGateListener,
    /// A required installation setting or relationship is refused.
    #[error("admin address must be 127.0.0.1")]
    AdminAddressNotLoopback,
    /// A required installation setting or relationship is refused.
    #[error("admin port must be between 1 and 65535")]
    AdminPortOutOfRange,
    /// A required installation setting or relationship is refused.
    #[error("capture machine ID must be one directory component")]
    InvalidCaptureMachineId,
    /// A required installation setting or relationship is refused.
    #[error("closing hook requires a loopback TCP client")]
    InvalidClosingHookTransport,
    /// A required installation setting or relationship is refused.
    #[error("commit hook requires injected ASTM loopback listener")]
    MissingAstmCommitListener,
    /// The test-only ASTM pause found no ASTM stream machine to pause.
    #[cfg(feature = "test-hooks")]
    #[error("ASTM pause marker requires an ASTM stream machine")]
    MissingAstmPauseMachine,
    /// A required installation setting or relationship is refused.
    #[error("commit hook requires injected LAW loopback listener")]
    MissingLawCommitListener,
    /// A required installation setting or relationship is refused.
    #[error("configuration exceeds 1 MiB")]
    ConfigurationTooLarge,
    /// A required installation setting or relationship is refused.
    #[error("configuration has no parent directory")]
    ConfigurationHasNoParentDirectory,
    /// A machine without a definition waits for the setup page, which needs `[admin]`.
    #[error(
        "a machine listed without a definition waits for the setup page; enable [admin] or add its definition"
    )]
    SetupNeedsAdmin,
    /// A machine waiting for setup has capture or overrides, which need a definition.
    #[error(
        "a machine listed without a definition cannot have capture or overrides until it is set up"
    )]
    AwaitingSetupWithSettings,
    /// No machine is listed and there is no setup page to add one.
    #[error(
        "the program lists no machines; enable [admin] so the setup page can add one, or add a [[machines]] entry"
    )]
    NoMachines,
    /// Two machines would open the same port, serial number, address or folder.
    #[error("machines {first} and {second} both use {what}; give each machine its own connection")]
    SharedConnection {
        /// The machine listed first.
        first: String,
        /// The machine listed later.
        second: String,
        /// The shared connection, for example "serial port COM3".
        what: String,
    },
    /// A required installation setting or relationship is refused.
    #[error("duplicate machine identity")]
    DuplicateMachineIdentity,
    /// A required installation setting or relationship is refused.
    #[error("inconsistent shared destination")]
    InconsistentSharedDestination,
    /// Two retired destinations share one name, so which address to use is unknown.
    #[error(
        "retired destination {0} is listed twice; keep one [[retired_destinations]] table per name"
    )]
    DuplicateRetiredDestination(String),
    /// Too many retired destinations to deliver from.
    #[error("at most 128 retired destinations are supported")]
    TooManyRetiredDestinations,
    /// A required installation setting or relationship is refused.
    #[error("invalid program version, machine count or storage limits")]
    InvalidProgramLimits,
    /// A required installation setting or relationship is refused.
    #[error("invalid signing rotation references or count")]
    InvalidSigningRotation,
    /// A required installation setting or relationship is refused.
    #[error("receive transport operation timeout cannot exceed 60000 ms")]
    ReceiveTimeoutTooLong,
    /// A required installation setting or relationship is refused.
    #[error("store record limit is smaller than machine message or effective config")]
    StoreRecordLimitTooSmall,
    /// A required installation setting or relationship is refused.
    #[error("stuck shutdown hook requires injected loopback listeners")]
    MissingShutdownListeners,
    /// A required installation setting or relationship is refused.
    #[error("test listener does not match configured loopback server")]
    TestListenerMismatch,
    /// A required installation setting or relationship is refused.
    #[error("test polling requires a loopback override and a bounded interval")]
    InvalidTestPolling,
    /// A synthetic store wait must remain bounded behind a loopback override.
    #[error("test store budget requires a loopback override and 1 through 60 seconds")]
    InvalidTestStoreBudget,
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
    SinkDestinationError(#[from] analyte_sink::DestinationError),
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
            Self::SinkDestinationError(cause) => cause,
            Self::Alerts(cause) => cause,
            Self::Webhooks(cause) => cause,
            Self::Address(cause) => cause,
            Self::Integer(cause) => cause,
            Self::UnsupportedChecksumMode
            | Self::MissingLawGateListener
            | Self::AdminAddressNotLoopback
            | Self::AdminPortOutOfRange
            | Self::InvalidCaptureMachineId
            | Self::InvalidClosingHookTransport
            | Self::MissingAstmCommitListener
            | Self::MissingLawCommitListener
            | Self::ConfigurationTooLarge
            | Self::ConfigurationHasNoParentDirectory
            | Self::DuplicateMachineIdentity
            | Self::NoMachines
            | Self::SharedConnection { .. }
            | Self::SetupNeedsAdmin
            | Self::AwaitingSetupWithSettings
            | Self::InconsistentSharedDestination
            | Self::DuplicateRetiredDestination(_)
            | Self::TooManyRetiredDestinations
            | Self::InvalidProgramLimits
            | Self::InvalidSigningRotation
            | Self::ReceiveTimeoutTooLong
            | Self::StoreRecordLimitTooSmall
            | Self::MissingShutdownListeners
            | Self::TestListenerMismatch
            | Self::InvalidTestStoreBudget
            | Self::InvalidTestPolling
            | Self::UnknownTestDestination
            | Self::UnknownTestMachine => return None,
            #[cfg(feature = "test-hooks")]
            Self::MissingAstmPauseMachine => return None,
        };
        cause.downcast_ref::<T>()
    }
}
