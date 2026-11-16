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

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_store::FreeSpace as _;

    #[test]
    fn adr0003_rule1_disk_probe_failure_is_not_available_capacity() {
        let missing = Path::new("\0");
        assert!(Volume.available(missing).is_err());
    }
}
