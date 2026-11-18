//! Tests portable service installation settings.
use std::collections::BTreeMap;

// Plan §11; AGENTS.md
#[test]
fn systemd_unit_structure_and_shutdown_margin()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let unit = include_str!("../../../packaging/linux/analyte.service");
    let mut section = "";
    let mut entries = BTreeMap::new();
    for line in unit.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            assert!(
                matches!(name, "Unit" | "Service" | "Install"),
                "unknown section: {name}"
            );
            section = name;
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("invalid unit directive")?;
        assert!(!section.is_empty() && !key.is_empty() && !value.is_empty());
        assert!(key.bytes().all(|byte| byte.is_ascii_alphabetic()));
        assert!(
            entries.insert((section, key), value).is_none(),
            "duplicate directive: {key}"
        );
    }
    for (section, key, value) in [
        ("Unit", "After", "network-online.target"),
        ("Service", "Type", "simple"),
        ("Service", "User", "analyte"),
        ("Service", "Group", "analyte"),
        ("Service", "SupplementaryGroups", "dialout"),
        ("Service", "StateDirectory", "analyte"),
        ("Service", "StateDirectoryMode", "0700"),
        ("Service", "UMask", "0077"),
        ("Service", "KillSignal", "SIGTERM"),
        ("Service", "Restart", "on-failure"),
        ("Service", "NoNewPrivileges", "true"),
        ("Service", "ProtectSystem", "strict"),
        ("Service", "ReadWritePaths", "/var/lib/analyte"),
        ("Service", "PrivateTmp", "true"),
        ("Service", "StandardError", "journal"),
        ("Install", "WantedBy", "multi-user.target"),
    ] {
        assert_eq!(
            entries.get(&(section, key)),
            Some(&value),
            "{section}.{key}"
        );
    }
    assert!(
        !entries.contains_key(&("Service", "PrivateDevices")),
        "serial devices must remain accessible"
    );
    let command = entries
        .get(&("Service", "ExecStart"))
        .copied()
        .unwrap_or_default();
    let args: Vec<_> = command.split_ascii_whitespace().collect();
    assert!(command.starts_with("/usr/local/bin/analyte run --config /etc/analyte/analyte.toml "));
    let bound = args
        .windows(2)
        .find_map(|pair| match pair {
            ["--shutdown-timeout-secs", value] => value.parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or_default();
    let timeout = entries
        .get(&("Service", "TimeoutStopSec"))
        .and_then(|s| s.strip_suffix('s'))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_default();
    assert!((30..=300).contains(&bound));
    assert_eq!(bound.checked_add(5), Some(timeout));
    Ok(())
}
