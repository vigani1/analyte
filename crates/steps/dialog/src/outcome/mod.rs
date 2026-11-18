//! Declares stored order outcome code enums and the codes the host writes for any protocol.
//!
//! Each protocol keeps its codes in `<key>/outcome.rs` with one shape: a `Code`
//! enum of fixed codes and, where the protocol stores diagnostic text, a
//! `Detail` enum. Stored spellings are compatibility data in the protected
//! store, so they are written out once here and never derived from `Debug`.
#[cfg(test)]
mod tests;

/// Declares a closed enum of stored codes; each spelling sits beside its variant.
/// `lib.rs` declares this module first with `#[macro_use]`, so every protocol
/// module can invoke `codes!` directly.
macro_rules! codes {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            $(#[doc = concat!("Stored as `", $text, "`.")] $variant,)+
        }
        impl $name {
            /// Returns the exact historical spelling retained in the protected store.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

codes! {
    /// Stored order outcome codes the host writes for any protocol.
    Host {
        QueryReservationRefused => "query-reservation-refused",
        QueryDeadlineBeforeSend => "query-deadline-before-send",
        HostRestart => "host-restart",
        ConnectionEndedBeforeConfirmation => "connection-ended-before-transfer-confirmation",
    }
}
