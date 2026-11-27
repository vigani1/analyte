//! Distinguishes installation input refusals from file, parsing and machine configuration failures.
/// Failures loading bounded installation files or validating their relationships.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A bounded configuration file is not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::string::FromUtf8Error),
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-astm-checksum-hex-only"))]
    UnsupportedChecksumMode,
    /// The machine definition is not in a `<vendor>/<model>/` folder, so its
    /// results and order events could not name the machine model (ADR 0033).
    #[error("{}", analyte_i18n::message("setup-config-definition-outside-model-folder", [("definition", (definition.display()).to_string().into()), ("error", (source).to_string().into())]))]
    DefinitionOutsideModelFolder {
        /// The definition file.
        definition: std::path::PathBuf,
        /// Why its folder names are not a model key.
        source: analyte_contract::document::InvalidModel,
    },
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("LAW gates require injected loopback listener")]
    MissingLawGateListener,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-admin-address-loopback"))]
    AdminAddressNotLoopback,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-admin-port-range"))]
    AdminPortOutOfRange,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-capture-machine-id"))]
    InvalidCaptureMachineId,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("closing hook requires a loopback TCP client")]
    InvalidClosingHookTransport,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("commit hook requires injected ASTM loopback listener")]
    MissingAstmCommitListener,
    /// The test-only ASTM pause found no ASTM stream machine to pause.
    #[cfg(feature = "test-hooks")]
    #[error("ASTM pause marker requires an ASTM stream machine")]
    MissingAstmPauseMachine,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("commit hook requires injected LAW loopback listener")]
    MissingLawCommitListener,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-too-large"))]
    ConfigurationTooLarge,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-no-parent-directory"))]
    ConfigurationHasNoParentDirectory,
    /// A machine without a definition waits for the setup page, which needs `[admin]`.
    #[error("{}", analyte_i18n::text("setup-config-setup-needs-admin"))]
    SetupNeedsAdmin,
    /// A machine waiting for setup has capture or overrides, which need a definition.
    #[error("{}", analyte_i18n::text("setup-config-awaiting-setup-with-settings"))]
    AwaitingSetupWithSettings,
    /// No machine is listed and there is no setup page to add one.
    #[error("{}", analyte_i18n::text("setup-config-no-machines"))]
    NoMachines,
    /// Two machines would open the same port, serial number, address or folder.
    #[error("{}", analyte_i18n::message("setup-config-shared-connection", [("first", (first).to_string().into()), ("second", (second).to_string().into()), ("what", (what).to_string().into())]))]
    SharedConnection {
        /// The machine listed first.
        first: String,
        /// The machine listed later.
        second: String,
        /// The shared connection, for example "serial port COM3".
        what: String,
    },
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-duplicate-machine-identity"))]
    DuplicateMachineIdentity,
    /// One lab app name is written with different settings in two places, so
    /// which address or keys are meant is unknown; Analyte never guesses.
    #[error("{}", analyte_i18n::message("setup-config-conflicting-lab-app", [("name", (name).to_string().into()), ("first", (first).to_string().into()), ("second", (second).to_string().into())]))]
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
    #[error("{}", analyte_i18n::message("setup-config-name-clash", [("what", (what).to_string().into()), ("first", (first).to_string().into()), ("second", (second).to_string().into()), ("variable", (variable).to_string().into())]))]
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
    #[error("{}", analyte_i18n::message("setup-config-duplicate-retired-destination", [("name", (.0).to_string().into())]))]
    DuplicateRetiredDestination(String),
    /// Too many retired destinations to deliver from.
    #[error("{}", analyte_i18n::text("setup-config-too-many-retired-destinations"))]
    TooManyRetiredDestinations,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-invalid-program-limits"))]
    InvalidProgramLimits,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-invalid-signing-rotation"))]
    InvalidSigningRotation,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-receive-timeout-too-long"))]
    ReceiveTimeoutTooLong,
    /// A required installation setting or relationship is refused.
    #[error("{}", analyte_i18n::text("setup-config-store-record-limit-too-small"))]
    StoreRecordLimitTooSmall,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("stuck shutdown hook requires injected loopback listeners")]
    MissingShutdownListeners,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("test listener does not match configured loopback server")]
    TestListenerMismatch,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("test polling requires a loopback override and a bounded interval")]
    InvalidTestPolling,
    /// A synthetic store wait must remain bounded behind a loopback override.
    #[cfg(feature = "test-hooks")]
    #[error("test store budget requires a loopback override and 1 through 60 seconds")]
    InvalidTestStoreBudget,
    /// An ASTM frame budget override must apply to a bounded synthetic server.
    #[cfg(feature = "test-hooks")]
    #[error("test ASTM frame budget requires a loopback ASTM server and 1 through 60 seconds")]
    InvalidTestAstmFrameBudget,
    /// The synthetic shutdown I/O budget must stay within fixture limits.
    #[cfg(feature = "test-hooks")]
    #[error("test shutdown I/O budget must be 5 through 60 seconds")]
    InvalidTestShutdownIoBudget,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
    #[error("unknown test destination")]
    UnknownTestDestination,
    /// A required installation setting or relationship is refused.
    #[cfg(feature = "test-hooks")]
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
            Self::LabApps => analyte_i18n::text("setup-config-clashing-lab-apps"),
            Self::Keys => analyte_i18n::text("setup-config-clashing-keys"),
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
            | Self::AdminAddressNotLoopback
            | Self::AdminPortOutOfRange
            | Self::InvalidCaptureMachineId
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
            | Self::StoreRecordLimitTooSmall => return None,

            #[cfg(feature = "test-hooks")]
            Self::MissingAstmPauseMachine
            | Self::MissingLawGateListener
            | Self::InvalidClosingHookTransport
            | Self::MissingAstmCommitListener
            | Self::MissingLawCommitListener
            | Self::MissingShutdownListeners
            | Self::TestListenerMismatch
            | Self::InvalidTestPolling
            | Self::InvalidTestStoreBudget
            | Self::InvalidTestAstmFrameBudget
            | Self::InvalidTestShutdownIoBudget
            | Self::UnknownTestDestination
            | Self::UnknownTestMachine => return None,
        };
        cause.downcast_ref::<T>()
    }
}
