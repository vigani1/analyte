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
    /// The machine definition is not in a `<vendor>/<model>/` folder, so its
    /// results and order events could not name the machine model (ADR 0033).
    #[error(
        "{definition}: a machine definition must sit in a <vendor>/<model>/ folder, such as machines/roche/cobas-e411/machine.toml, so results can name the machine model: {source}"
    )]
    DefinitionOutsideModelFolder {
        /// The definition file.
        definition: std::path::PathBuf,
        /// Why its folder names are not a model key.
        source: analyte_contract::document::InvalidModel,
    },
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
    /// One lab app name is written with different settings in two places, so
    /// which address or keys are meant is unknown; Analyte never guesses.
    #[error(
        "lab app {name} is written differently in {first} and in {second}; make both the same (or give one another name), then start again"
    )]
    ConflictingLabApp {
        /// The lab app name.
        name: String,
        /// The place read first.
        first: String,
        /// The place read later.
        second: String,
    },
    /// Two different lab app names, or two different key names, would share one
    /// environment variable, so one could be given the other's key.
    #[error(
        "{what} \"{first}\" and \"{second}\" clash: both would use the key name {variable}. Rename one so the names differ by more than letter case or - versus _, in the file that defines it (lab-apps.toml, a site file or the program file)"
    )]
    NameClash {
        /// Whether lab app names or key names clash.
        what: Clashing,
        /// The name read first.
        first: String,
        /// The name read later.
        second: String,
        /// The environment variable both would use.
        variable: String,
    },
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
    /// An ASTM frame budget override must apply to a bounded synthetic server.
    #[error("test ASTM frame budget requires a loopback ASTM server and 1 through 60 seconds")]
    InvalidTestAstmFrameBudget,
    /// The synthetic shutdown I/O budget must stay within fixture limits.
    #[error("test shutdown I/O budget must be 5 through 60 seconds")]
    InvalidTestShutdownIoBudget,
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

/// What kind of names clash in [`Error::NameClash`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clashing {
    /// Two lab app names.
    LabApps,
    /// Two credential key names.
    Keys,
}

impl std::fmt::Display for Clashing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::LabApps => "Lab apps",
            Self::Keys => "Keys",
        })
    }
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
            Self::DefinitionOutsideModelFolder { source, .. } => source,
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
            | Self::ConflictingLabApp { .. }
            | Self::NameClash { .. }
            | Self::DuplicateRetiredDestination(_)
            | Self::TooManyRetiredDestinations
            | Self::InvalidProgramLimits
            | Self::InvalidSigningRotation
            | Self::ReceiveTimeoutTooLong
            | Self::StoreRecordLimitTooSmall
            | Self::MissingShutdownListeners
            | Self::TestListenerMismatch
            | Self::InvalidTestShutdownIoBudget
            | Self::InvalidTestStoreBudget
            | Self::InvalidTestAstmFrameBudget
            | Self::InvalidTestPolling
            | Self::UnknownTestDestination
            | Self::UnknownTestMachine => return None,
            #[cfg(feature = "test-hooks")]
            Self::MissingAstmPauseMachine => return None,
        };
        cause.downcast_ref::<T>()
    }
}
