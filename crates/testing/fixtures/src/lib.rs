//! Provides synthetic peer messages, bounded waits and controlled free-space probes for tests.
#![forbid(unsafe_code)]

pub mod poct;

/// Upper bound for broken test waits, independent of protocol deadlines.
pub const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(60);

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

/// Builds a synthetic program with explicit machine entries and fixed test storage budgets.
#[must_use]
pub fn program(machines: &str) -> String {
    format!(
        "schema_version = 1\ndata_directory = 'data'\n{machines}[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n"
    )
}

/// Writes one synthetic machine and site using the common program storage policy.
/// # Errors
/// Returns the original file write failure.
pub fn write_machine(
    root: &std::path::Path,
    definition: &str,
    endpoint: &str,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::write(root.join("machine.toml"), definition)?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../core/config/examples/site.toml").replace("127.0.0.1:5600", endpoint),
    )?;
    let path = root.join("program.toml");
    std::fs::write(
        &path,
        program(
            "[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n",
        ),
    )?;
    Ok(path)
}

/// Builds the standard synthetic ASTM TCP definition without serial settings.
#[must_use]
pub fn astm_tcp() -> String {
    include_str!("../../../core/config/examples/astm.toml")
        .replace("kind = \"serial\"", "kind = \"tcp_server\"")
        .replace("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "")
        .replace("transport = \"serial\"", "transport = \"tcp_server\"")
}
