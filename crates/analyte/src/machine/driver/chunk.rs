//! Shares the chunk-host Effects boilerplate as one macro.
/// Supplies the common input-journal and monotonic-wait policy for chunk hosts.
/// Hosts keep their protocol transitions and effects explicit; this only shares
/// the Effects boilerplate for a dialog with a connection `start` epoch.
macro_rules! chunk_effects {
    ($error:ty) => {
        type Error = $error;
        const CUSTODY: $crate::machine::driver::Custody = $crate::machine::driver::Custody::Chunks;
        fn wait(&self) -> Result<Option<std::time::Duration>, $error> {
            Ok(self
                .dialog
                .next_deadline()
                .map(|deadline| deadline.saturating_sub(self.start.elapsed())))
        }
    };
}
pub(in crate::machine) use chunk_effects;
