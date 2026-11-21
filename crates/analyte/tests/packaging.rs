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
        "-ConfigWritable -CreateAdminToken -UsersMayStart -Start",
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
    // Run 44411503596: WMI's Win32_Service.Create refuses the virtual account
    // (code 22); sc.exe registers it without a password. Other WMI methods,
    // such as reading a process's owner, are fine.
    assert!(!install.contains("-MethodName Create"));
    assert!(install.contains(
        r#"'{0} Analyte type= own start= auto error= normal binPath= "{1}" obj= "{2}" DisplayName= Analyte'"#
    ));
    // The printed token is discarded; the installer reads the protected file.
    assert!(install.contains("$null = & $Executable --config $Config admin token --rotate"));
    assert!(install.contains("if (Test-Path -LiteralPath $token)"));
    Ok(())
}

// Owner request 2026-11-19 (Aperture logo, PR #273): the installer and the Apps
// list show the Analyte icon. The release script stages the one committed
// analyte.ico, which holds PNG frames at 16 and 20 px (from the hand-tuned
// SVGs) and at 24, 32, 48 and 256 px (from the large icon); docs/design/README.md.
#[test]
fn windows_installer_shows_the_analyte_icon() -> Outcome {
    let installer = read("windows/analyte.iss")?;
    for expected in [
        r"SetupIconFile={#SourceDir}\analyte.ico",
        r"UninstallDisplayIcon={app}\analyte.ico",
        r#"Source: "{#SourceDir}\analyte.ico"; DestDir: "{app}"; Flags: ignoreversion"#,
    ] {
        assert!(installer.contains(expected), "analyte.iss lacks {expected}");
    }
    let build = read("build.ps1")?;
    assert!(
        build.contains("Copy-Item -LiteralPath crates/analyte/analyte.ico -Destination $stage")
    );
    let icon = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("analyte.ico"))?;
    // The icon directory: reserved, type 1 (icon), six frames.
    assert_eq!(icon.get(..6), Some([0, 0, 1, 0, 6, 0].as_slice()));
    let mut sides = Vec::new();
    for entry in icon
        .get(6..102)
        .ok_or("frame entries missing")?
        .as_chunks::<16>()
        .0
    {
        let word = |at: usize| -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
            let bytes = entry
                .get(at..at.checked_add(4).ok_or("offset")?)
                .ok_or("entry")?;
            Ok(usize::try_from(u32::from_le_bytes(bytes.try_into()?))?)
        };
        // A 256 px frame is written as 0 in the one-byte width and height.
        let side = match entry.first() {
            Some(0) => 256,
            Some(&side) => u32::from(side),
            None => return Err("empty entry".into()),
        };
        let (length, offset) = (word(8)?, word(12)?);
        let png = icon
            .get(offset..offset.checked_add(length).ok_or("length")?)
            .ok_or("frame outside the file")?;
        assert!(
            png.starts_with(b"\x89PNG\r\n\x1a\n"),
            "frame {side} is no PNG"
        );
        let width = u32::from_be_bytes(png.get(16..20).ok_or("no width")?.try_into()?);
        let height = u32::from_be_bytes(png.get(20..24).ok_or("no height")?.try_into()?);
        assert_eq!((width, height), (side, side));
        sides.push(side);
    }
    assert_eq!(sides, [16, 20, 24, 32, 48, 256]);
    Ok(())
}

// ADR 0019 (desktop app): the installer puts analyte-app.exe beside the engine
// with a Start menu entry "Analyte" whose AppUserModelID is the app's
// identifier (so notifications say "Analyte"), writes the same sign-in entry
// the tray writes, the app's settings with the admin port, lets signed-in
// users start (never stop) the service, closes the app before replacing or
// removing it, and opens the app instead of the browser.
#[test]
fn windows_installer_installs_the_desktop_app() -> Outcome {
    let installer = read("windows/analyte.iss")?;
    let install = read("windows/install.ps1")?;
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app");
    let config = std::fs::read_to_string(app.join("tauri.conf.json"))?;
    let identifier = config
        .lines()
        .find_map(|line| line.trim().strip_prefix("\"identifier\": \""))
        .and_then(|rest| rest.strip_suffix("\","))
        .ok_or("tauri.conf.json names no identifier")?;
    assert!(config.contains("\"productName\": \"Analyte\""));
    let autostart = std::fs::read_to_string(app.join("src/shell/autostart.rs"))?;
    assert!(autostart.contains("pub(super) const VALUE: &str = \"Analyte\";"));
    let shell = std::fs::read_to_string(app.join("src/shell/mod.rs"))?;
    assert!(shell.contains("pub const TRAY_ONLY: &str = \"--tray\";"));
    let engine = std::fs::read_to_string(app.join("src/engine/mod.rs"))?;
    assert!(engine.contains("pub const SETTINGS: &str = \"analyte-app.toml\";"));
    for expected in [
        r#"Source: "{#SourceDir}\analyte-app.exe"; DestDir: "{app}"; Flags: ignoreversion"#.to_owned(),
        format!(
            r#"Name: "{{autoprograms}}\Analyte"; Filename: "{{app}}\analyte-app.exe"; AppUserModelID: "{identifier}";"#
        ),
        r#"ValueName: "Analyte"; ValueData: """{app}\analyte-app.exe"" --tray"; Flags: uninsdeletevalue; Tasks: autostart"#.to_owned(),
        r#"Name: "autostart"; Description: "Start Analyte when I sign in""#.to_owned(),
        r"SaveStringToFile(ExpandConstant('{app}\analyte-app.toml'),".to_owned(),
        "+ 'admin_port = ' + Port + #13#10, False)".to_owned(),
        r#"Type: files; Name: "{app}\analyte-app.toml""#.to_owned(),
        r#"Filename: "{app}\analyte-app.exe"; Description: "Open Analyte"; Flags: postinstall nowait skipifsilent runasoriginaluser; Check: CanOpenApp"#.to_owned(),
        r"Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM analyte-app.exe',".to_owned(),
        "  Result := '';\n  CloseDesktopApp;".to_owned(),
        "  begin\n    CloseDesktopApp;".to_owned(),
        "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}".to_owned(),
    ] {
        assert!(installer.contains(&expected), "analyte.iss lacks {expected}");
    }
    // SERVICE_START for interactive users only: no stop ("WP") or change right.
    assert!(install.contains("$startRight = '(A;;RP;;;IU)'"));
    assert!(!install.contains("WP;;;IU"));
    let build = read("build.ps1")?;
    assert!(
        build.contains(
            "Copy-Item -LiteralPath $App -Destination (Join-Path $stage 'analyte-app.exe')"
        )
    );
    Ok(())
}

// ADR 0021 (Windows sign-in), "Who may sign in": setup creates the local group
// "Analyte users" only when it is missing and adds the person who started it,
// so an upgrade keeps the members and a repeat is no error; uninstall keeps the
// group, since it may list other people, and says so.
#[test]
fn windows_setup_creates_and_keeps_the_sign_in_group() -> Outcome {
    let installer = read("windows/analyte.iss")?;
    let install = read("windows/install.ps1")?;
    let uninstall = read("windows/uninstall.ps1")?;
    for expected in [
        "$signInGroup = 'Analyte users'",
        "} catch [Microsoft.PowerShell.Commands.GroupNotFoundException] {",
        "New-LocalGroup -Name $signInGroup",
        "Add-LocalGroupMember -Group $signInGroup -Member $starter -ErrorAction Stop",
        "} catch [Microsoft.PowerShell.Commands.MemberExistsException] {",
        // The person, not the elevated administrator: owners of older ancestor
        // processes in this session, and never a service account.
        "$parent.SessionId -ne $self.SessionId -or $parent.CreationDate -gt $child.CreationDate",
        "if ($starter -match '^S-1-(5-21|12-1)-') {",
    ] {
        assert!(install.contains(expected), "install.ps1 lacks {expected}");
    }
    for script in [&install, &uninstall] {
        assert!(!script.contains("Remove-LocalGroup"));
        assert!(!script.contains("Remove-LocalGroupMember"));
    }
    assert!(uninstall.contains("The local group 'Analyte users' was kept"));
    assert!(installer.contains("'group \"Analyte users\" was kept too"));
    Ok(())
}

// Owner request 2026-11-21 (network setup): analyzers on the lab network reach
// the service through one inbound Windows Firewall rule, added by install.ps1
// (which the installer runs) and removed by uninstall.ps1, under a fixed name so
// a reinstall or upgrade replaces it. It allows TCP to analyte.exe only, from
// this PC's own subnets only, on every profile. Setup step 3 names the same rule.
#[test]
fn windows_firewall_rule_lets_only_local_analyzers_in() -> Outcome {
    let installer = read("windows/analyte.iss")?;
    let install = read("windows/install.ps1")?;
    let uninstall = read("windows/uninstall.ps1")?;
    let verdict = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/admin/setup/wizard/verdict.rs"),
    )?;
    let name = "Analyte - analyzers on the lab network";
    assert!(verdict.contains(&format!("FIREWALL_RULE: &str = \"{name}\";")));
    assert!(verdict.contains("FIREWALL_RULE_NAME: &str = \"Analyte-Analyzers-In\";"));
    for expected in [
        "$firewallRule = 'Analyte-Analyzers-In'".to_owned(),
        format!("$firewallName = '{name}'"),
        "if (Get-NetFirewallRule -Name $firewallRule -ErrorAction SilentlyContinue) {\n    Remove-NetFirewallRule -Name $firewallRule -ErrorAction Stop\n}".to_owned(),
        "-Direction Inbound -Action Allow -Protocol TCP -Program $Executable -RemoteAddress LocalSubnet `".to_owned(),
        "-Profile Domain, Private, Public -Enabled True -ErrorAction Stop".to_owned(),
    ] {
        assert!(install.contains(&expected), "install.ps1 lacks {expected}");
    }
    // No port and no address beyond the local subnets: what analyte.exe binds decides.
    assert!(!install.contains("-LocalPort"));
    assert!(!install.contains("-RemoteAddress Any"));
    for expected in [
        "$firewallRule = 'Analyte-Analyzers-In'".to_owned(),
        "Remove-NetFirewallRule -Name $firewallRule -ErrorAction Stop".to_owned(),
        format!("Windows Firewall rule '{name}' removed."),
    ] {
        assert!(
            uninstall.contains(&expected),
            "uninstall.ps1 lacks {expected}"
        );
    }
    // Removed before the early return for a service that is already gone.
    let removed = uninstall
        .find("Remove-NetFirewallRule")
        .ok_or("no removal")?;
    let early = uninstall
        .find("if (-not $existing)")
        .ok_or("no early return")?;
    assert!(removed < early);
    assert!(installer.contains("its Windows Firewall rule"));
    Ok(())
}
