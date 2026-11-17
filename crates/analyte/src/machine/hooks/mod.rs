//! Selects host-owned synthetic pause points without changing protocol state.
use crate::runtime::test_hooks::Error;
use tokio::sync::watch;
#[cfg(feature = "test-hooks")]
mod enabled;
#[cfg(feature = "test-hooks")]
pub(crate) use enabled::MachineHooks;
#[cfg(not(feature = "test-hooks"))]
mod disabled;
#[cfg(not(feature = "test-hooks"))]
pub(crate) use disabled::MachineHooks;

#[derive(Clone, Copy)]
pub(crate) enum Pause {
    AutolyserQuiescent,
    InputProcessed,
    LawPrecommit,
    LawCommitted,
    AstmCommitted,
    ConnectionClosing,
    LawRsp,
    LawHandover,
    LawQueued,
}
use crate::runtime::hooks::Operation;
