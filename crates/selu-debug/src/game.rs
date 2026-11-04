//! Testing in the real game: find World of Warcraft installs, copy the
//! built addon into a client's `Interface\AddOns`, and start that client.
//!
//! Installs are found from the Windows uninstall entries and Blizzard's
//! registry key, from the usual folder names on every drive, and from
//! folders the user adds. Inside an install, every `_flavour_` folder with a
//! game executable is one client. Nothing here changes the registry.
//!
//! Copying never silently replaces an addon Studio didn't put there: the
//! folders it copies carry a marker file, and an existing folder without
//! one needs the user's confirmation. Replaced folders go to the Recycle
//! Bin.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Studio writes this file into every addon folder it copies.
pub const MARKER: &str = ".selu-studio";

/// One game client: an install's `_flavour_` folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    /// e.g. `E:\Games\World of Warcraft\_retail_`
    pub folder: PathBuf,
    /// Selu flavour (`retail`, `classic-era`, ...), when known.
    pub flavour: Option<String>,
    /// `Wow.exe`, `WowClassic.exe`, ...
    pub exe: Option<PathBuf>,
}

impl Client {
    pub fn addons_dir(&self) -> PathBuf {
        self.folder.join("Interface").join("AddOns")
    }

    pub fn folder_name(&self) -> String {
        self.folder
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// A short label: `retail (_retail_)`.
    pub fn label(&self) -> String {
        match &self.flavour {
            Some(f) => format!("{f} ({})", self.folder_name()),
            None => self.folder_name(),
        }
    }
}

/// Selu flavour for a client folder name. Test realms map to their base
/// flavour; unknown folders return `None` and can be set by the user.
pub fn flavour_of(folder: &str) -> Option<&'static str> {
    let f = folder.trim_matches('_').to_ascii_lowercase();
    let base = f
        .trim_end_matches("_ptr")
        .trim_end_matches("_beta")
        .trim_end_matches("_xptr");
    match base {
        "retail" | "ptr" | "xptr" | "beta" => Some("retail"),
        "forever" => Some("forever"),
        "classic" => Some("classic"),
        "classic_era" | "era" => Some("classic-era"),
        "anniversary" | "classic_anniversary" => Some("classic-anniversary"),
        "titan" | "classic_titan" => Some("classic-titan"),
        _ => None,
    }
}

/// Battle.net's game id for a client folder, as Battle.net lists its
/// installed games (`_retail_` -> `wow`, `_classic_era_` -> `wow_classic_era`,
/// `_anniversary_` -> `wow_classic_anniversary`).
pub fn product_code(folder: &str) -> Option<String> {
    let f = folder.trim_matches('_').to_ascii_lowercase();
    if f.is_empty() {
        return None;
    }
    Some(match f.as_str() {
        "retail" => "wow".into(),
        // Not in Battle.net's list on the owner's PC; the install's
        // .build.info names the product `wowf`.
        "forever" => "wowf".into(),
        "ptr" => "wowt".into(),
        "xptr" => "wowxptr".into(),
        "beta" => "wow_beta".into(),
        "anniversary" => "wow_classic_anniversary".into(),
        other => format!("wow_{other}"),
    })
}

/// The code Battle.net's `--exec="launch <code>"` takes for a client
/// folder. Sources: Blizzard's forum and Lutris' installers use `WoW` for
/// retail, `WoWC` for Classic and `WoWF` for Forever. Battle.net groups
/// Classic Era, Progression and Anniversary under one Classic entry, so
/// `WoWC` starts whichever Classic version is selected in its dropdown.
/// Test realms have no known code.
pub fn launch_code(folder: &str) -> Option<&'static str> {
    match flavour_of(folder)? {
        _ if folder.contains("ptr") || folder.contains("beta") => None,
        "retail" => Some("WoW"),
        "forever" => Some("WoWF"),
        "classic" | "classic-era" | "classic-anniversary" | "classic-titan" => Some("WoWC"),
        _ => None,
    }
}

/// How Run in Game starts the client.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchMode {
    /// `Battle.net.exe --exec="launch <id>"`: Battle.net starts the game,
    /// logged in.
    #[default]
    Battlenet,
    /// `battlenet://game/<id>`: only selects the game in Battle.net.
    BattlenetPage,
    /// The game's executable, without Battle.net's login.
    Direct,
}

impl LaunchMode {
    pub fn label(self) -> &'static str {
        match self {
            LaunchMode::Battlenet => "Start the game through Battle.net (logged in)",
            LaunchMode::BattlenetPage => "Only open the game in Battle.net",
            LaunchMode::Direct => "Start the game's executable directly (no login)",
        }
    }
}

/// Battle.net's executable, from its registered `battlenet://` handler
/// (read-only `reg query`), else the default install location.
pub fn battlenet_exe() -> Option<PathBuf> {
    if let Ok(out) = std::process::Command::new("reg")
        .args(["query", r"HKCR\battlenet\shell\open\command", "/ve"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(rest) = line.trim().split("REG_SZ").nth(1) {
                let rest = rest.trim();
                let exe = match rest.strip_prefix('"') {
                    Some(r) => r.split('"').next().unwrap_or(""),
                    None => rest.split_whitespace().next().unwrap_or(""),
                };
                let p = PathBuf::from(exe);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    [
        r"C:\Program Files (x86)\Battle.net\Battle.net.exe",
        r"C:\Program Files\Battle.net\Battle.net.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// User settings for game testing, kept in `%APPDATA%\Selu\studio.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Install folders added by hand.
    #[serde(default)]
    pub extra_roots: Vec<PathBuf>,
    /// Flavour overrides: client folder path -> flavour.
    #[serde(default)]
    pub flavours: BTreeMap<String, String>,
    /// The client last used for each flavour: flavour -> client folder.
    #[serde(default)]
    pub chosen: BTreeMap<String, String>,
    /// How Run in Game starts the client.
    #[serde(default)]
    pub launch: LaunchMode,
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Selu").join("studio.toml"))
}

impl Settings {
    pub fn load() -> Settings {
        settings_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path().ok_or("no APPDATA folder")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    }
}

/// Install folders from the registry (read-only `reg query`).
fn registry_roots() -> Vec<PathBuf> {
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
        let Ok(output) = std::process::Command::new("reg").args(args).output() else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
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
fn common_roots() -> Vec<PathBuf> {
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

/// The clients inside one install folder.
pub fn clients_in(root: &Path, settings: &Settings) -> Vec<Client> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let folder = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !folder.is_dir() || !(name.starts_with('_') && name.ends_with('_')) {
            continue;
        }
        let exe = std::fs::read_dir(&folder).ok().and_then(|files| {
            files.flatten().map(|f| f.path()).find(|p| {
                p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy().to_ascii_lowercase();
                    n.starts_with("wow") && n.ends_with(".exe")
                })
            })
        });
        let key = folder.to_string_lossy().into_owned();
        let flavour = settings
            .flavours
            .get(&key)
            .cloned()
            .or_else(|| flavour_of(&name).map(String::from));
        out.push(Client {
            folder,
            flavour,
            exe,
        });
    }
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

/// Every client of every install found on this PC.
pub fn detect(settings: &Settings) -> Vec<Client> {
    let mut roots: Vec<PathBuf> = registry_roots();
    roots.extend(common_roots());
    roots.extend(settings.extra_roots.iter().cloned());
    // The same install can be reached by several names (for example a
    // substituted drive letter); compare canonical paths.
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for root in roots {
        let canonical = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        if !seen.insert(canonical.to_string_lossy().to_lowercase()) {
            continue;
        }
        out.extend(clients_in(&root, settings));
    }
    out
}

/// The client to use for `flavour`: the user's last choice, else the first
/// non-test client with an executable.
pub fn pick<'a>(clients: &'a [Client], flavour: &str, settings: &Settings) -> Option<&'a Client> {
    let matching: Vec<&Client> = clients
        .iter()
        .filter(|c| c.flavour.as_deref() == Some(flavour))
        .collect();
    if let Some(chosen) = settings.chosen.get(flavour)
        && let Some(c) = matching
            .iter()
            .find(|c| c.folder.to_string_lossy() == *chosen)
    {
        return Some(c);
    }
    let test = |c: &&Client| {
        let n = c.folder_name();
        n.contains("ptr") || n.contains("beta")
    };
    matching
        .iter()
        .find(|c| c.exe.is_some() && !test(c))
        .or_else(|| matching.iter().find(|c| c.exe.is_some()))
        .or_else(|| matching.first())
        .copied()
}

#[derive(Debug, PartialEq)]
pub enum CopyError {
    /// An addon folder of that name exists and Studio didn't create it.
    NotOurs(PathBuf),
    Io(String),
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()))?;
        } else {
            std::fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Copy the built addon folder `built` into the client's AddOns folder.
/// `force` replaces a folder Studio didn't create (after the user agreed).
/// Returns the destination.
pub fn install_addon(built: &Path, client: &Client, force: bool) -> Result<PathBuf, CopyError> {
    let name = built
        .file_name()
        .ok_or_else(|| CopyError::Io("the built folder has no name".into()))?;
    let addons = client.addons_dir();
    std::fs::create_dir_all(&addons).map_err(|e| CopyError::Io(e.to_string()))?;
    let dest = addons.join(name);
    if dest.exists() {
        if !dest.join(MARKER).is_file() && !force {
            return Err(CopyError::NotOurs(dest));
        }
        trash::delete(&dest)
            .map_err(|e| CopyError::Io(format!("cannot replace {}: {e}", dest.display())))?;
    }
    copy_dir(built, &dest).map_err(|e| CopyError::Io(e.to_string()))?;
    let marker =
        "Copied here by Selu Studio's Run in Game. Studio replaces this folder on the next run.\n";
    std::fs::write(dest.join(MARKER), marker).map_err(|e| CopyError::Io(e.to_string()))?;
    Ok(dest)
}

/// Is this client's game running? (`tasklist`, read-only.)
pub fn is_running(client: &Client) -> bool {
    let Some(exe) = client.exe.as_ref().and_then(|e| e.file_name()) else {
        return false;
    };
    let exe = exe.to_string_lossy();
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains(&exe.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

/// Start the client the chosen way. The game opens its own window; Studio
/// doesn't wait. Returns what was done, for the console.
pub fn launch(client: &Client, mode: LaunchMode) -> Result<String, String> {
    let folder = client.folder_name();
    if mode != LaunchMode::Direct
        && let Some(bnet) = battlenet_exe()
    {
        let mut cmd = std::process::Command::new(&bnet);
        let what = match mode {
            LaunchMode::Battlenet => {
                let Some(code) = launch_code(&folder) else {
                    return launch_exe(client).map(|()| {
                        format!(
                            "started {} directly (Battle.net has no launch code for this client)",
                            client.label()
                        )
                    });
                };
                // Battle.net reads the raw command line and wants exactly
                // --exec="launch WoW"; normal argument quoting would wrap
                // the whole argument instead.
                exec_arg(&mut cmd, code);
                let note = if code == "WoWC" {
                    "; Battle.net starts the Classic version selected in its Classic dropdown"
                } else {
                    ""
                };
                format!(
                    "asked Battle.net to start {} (launch {code}){note}",
                    client.label()
                )
            }
            _ => {
                let id = product_code(&folder).unwrap_or_else(|| "wow".into());
                cmd.arg(format!("--uri=battlenet://game/{id}"));
                format!("opened {} in Battle.net", client.label())
            }
        };
        return cmd
            .spawn()
            .map(|_| what)
            .map_err(|e| format!("cannot start Battle.net ({}): {e}", bnet.display()));
    }
    launch_exe(client).map(|()| format!("started {} directly", client.label()))
}

#[cfg(windows)]
fn exec_arg(cmd: &mut std::process::Command, code: &str) {
    use std::os::windows::process::CommandExt;
    cmd.raw_arg(format!("--exec=\"launch {code}\""));
}

#[cfg(not(windows))]
fn exec_arg(cmd: &mut std::process::Command, code: &str) {
    cmd.arg(format!("--exec=launch {code}"));
}

/// Start the client's executable directly (no Battle.net login).
fn launch_exe(client: &Client) -> Result<(), String> {
    let exe = client
        .exe
        .as_ref()
        .ok_or_else(|| format!("no game executable in {}", client.folder.display()))?;
    std::process::Command::new(exe)
        .current_dir(&client.folder)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))
}

/// The built addon folder for `flavour` under the project's `dist/`: the
/// only one, or the `<Name>-<flavour>` one when the project builds several.
pub fn built_folder(project: &Path, flavour: &str) -> Option<PathBuf> {
    let dist = project.join("dist");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dist)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && p.join(format!("{n}.toc")).is_file())
        })
        .collect();
    if found.len() > 1 {
        found.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&format!("-{flavour}")))
        });
    }
    found.sort();
    found.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_codes_from_folder_names() {
        assert_eq!(launch_code("_retail_"), Some("WoW"));
        assert_eq!(launch_code("_forever_"), Some("WoWF"));
        assert_eq!(launch_code("_classic_"), Some("WoWC"));
        assert_eq!(launch_code("_classic_era_"), Some("WoWC"));
        assert_eq!(launch_code("_anniversary_"), Some("WoWC"));
        assert_eq!(launch_code("_ptr_"), None);
        assert_eq!(launch_code("_classic_beta_"), None);
    }

    #[test]
    fn product_codes_from_folder_names() {
        // As Battle.net lists its installed games.
        assert_eq!(product_code("_retail_").as_deref(), Some("wow"));
        assert_eq!(product_code("_classic_").as_deref(), Some("wow_classic"));
        assert_eq!(
            product_code("_classic_era_").as_deref(),
            Some("wow_classic_era")
        );
        assert_eq!(
            product_code("_anniversary_").as_deref(),
            Some("wow_classic_anniversary")
        );
        assert_eq!(product_code("_forever_").as_deref(), Some("wowf"));
        assert_eq!(product_code("_ptr_").as_deref(), Some("wowt"));
        assert_eq!(
            product_code("_classic_beta_").as_deref(),
            Some("wow_classic_beta")
        );
    }

    #[test]
    fn flavours_from_folder_names() {
        assert_eq!(flavour_of("_retail_"), Some("retail"));
        assert_eq!(flavour_of("_ptr_"), Some("retail"));
        assert_eq!(flavour_of("_classic_"), Some("classic"));
        assert_eq!(flavour_of("_classic_era_"), Some("classic-era"));
        assert_eq!(flavour_of("_classic_era_ptr_"), Some("classic-era"));
        assert_eq!(flavour_of("_anniversary_"), Some("classic-anniversary"));
        assert_eq!(flavour_of("_classic_beta_"), Some("classic"));
        assert_eq!(flavour_of("_forever_"), Some("forever"));
        assert_eq!(flavour_of("_something_new_"), None);
    }

    fn fake_install(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("selu-game-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (folder, exe) in [
            ("_retail_", "Wow.exe"),
            ("_classic_era_", "WowClassic.exe"),
            ("_ptr_", "WowT.exe"),
        ] {
            std::fs::create_dir_all(root.join(folder).join("Interface").join("AddOns")).unwrap();
            std::fs::write(root.join(folder).join(exe), "").unwrap();
        }
        std::fs::create_dir_all(root.join("Data")).unwrap();
        root
    }

    #[test]
    fn finds_clients_and_picks_the_live_one() {
        let root = fake_install("clients");
        let mut settings = Settings::default();
        let clients = clients_in(&root, &settings);
        assert_eq!(clients.len(), 3, "{clients:?}");
        let retail = pick(&clients, "retail", &settings).unwrap();
        assert_eq!(
            retail.folder_name(),
            "_retail_",
            "live client before the test realm"
        );
        assert!(retail.exe.as_ref().unwrap().ends_with("Wow.exe"));
        settings.chosen.insert(
            "retail".into(),
            root.join("_ptr_").to_string_lossy().into_owned(),
        );
        assert_eq!(
            pick(&clients, "retail", &settings).unwrap().folder_name(),
            "_ptr_"
        );
        assert!(pick(&clients, "forever", &settings).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copying_respects_folders_studio_did_not_create() {
        let root = fake_install("copy");
        let client = clients_in(&root, &Settings::default())
            .into_iter()
            .find(|c| c.folder_name() == "_retail_")
            .unwrap();
        let built = root.join("build").join("Demo");
        std::fs::create_dir_all(&built).unwrap();
        std::fs::write(built.join("Demo.toc"), "## Title: Demo\nmain.lua\n").unwrap();
        std::fs::write(built.join("main.lua"), "print(1)").unwrap();
        // Someone's own addon with the same name.
        let dest = client.addons_dir().join("Demo");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("Demo.toc"), "mine").unwrap();
        assert_eq!(
            install_addon(&built, &client, false),
            Err(CopyError::NotOurs(dest.clone()))
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("Demo.toc")).unwrap(),
            "mine",
            "untouched"
        );
        // A first copy into an empty slot, then a replacement of our own copy.
        std::fs::remove_dir_all(&dest).unwrap();
        assert_eq!(install_addon(&built, &client, false).unwrap(), dest);
        assert!(dest.join(MARKER).is_file());
        std::fs::write(built.join("main.lua"), "print(2)").unwrap();
        install_addon(&built, &client, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("main.lua")).unwrap(),
            "print(2)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
