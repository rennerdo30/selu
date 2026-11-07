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
    /// TACT product (`wow`, `wow_classic_era`, ...), when known.
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

/// TACT product for a client folder. Test products retain their own identities;
/// unknown folders return `None` and can be set by the user.
pub fn flavour_of(folder: &str) -> Option<&'static str> {
    match folder.trim_matches('_').to_ascii_lowercase().as_str() {
        "retail" => Some("wow"),
        "ptr" => Some("wowt"),
        "xptr" => Some("wowxptr"),
        "beta" => Some("wow_beta"),
        "forever" => Some("wowf"),
        "classic" => Some("wow_classic"),
        "classic_ptr" => Some("wow_classic_ptr"),
        "classic_beta" => Some("wow_classic_beta"),
        "classic_era" => Some("wow_classic_era"),
        "classic_era_ptr" => Some("wow_classic_era_ptr"),
        "anniversary" | "classic_anniversary" => Some("wow_anniversary"),
        "anniversary_ptr" => Some("wow_anniversary_ptr"),
        "titan" | "classic_titan" => Some("wow_classic_titan"),
        _ => None,
    }
}

/// The client folder (`_retail_`, `_forever_`, ...) of a product inside an
/// install root, if that product is installed there.
pub fn product_folder(install: &Path, product: &str) -> Option<PathBuf> {
    let product = selu_manifest::product(product);
    let mut found: Vec<PathBuf> = std::fs::read_dir(install)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name().and_then(|n| n.to_str()).and_then(flavour_of) == Some(product)
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The branch a source checkout has checked out, read from its `.git/HEAD`
/// (a worktree's `.git` file points at its own HEAD). Never runs git.
pub fn checkout_branch(checkout: &Path) -> Option<String> {
    let git = checkout.join(".git");
    let head = if git.is_file() {
        let pointer = std::fs::read_to_string(&git).ok()?;
        let dir = PathBuf::from(pointer.trim().strip_prefix("gitdir:")?.trim());
        let dir = if dir.is_absolute() {
            dir
        } else {
            checkout.join(dir)
        };
        std::fs::read_to_string(dir.join("HEAD")).ok()?
    } else {
        std::fs::read_to_string(git.join("HEAD")).ok()?
    };
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_owned)
}

/// Where a session for `target` reads Blizzard's interface code.
///
/// The product's installed client comes first: its files are read from CASC,
/// so every product works without a checkout. A configured source checkout is
/// used only when the client is not available and the checkout's branch is
/// the product's branch. The second value explains a skipped checkout.
pub fn ui_source(
    configured: Option<&Path>,
    install: Option<&Path>,
    target: &str,
) -> (Option<PathBuf>, Option<String>) {
    let product = selu_manifest::product(target.split('@').next().unwrap_or(""));
    if let Some(folder) = install.and_then(|root| product_folder(root, product)) {
        return (Some(folder), None);
    }
    let Some(checkout) = configured.filter(|p| p.join("Interface/ui-toc-list.txt").is_file())
    else {
        return (
            None,
            Some(format!(
                "no Blizzard UI for {product}: the client is not installed and no source checkout is configured"
            )),
        );
    };
    let wanted = selu_manifest::source_branch(product);
    match (checkout_branch(checkout), wanted) {
        (Some(branch), Some(wanted)) if branch != wanted => (
            None,
            Some(format!(
                "Blizzard UI source {} is on branch {branch}, but {product} needs {wanted}; not loading it",
                checkout.display()
            )),
        ),
        _ => (Some(checkout.to_path_buf()), None),
    }
}

/// Blizzard TACT product for a known client folder.
pub fn product_code(folder: &str) -> Option<String> {
    flavour_of(folder).map(String::from)
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
        "wow" => Some("WoW"),
        "wowf" => Some("WoWF"),
        "wow_classic" | "wow_classic_era" | "wow_anniversary" | "wow_classic_titan" => Some("WoWC"),
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UiMode {
    None,
    #[default]
    Core,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub ui_mode: UiMode,
    #[serde(default = "core_ui_roots")]
    pub core_ui_addons: Vec<String>,
    #[serde(default)]
    pub default_ui_source: Option<PathBuf>,
    #[serde(default)]
    pub decrypt_client_files: bool,
    #[serde(default)]
    pub client_key_file: Option<PathBuf>,
    #[serde(default)]
    pub wtf_override: String,
    #[serde(default)]
    pub saved_choices: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub disable_client_assets: bool,
    #[serde(default)]
    pub client_install: Option<PathBuf>,
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
    /// Projects opened in Studio, newest first.
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
}

fn core_ui_roots() -> Vec<String> {
    selu_sim::loader::CORE_UI_ADDONS
        .iter()
        .map(|s| (*s).into())
        .collect()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_mode: UiMode::Core,
            core_ui_addons: core_ui_roots(),
            default_ui_source: None,
            decrypt_client_files: false,
            client_key_file: None,
            wtf_override: String::new(),
            saved_choices: BTreeMap::new(),
            disable_client_assets: false,
            client_install: None,
            extra_roots: Vec::new(),
            flavours: BTreeMap::new(),
            chosen: BTreeMap::new(),
            launch: LaunchMode::default(),
            recent_projects: Vec::new(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Selu").join("studio.toml"))
}

impl Settings {
    pub fn active_key_file(&self) -> Option<PathBuf> {
        self.decrypt_client_files
            .then(|| self.client_key_file.clone())
            .flatten()
    }
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

use selu_gamedata::discovery::{common_roots, registry_roots};

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
            .map(|f| selu_manifest::product(&f).into())
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
        .filter(|c| c.flavour.as_deref() == Some(selu_manifest::product(flavour)))
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

/// A generated output has one TOC; its basename is the client's addon identity.
fn toc_addon_name(built: &Path) -> Option<std::ffi::OsString> {
    let mut tocs = std::fs::read_dir(built)
        .ok()?
        .collect::<std::io::Result<Vec<_>>>()
        .ok()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("toc"))
        });
    let first = tocs.next()?;
    if tocs.next().is_some() {
        return None;
    }
    first.file_stem().map(std::ffi::OsStr::to_os_string)
}

/// Copy the built addon folder `built` into the client's AddOns folder.
/// `force` replaces a folder Studio didn't create (after the user agreed).
/// Returns the destination.
pub fn install_addon(built: &Path, client: &Client, force: bool) -> Result<PathBuf, CopyError> {
    let name = toc_addon_name(built).ok_or_else(|| {
        CopyError::Io("the built addon must contain exactly one generated TOC".into())
    })?;
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

/// Select the current build layout, including target overrides, without using stale siblings.
pub fn built_folder(
    project: &Path,
    product: &str,
    target_override: Option<&str>,
) -> Option<PathBuf> {
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(project.join("selu.toml")).ok()?).ok()?;
    let addon = config.get("addon")?;
    let name = addon.get("name")?.as_str()?;
    let count = target_override
        .map(|targets| targets.split(',').filter(|t| !t.trim().is_empty()).count())
        .unwrap_or_else(|| {
            addon
                .get("targets")
                .and_then(toml::Value::as_array)
                .map_or(0, Vec::len)
        });
    let shared = config
        .get("build")
        .and_then(|build| build.get("layout"))
        .and_then(toml::Value::as_str)
        == Some("shared");
    let folder = if count > 1 && !shared {
        format!("{name}-{}", selu_manifest::product(product))
    } else {
        name.to_owned()
    };
    let built = project.join("dist").join(folder);
    (toc_addon_name(&built)? == name).then_some(built)
}

/// Version from the install root's pipe-separated Blizzard metadata, read only.
pub fn installed_build(client: &Client) -> Option<selu_manifest::Build> {
    let product = client.flavour.as_deref()?;
    let text = std::fs::read_to_string(client.folder.parent()?.join(".build.info")).ok()?;
    let mut lines = text.lines();
    let columns: Vec<_> = lines
        .next()?
        .split('|')
        .map(|s| s.split('!').next().unwrap_or(s))
        .collect();
    let p = columns.iter().position(|c| *c == "Product")?;
    let v = columns.iter().position(|c| *c == "Version")?;
    let active = columns.iter().position(|c| *c == "Active");
    for line in lines {
        let fields: Vec<_> = line.split('|').collect();
        if fields.get(p).copied() == Some(product)
            && active.is_none_or(|i| fields.get(i).copied() == Some("1"))
        {
            return selu_manifest::Build::parse(fields.get(v)?).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decryption_is_explicitly_opt_in() {
        let mut settings = Settings::default();
        settings.client_key_file = Some("self-written-nonexistent-fixture.keys".into());
        assert!(settings.active_key_file().is_none());
        settings.decrypt_client_files = true;
        assert_eq!(settings.active_key_file(), settings.client_key_file);
        let restored: Settings = serde_json::from_str("{}").unwrap();
        assert!(restored.active_key_file().is_none());
    }

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
            Some("wow_anniversary")
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
        assert_eq!(flavour_of("_retail_"), Some("wow"));
        assert_eq!(flavour_of("_ptr_"), Some("wowt"));
        assert_eq!(flavour_of("_classic_"), Some("wow_classic"));
        assert_eq!(flavour_of("_classic_era_"), Some("wow_classic_era"));
        assert_eq!(flavour_of("_classic_era_ptr_"), Some("wow_classic_era_ptr"));
        assert_eq!(flavour_of("_anniversary_"), Some("wow_anniversary"));
        assert_eq!(flavour_of("_classic_beta_"), Some("wow_classic_beta"));
        assert_eq!(flavour_of("_forever_"), Some("wowf"));
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
    fn ui_source_prefers_the_products_client_then_a_matching_checkout() {
        let root = fake_install("ui-source");
        // Installed products read Blizzard's UI from their own client folder.
        let (source, note) = ui_source(None, Some(&root), "wow@12.0.5.1");
        assert_eq!(source, Some(root.join("_retail_")));
        assert!(note.is_none());
        let (source, _) = ui_source(None, Some(&root), "wow_classic_era@1.15.8.1");
        assert_eq!(source, Some(root.join("_classic_era_")));
        // A checkout is used only when its branch is the product's branch.
        let checkout = root.join("wow-ui-source");
        std::fs::create_dir_all(checkout.join("Interface")).unwrap();
        std::fs::write(checkout.join("Interface/ui-toc-list.txt"), "").unwrap();
        std::fs::create_dir_all(checkout.join(".git")).unwrap();
        std::fs::write(
            checkout.join(".git/HEAD"),
            "ref: refs/heads/forever
",
        )
        .unwrap();
        assert_eq!(checkout_branch(&checkout).as_deref(), Some("forever"));
        let (source, note) = ui_source(Some(&checkout), Some(&root), "wowf@1.60.1.1");
        assert_eq!(
            source,
            Some(checkout.clone()),
            "wowf is not installed in the fake root"
        );
        assert!(note.is_none());
        let (source, note) = ui_source(Some(&checkout), None, "wow_anniversary@2.5.5.1");
        assert_eq!(source, None);
        assert!(note.unwrap().contains("classic_anniversary"));
        let (source, note) = ui_source(None, None, "wow_classic@5.5.4.1");
        assert_eq!(source, None);
        assert!(note.is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn generated_toc_identity_for_all_output_layouts() {
        let root = fake_install("output-layouts");
        let project = root.join("project");
        for folder in ["Demo", "Demo-wow", "Demo-wow_classic"] {
            let output = project.join("dist").join(folder);
            std::fs::create_dir_all(&output).unwrap();
            std::fs::write(
                output.join("Demo.toc"),
                "## Interface: 120105, 50504\nmain.lua\n",
            )
            .unwrap();
            std::fs::write(output.join("main.lua"), folder).unwrap();
        }
        for (layout, product, override_target, expected) in [
            ("single", "wow", None, "Demo"),
            ("per-target", "wow_classic", None, "Demo-wow_classic"),
            ("shared", "wow_classic", None, "Demo"),
            ("override", "wow", Some("wow"), "Demo"),
        ] {
            let targets = if layout == "single" {
                "[\"wow\"]"
            } else {
                "[\"wow\", \"wow_classic\"]"
            };
            let mode = if layout == "shared" {
                "shared"
            } else {
                "per-target"
            };
            std::fs::write(
                project.join("selu.toml"),
                format!("[addon]\nname=\"Demo\"\ntargets={targets}\n[build]\nlayout=\"{mode}\"\n"),
            )
            .unwrap();
            let built = built_folder(&project, product, override_target).unwrap();
            assert_eq!(built.file_name().unwrap(), expected, "{layout}");
            let client = Client {
                folder: root.join("clients").join(layout).join("_client_"),
                flavour: Some(product.into()),
                exe: None,
            };
            let installed = install_addon(&built, &client, false).unwrap();
            assert_eq!(installed, client.addons_dir().join("Demo"), "{layout}");
            assert!(installed.join("Demo.toc").is_file());
            assert_eq!(
                std::fs::read_to_string(installed.join("main.lua")).unwrap(),
                expected
            );
        }
        assert!(
            std::fs::canonicalize(&root)
                .unwrap()
                .starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn installed_version_uses_the_active_product_row() {
        let root = fake_install("metadata");
        std::fs::write(root.join(".build.info"), "Product!STRING:0|Version!STRING:0|Active!DEC:1\nwow|12.1.5.1|0\nwow_classic|5.5.4.3|1\nwow|12.1.5.2|1\n").unwrap();
        let client = clients_in(&root, &Settings::default())
            .into_iter()
            .find(|c| c.folder_name() == "_retail_")
            .unwrap();
        assert_eq!(installed_build(&client).unwrap().build, 2);
        std::fs::remove_dir_all(root).unwrap();
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
            "_retail_"
        );
        assert_eq!(
            pick(&clients, "wowt", &settings).unwrap().folder_name(),
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
