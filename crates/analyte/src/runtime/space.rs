//! Reads free disk space for store capacity checks.
//!
//! The store asks before every receipt and frame, so this must be fast and
//! bounded: it uses the operating system's direct free-space call (statvfs on
//! Linux, `GetDiskFreeSpaceExW` on Windows) through `fs4`, never a child
//! process. Any error is returned as is; the store treats it as "no capacity"
//! and refuses visibly.
use std::{io, path::Path};

/// The volume holding the store's data directory.
pub(crate) struct Volume;

impl analyte_store::FreeSpace for Volume {
    fn available(&self, path: &Path) -> io::Result<u64> {
        fs4::available_space(path)
    }
}
