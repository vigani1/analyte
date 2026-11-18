//! Tests the service unit, the starter configuration and the installer packages agree.
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
        ("Service", "ReadWritePaths", "/var/lib/analyte /etc/analyte"),
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
    assert!(command.starts_with("/usr/bin/analyte run --config /etc/analyte/analyte.toml "));
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

type Outcome = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The packaging folder at the repository root.
fn packaging() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging")
}

/// The value of `key = value` on the first line that sets it, without quotes.
fn setting<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches(|c| c == '"' || c == '\''))
    })
}

fn read(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(packaging().join(path))
}

// Owner request 2026-11-18 (installers): both starter programs pass check-config
// with one machine waiting for the setup page, so the service starts with no lab;
// they share one site file and name the data folder each installer protects.
#[test]
fn starter_configuration_passes_check_config() -> Outcome {
    for os in ["windows", "linux"] {
        let program = packaging().join(os).join("starter/analyte.toml");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("check-config")
            .arg(&program)
            .output()?;
        assert!(
            output.status.success(),
            "{os}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)?.trim(),
            "machine-1 not set up: choose its definition on the setup page"
        );
        let text = std::fs::read_to_string(&program)?;
        assert_eq!(setting(&text, "port"), Some("8080"), "{os} admin port");
        assert!(text.contains("\n[admin]\n"), "{os}: setup needs [admin]");
        assert!(
            setting(&text, "definition").is_none(),
            "{os}: no model chosen"
        );
    }
    let site = read("linux/starter/machine-1.site.toml")?;
    assert_eq!(read("windows/starter/machine-1.site.toml")?, site);
    assert_eq!(setting(&site, "retention"), Some("keep"));
    // No address, endpoint or key: the setup page asks for them.
    for absent in ["base_url", "endpoint", "usb_serial", "name"] {
        assert!(setting(&site, absent).is_none(), "{absent}");
    }
    let windows = read("windows/starter/analyte.toml")?;
    assert_eq!(
        setting(&windows, "data_directory"),
        Some(r"C:\ProgramData\Analyte\data")
    );
    let linux = read("linux/starter/analyte.toml")?;
    assert_eq!(setting(&linux, "data_directory"), Some("/var/lib/analyte"));
    Ok(())
}

// Owner request 2026-11-18 (installers): the systemd unit, the nfpm package and
// its scripts agree on the program, config and data paths, the account and the
// folders the service may write.
#[test]
fn linux_unit_and_package_agree() -> Outcome {
    let unit = read("linux/analyte.service")?;
    let package = read("linux/nfpm.yaml")?;
    let preinstall = read("linux/scripts/preinstall.sh")?;
    let postinstall = read("linux/scripts/postinstall.sh")?;
    let installed: Vec<&str> = package
        .lines()
        .filter_map(|line| line.trim().trim_start_matches("- ").strip_prefix("dst: "))
        .collect();
    let start = setting(&unit, "ExecStart").ok_or("ExecStart")?;
    let mut words = start.split_ascii_whitespace();
    let program = words.next().ok_or("program")?;
    let config = words
        .skip_while(|word| *word != "--config")
        .nth(1)
        .ok_or("--config")?;
    for path in [
        program,
        config,
        "/usr/lib/systemd/system/analyte.service",
        "/etc/analyte/machine-1.site.toml",
        // The setup page lists definitions beside the program file.
        "/etc/analyte/machines",
    ] {
        assert!(installed.contains(&path), "not installed: {path}");
    }
    let config_folder = config.rsplit_once('/').ok_or("config folder")?.0;
    let data = setting(&unit, "WorkingDirectory").ok_or("WorkingDirectory")?;
    let writable: Vec<&str> = setting(&unit, "ReadWritePaths")
        .ok_or("ReadWritePaths")?
        .split_ascii_whitespace()
        .collect();
    assert_eq!(writable, [data, config_folder]);
    for folder in &writable {
        assert!(installed.contains(folder), "not created: {folder}");
    }
    assert_eq!(
        setting(&read("linux/starter/analyte.toml")?, "data_directory"),
        Some(data)
    );
    let user = setting(&unit, "User").ok_or("User")?;
    assert_eq!(setting(&unit, "Group"), Some(user));
    assert!(preinstall.contains(&format!("groupadd --system {user}")));
    assert!(preinstall.contains(&format!("--home-dir {data} ")));
    assert!(preinstall.contains(&format!("\" {user}\n")));
    assert!(package.contains(&format!("owner: {user}\n      group: {user}")));
    // Conffiles: an upgrade never replaces the lab's configuration.
    assert_eq!(package.matches("type: config|noreplace").count(), 2);
    for expected in [
        format!("config={config}\n"),
        format!("chown root:{user} {config_folder}\n"),
        format!("chown {user}:{user} {data}\n"),
        format!(
            "runuser -u {user} -- {program} --config \"$config\" admin token --rotate >/dev/null"
        ),
        format!("sudo cat {data}/admin-token"),
    ] {
        assert!(
            postinstall.contains(&expected),
            "postinstall lacks {expected}"
        );
    }
    Ok(())
}

// Owner request 2026-11-18 (installers): the Windows installer puts the starter
// where install.ps1 expects it, only on a first installation, and runs
// install.ps1 with the setup-page switches; stop.ps1 is the one stop rule.
#[test]
fn windows_installer_and_scripts_agree() -> Outcome {
    let installer = read("windows/analyte.iss")?;
    let install = read("windows/install.ps1")?;
    let uninstall = read("windows/uninstall.ps1")?;
    for default in [
        r#"[string]$Executable = "$env:ProgramFiles\Analyte\analyte.exe""#,
        r#"[string]$Config = "$env:ProgramData\Analyte\config\analyte.toml""#,
        r#"[string]$DataDirectory = "$env:ProgramData\Analyte\data""#,
    ] {
        assert!(install.contains(default), "{default}");
    }
    for expected in [
        r"DefaultDirName={autopf}\Analyte",
        r"Result := ExpandConstant('{commonappdata}\Analyte\config\analyte.toml');",
        r"Result := ExpandConstant('{commonappdata}\Analyte\data');",
        r#"DestDir: "{commonappdata}\Analyte\config\machines""#,
        r#"starter\analyte.toml"; DestDir: "{commonappdata}\Analyte\config"; Flags: onlyifdoesntexist uninsneveruninstall; Check: IsFirstInstall"#,
        r#"starter\machine-1.site.toml"; DestDir: "{commonappdata}\Analyte\config"; Flags: onlyifdoesntexist uninsneveruninstall; Check: IsFirstInstall"#,
        "-ConfigWritable -CreateAdminToken -Start",
        "ExtractTemporaryFile('stop.ps1')",
        r"packaging\windows\uninstall.ps1",
        "SetupLogging=no",
        // PR #255 run 44408167214: install.ps1 failed and setup still exited 0.
        // A script counts as done only when it exits 0, and a failed service
        // setup makes the installer exit 10.
        "'', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);",
        // Run 44410216186: Windows PowerShell inherited PowerShell 7's module
        // path and could not load Get-Acl; it gets its own default path.
        r#"'/c "set "PSModulePath=' + ExpandConstant('{commonpf64}\WindowsPowerShell\Modules;'"#,
        r#"+ '{sys}\WindowsPowerShell\v1.0\Modules') + '" && "' + PowerShell"#,
        "  if ServiceReady then",
        "function GetCustomSetupExitCode: Integer;",
        "  if InstallError <> '' then\n    Result := 10",
    ] {
        assert!(installer.contains(expected), "analyte.iss lacks {expected}");
    }
    for script in [&install, &uninstall] {
        assert!(script.contains("& (Join-Path $PSScriptRoot 'stop.ps1')"));
        assert!(!script.contains("ServiceControllerStatus]::Stopped"));
    }
    // The printed token is discarded; the installer reads the protected file.
    assert!(install.contains("$null = & $Executable --config $Config admin token --rotate"));
    assert!(install.contains("if (Test-Path -LiteralPath $token)"));
    Ok(())
}
