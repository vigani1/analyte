//! Provides synthetic peer messages, bounded waits and controlled free-space probes for tests.
#![forbid(unsafe_code)]

pub mod orders;
pub mod poct;

/// A record-only definition with the documented cobas packing mode for tests.
pub const ASTM_PACKED_RECORD_ONLY: &str = include_str!("../astm-packed-record-only.toml");

/// Wall-clock allowance for scheduler and durable disk delays on loaded CI runners.
/// This fixed factor applies only to harness budgets, never asserted protocol timers.
pub const CI_SCALE: u64 = 3;

/// Upper bound for broken test waits, independent of protocol deadlines.
pub const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(60 * CI_SCALE);

/// Store requests in tests allow slow durable writes without changing host defaults.
pub const STORE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10 * CI_SCALE);

/// Reports a fixed free-space budget without consulting the real disk.
pub struct Space(pub u64);

impl analyte_store::FreeSpace for Space {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(self.0)
    }
}

/// Reports unlimited space for tests whose subject is unrelated to disk capacity.
pub struct UnlimitedSpace;

impl analyte_store::FreeSpace for UnlimitedSpace {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

/// Reports a budget that tests can change while a store is running.
pub struct AdjustableSpace(pub std::sync::Arc<std::sync::atomic::AtomicU64>);
impl analyte_store::FreeSpace for AdjustableSpace {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(self.0.load(std::sync::atomic::Ordering::SeqCst))
    }
}

/// Holds a drop observer beside a free-space probe for store ownership tests.
pub struct ObservedSpace<T> {
    /// Probe used for admission decisions.
    pub space: UnlimitedSpace,
    /// Owned lifecycle observer, dropped with the probe.
    pub observer: T,
}
impl<T: Send + 'static> analyte_store::FreeSpace for ObservedSpace<T> {
    fn available(&self, path: &std::path::Path) -> std::io::Result<u64> {
        analyte_store::FreeSpace::available(&self.space, path)
    }
}

/// Builds a synthetic program with explicit machine entries and fixed test storage budgets.
#[must_use]
pub fn program(machines: &str) -> String {
    let mut limits = store_limits();
    limits.record_bytes = 2_097_152;
    limits.busy_timeout = std::time::Duration::from_secs(1);
    program_with_limits(machines, &limits)
}

/// Builds a synthetic program with explicit test storage budgets and machine entries.
#[must_use]
pub fn program_with_limits(machines: &str, limits: &analyte_store::Limits) -> String {
    format!(
        "schema_version = 1\ndata_directory = 'data'\n{machines}[store]\nreserve_bytes = {}\ndatabase_bytes = {}\nrecord_bytes = {}\npage_rows = {}\nbusy_timeout_ms = {}\n",
        limits.reserve_bytes,
        limits.database_bytes,
        limits.record_bytes,
        limits.page_rows,
        limits.busy_timeout.as_millis(),
    )
}

/// Where [`write_machine`] puts its definition, relative to the program
/// folder: a `<vendor>/<model>/` folder, as setup writes one, so deliveries
/// name a model (ADR 0033).
pub const DEFINITION: &str = "test/synthetic/machine.toml";

/// The model deliveries name for a definition at [`DEFINITION`].
pub const MODEL: &str = "test/synthetic";

/// The path of test definition `name` under `root`: `root/test/<name>/machine.toml`.
#[must_use]
pub fn definition_path(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    root.join("test").join(name).join("machine.toml")
}

/// Writes `text` as test definition `name` at [`definition_path`], creating
/// its folders, and returns the program file's `definition` value for it.
/// # Errors
/// Returns the original folder or file write failure.
pub fn write_definition(
    root: &std::path::Path,
    name: &str,
    text: impl AsRef<[u8]>,
) -> std::io::Result<String> {
    let path = definition_path(root, name);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, text)?;
    Ok(format!("test/{name}/machine.toml"))
}

/// Writes one synthetic machine and site using the common program storage policy.
/// # Errors
/// Returns the original file write failure.
pub fn write_machine(
    root: &std::path::Path,
    definition: &str,
    endpoint: &str,
) -> std::io::Result<std::path::PathBuf> {
    write_definition(root, "synthetic", definition)?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../core/config/examples/site.toml").rewrite("127.0.0.1:5600", endpoint),
    )?;
    let path = root.join("program.toml");
    std::fs::write(
        &path,
        program(
            "[[machines]]\nid = 'synthetic'\ndefinition = 'test/synthetic/machine.toml'\nsite = 'site.toml'\n",
        ),
    )?;
    Ok(path)
}

/// Builds the standard synthetic ASTM TCP definition without serial settings.
#[must_use]
pub fn astm_tcp() -> String {
    include_str!("../../../core/config/examples/astm.toml")
        .rewrite("kind = \"serial\"", "kind = \"tcp_server\"")
        .rewrite("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "")
        .rewrite("transport = \"serial\"", "transport = \"tcp_server\"")
}

/// Synthetic Standard Webhooks credential shared by local test peers.
pub const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

/// Returns the common synthetic store admission policy.
#[must_use]
pub fn store_limits() -> analyte_store::Limits {
    analyte_store::Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: std::time::Duration::from_millis(20),
    }
}

/// Edits synthetic text while requiring the old text to exist.
pub trait ConfigEdit {
    /// Replaces all occurrences, including repeated settings in a fixture.
    /// # Panics
    /// Panics if the target is empty or absent, so stale fixture edits fail visibly.
    #[must_use]
    fn rewrite(&self, from: &str, to: &str) -> String;
}

impl ConfigEdit for str {
    fn rewrite(&self, from: &str, to: &str) -> String {
        assert!(!from.is_empty(), "fixture edit target must not be empty");
        assert!(self.contains(from), "fixture edit target absent: {from:?}");
        self.replace(from, to)
    }
}

#[cfg(test)]
mod edit_tests {
    use super::ConfigEdit as _;

    #[test]
    fn replaces_every_present_target() {
        assert_eq!("key=1 key=1".rewrite("key=1", "key=2"), "key=2 key=2");
    }

    #[test]
    #[should_panic(expected = "fixture edit target absent")]
    fn rejects_stale_target() {
        let _ = "key=1".rewrite("renamed=1", "key=2");
    }

    #[test]
    #[should_panic(expected = "fixture edit target must not be empty")]
    fn rejects_empty_target() {
        let _ = "key=1".rewrite("", "key=2");
    }
}
