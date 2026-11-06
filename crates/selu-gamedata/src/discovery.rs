//! Shared read-only install discovery for Studio and test execution.
use std::path::{Path, PathBuf};
/// Install folders from the registry (read-only `reg query`).
pub fn registry_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let queries: [&[&str]; 2] = [
        &[
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            "/s",
            "/f",
            "World of Warcraft",
            "/d",
        ],
        &[
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
            "/s",
        ],
    ];
    for args in queries {
        let Some(bytes) = registry_query(args) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let line = line.trim();
            for key in ["InstallLocation", "InstallPath", "GamePath"] {
                if let Some(rest) = line.strip_prefix(key)
                    && let Some(value) = rest.trim().strip_prefix("REG_SZ")
                {
                    let mut p = PathBuf::from(value.trim().trim_end_matches(['\\', '/']));
                    // InstallPath points at a client folder (`..\_retail_`).
                    if p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('_'))
                        && let Some(parent) = p.parent()
                    {
                        p = parent.to_path_buf();
                    }
                    if !p.as_os_str().is_empty() {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

/// Usual install folders on every drive.
pub fn common_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for letter in b'C'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if !Path::new(&drive).exists() {
            continue;
        }
        for rel in [
            r"World of Warcraft",
            r"Program Files (x86)\World of Warcraft",
            r"Program Files\World of Warcraft",
            r"Games\World of Warcraft",
            r"Games\Battle.net\World of Warcraft",
            r"Blizzard\World of Warcraft",
            r"Battle.net\World of Warcraft",
        ] {
            let p = Path::new(&drive).join(rel);
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out
}

// Bound registry discovery and reap only the child this call starts.
fn registry_query(args: &[&str]) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new("reg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let _pid = child.id();
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    reader.join().ok().flatten()
}
