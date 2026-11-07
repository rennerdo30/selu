//! Reading an addon folder through its `.toc` (docs/SIMULATOR.md decisions 5
//! and 18). Owner: package S2.
//!
//! Reads flavour-specific TOCs, dependency and load directives, Lua chunks
//! and XML templates. External Blizzard sources prefer configured local CASC.

use std::path::{Path, PathBuf};

/// Real Blizzard addon roots for the lean compatibility environment. Required
/// TOC dependencies and the client's shared-library pass are resolved below.
/// Blizzard addons whose frames addons most often touch. Each product's own
/// `ui-toc-list.txt` and TOCs supply their dependencies and files. Checked
/// Root availability comes from each client's own TOCs. Modern clients split
/// aura, map and tracker services into separate addons; Classic clients keep
/// some of them in FrameXML. Absent built-in roots are noted, never stood in for.
/// Bag buttons and queue status complete implicit action-bar/Edit Mode
/// dependencies; their own TOCs select product-specific files.
/// Combat Log supplies the filter defaults that the chat configuration reads
/// during login, even before its panel is opened.
/// Bootstrap order also supplies implicit dependencies: map mixins before
/// providers, panel registration before settings, unit/raid frames before
/// UIWidgets, and the content tracker before its first login refresh.
pub const CORE_UI_ADDONS: &[&str] = &[
    "Blizzard_AuraContainer",
    "Blizzard_MapCanvas",
    "Blizzard_UIParentPanelManager",
    "Blizzard_UIParent",
    "Blizzard_GameTooltip",
    "Blizzard_Minimap",
    "Blizzard_ChatFrameBase",
    "Blizzard_CombatLog",
    "Blizzard_MainMenuBarBagButtons",
    "Blizzard_QueueStatusFrame",
    "Blizzard_UnitFrame",
    "Blizzard_CompactRaidFrames",
    "Blizzard_SplashFrame",
    "Blizzard_WorldMap",
    "Blizzard_ObjectiveTracker",
    "Blizzard_ContentTracking",
];

pub(crate) fn read_ui_text(
    path: &Path,
    client: Option<(&selu_gamedata::Storage, &Path)>,
    diagnostic: &mut impl FnMut(String),
) -> Result<String, String> {
    let _profile = crate::profile::Span::new("source.read");
    if let Some((storage, source)) = client {
        let relative = path
            .strip_prefix(source)
            .map_err(|_| "UI path escapes source root")?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if !name.starts_with("Interface/") {
            return Err("UI input is outside Interface".into());
        }
        if !storage.uses_user_keys()
            && let Some(text) = crate::ui_source_cache::get(path, false)
        {
            return Ok(text);
        }
        let _casc_profile = crate::profile::Span::new("source.casc");
        match storage.read_path(&name) {
            Ok(bytes) => {
                let text = String::from_utf8(bytes)
                    .map(|text| text.trim_start_matches('\u{feff}').to_owned())
                    .map_err(|e| e.to_string())?;
                if !storage.uses_user_keys() {
                    crate::ui_source_cache::put(path, &text, false);
                }
                return Ok(text);
            }
            Err(e) => diagnostic(format!("CASC UI fallback: {name}: {e}")),
        }
    }
    if let Some(text) = crate::ui_source_cache::get(path, true) {
        return Ok(text);
    }
    let _checkout_profile = crate::profile::Span::new("source.checkout");
    let text = std::fs::read_to_string(path)
        .map(|text| text.trim_start_matches('\u{feff}').to_owned())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    crate::ui_source_cache::put(path, &text, true);
    Ok(text)
}

/// The canonical path of a UI input. A file on disk canonicalizes as usual.
/// With client storage, a path below the source root that exists only inside
/// the client (Blizzard's interface read from CASC, no source checkout) is
/// normalized lexically instead, so the same loader serves every product.
pub(crate) fn ui_path(
    path: &Path,
    client: Option<(&selu_gamedata::Storage, &Path)>,
) -> std::io::Result<std::path::PathBuf> {
    let error = match path.canonicalize() {
        Ok(path) => return Ok(path),
        Err(e) => e,
    };
    let Some((storage, root)) = client else {
        return Err(error);
    };
    let normal = normalize(path);
    let name = normal
        .strip_prefix(root)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .filter(|name| name.starts_with("Interface/"));
    match name {
        Some(name) if storage.resolve(&name).is_ok() => Ok(normal),
        _ => Err(error),
    }
}

/// The canonical folder of an addon: on disk when it exists, else (client
/// storage only) its lexically normalized path. CASC stores files, not
/// folders, so a folder read from the client cannot be looked up itself.
pub(crate) fn ui_dir(
    path: &Path,
    client: Option<(&selu_gamedata::Storage, &Path)>,
) -> std::io::Result<std::path::PathBuf> {
    match path.canonicalize() {
        Err(_) if client.is_some_and(|(_, root)| normalize(path).starts_with(root)) => {
            Ok(normalize(path))
        }
        other => other,
    }
}

fn normalize(path: &Path) -> std::path::PathBuf {
    let mut normal = std::path::PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

#[derive(Debug, Default, Clone)]
pub struct DefaultUiReport {
    pub addons_loaded: usize,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub elapsed: std::time::Duration,
}

/// Preserve the source list's order, promoting LoadFirst roots and walking
/// dependencies before dependants. LoadOnDemand addons load only as dependencies.
pub fn default_ui_order(
    source: &Path,
    target: Option<&str>,
) -> Result<(Vec<Toc>, DefaultUiReport), String> {
    default_ui_order_client(source, target, None, None)
}
pub(crate) fn default_ui_order_client(
    source: &Path,
    target: Option<&str>,
    storage: Option<&selu_gamedata::Storage>,
    roots: Option<&[String]>,
) -> Result<(Vec<Toc>, DefaultUiReport), String> {
    let root = source.canonicalize().map_err(|e| e.to_string())?;
    let mut report = DefaultUiReport::default();
    if let Some(roots) = roots {
        report
            .notes
            .push(format!("Blizzard Core roots: {}", roots.join(", ")));
    }
    let client = storage.map(|s| (s, root.as_path()));
    let list = read_ui_text(&root.join("Interface/ui-toc-list.txt"), client, &mut |n| {
        report.notes.push(n)
    })?;
    let mut all = std::collections::BTreeMap::new();
    let mut names = Vec::new();
    for row in list
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let path = root.join(row.replace('\\', "/"));
        let dir = path.parent().ok_or("invalid UI TOC path")?.to_path_buf();
        if !dir.starts_with(&root) || row.split(['/', '\\']).any(|part| part == "..") {
            return Err("UI TOC path escapes source".into());
        }
        match read_toc_target_client(&dir, target, client, &mut |n| report.notes.push(n)) {
            Ok(toc) => {
                let game = target
                    .and_then(selu_manifest::client_ui_for_target)
                    .map_or("", |ui| ui.game_type);
                if !toc.allowed_game(game) {
                    continue;
                }
                if toc
                    .get("AllowLoad")
                    .is_some_and(|v| v.eq_ignore_ascii_case("Glue"))
                {
                    continue;
                }
                names.push(toc.name.clone());
                all.insert(toc.name.clone(), toc);
            }
            Err(e) => report.errors.push(e),
        }
    }
    // Secure-environment libraries are loaded in the client's secure pass
    // before global LoadFirst consumers instantiate their intrinsic widgets.
    names.sort_by_key(|name| {
        (
            all[name].get("UseSecureEnvironment") != Some("1"),
            all[name].get("LoadFirst") != Some("1"),
        )
    });
    fn visit(
        name: &str,
        all: &std::collections::BTreeMap<String, Toc>,
        active: &mut std::collections::HashSet<String>,
        done: &mut std::collections::HashSet<String>,
        out: &mut Vec<Toc>,
        errors: &mut Vec<String>,
    ) {
        if done.contains(name) {
            return;
        }
        let Some(toc) = all.get(name) else {
            errors.push(format!("missing Blizzard dependency: {name}"));
            return;
        };
        if !active.insert(name.into()) {
            errors.push(format!("cyclic Blizzard dependency: {name}"));
            return;
        }
        for key in ["Dep", "Dependencies", "RequiredDeps", "RequiredDep"] {
            for dep in toc.list(key) {
                visit(&dep, all, active, done, out, errors);
            }
        }
        active.remove(name);
        done.insert(name.into());
        out.push(toc.clone());
    }
    let mut active = std::collections::HashSet::new();
    let mut done = std::collections::HashSet::new();
    let mut out = Vec::new();
    // These shared libraries already exist when the native client enters its
    // game addon pass. LoadFirst addons consume their globals without listing
    // an explicit dependency (for example SecondsFormatterMixin).
    for root in [
        "Blizzard_SharedXMLBase",
        "Blizzard_SharedXML",
        "Blizzard_SharedXMLGame",
    ] {
        if all.contains_key(root) {
            visit(
                root,
                &all,
                &mut active,
                &mut done,
                &mut out,
                &mut report.errors,
            );
        }
    }
    // A partial environment has explicit bootstrap roots: retain their order
    // for implicit dependencies that the complete game pass normally supplies.
    // Required TOC dependencies still precede each root; secure roots go first.
    if let Some(roots) = roots {
        names = roots
            .iter()
            .filter(|n| all.contains_key(*n))
            .cloned()
            .collect();
        names.sort_by_key(|n| all[n].get("UseSecureEnvironment") != Some("1"));
    }
    for name in names {
        if roots.map_or_else(
            || all[&name].get("LoadOnDemand") != Some("1"),
            |roots| roots.contains(&name),
        ) {
            visit(
                &name,
                &all,
                &mut active,
                &mut done,
                &mut out,
                &mut report.errors,
            );
        }
    }
    if let Some(roots) = roots {
        for name in roots {
            if !all.contains_key(name) {
                let message = format!("missing Blizzard Core root for this client: {name}");
                if CORE_UI_ADDONS.contains(&name.as_str()) {
                    // Split addon services differ between client families.
                    // Keep the unavailable root visible without manufacturing
                    // its UI or reporting a file-load failure for an addon
                    // this client does not ship. Custom roots remain errors.
                    report.notes.push(message);
                } else {
                    report.errors.push(message);
                }
            }
        }
    }
    Ok((out, report))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toc {
    /// The addon name: the folder name, as the client uses it.
    pub name: String,
    pub dir: PathBuf,
    /// `## Key: Value` lines in order, keys as written.
    pub metadata: Vec<(String, String)>,
    /// Files in load order, with forward slashes, relative to `dir`.
    pub files: Vec<String>,
}

impl Toc {
    fn allowed_game(&self, game: &str) -> bool {
        if game.is_empty() {
            return true;
        }
        let matches = |token: &str| {
            token.eq_ignore_ascii_case(game)
                || (token.eq_ignore_ascii_case("mainline")
                    && matches!(game, "mainline" | "camelot"))
                || (token.eq_ignore_ascii_case("standard") && game == "mainline")
                || (token.eq_ignore_ascii_case("classic")
                    && !matches!(game, "mainline" | "camelot"))
        };
        (self.get("AllowLoadGameType").is_none()
            || self
                .list("AllowLoadGameType")
                .iter()
                .any(|token| matches(token)))
            && !self
                .list("ExcludeLoadGameType")
                .iter()
                .any(|token| matches(token))
    }
    /// First value for a metadata key, compared case-insensitively.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Names listed in a comma-separated key such as `SavedVariables`.
    pub fn list(&self, key: &str) -> Vec<String> {
        self.metadata
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(key))
            .flat_map(|(_, v)| {
                v.split(|c: char| c == ',' || c.is_whitespace())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .collect()
    }
}

pub fn parse_toc(name: &str, dir: &Path, text: &str) -> Toc {
    parse_toc_game(name, dir, text, None)
}

/// Blizzard's bracket directives apply to metadata as well as file rows.
pub fn parse_toc_game(name: &str, dir: &Path, text: &str, game: Option<&str>) -> Toc {
    let mut metadata = Vec::new();
    let mut files = Vec::new();
    for line in text.lines() {
        let mut line = line.trim().trim_start_matches('\u{feff}').to_string();
        let mut allowed = true;
        for directive in [
            "AllowLoadGameType",
            "ExcludeLoadGameType",
            "AllowLoad",
            "AllowLoadEnvironment",
            "LoadIntoEnvironment",
        ] {
            while let Some(start) = line
                .to_ascii_lowercase()
                .find(&format!("[{directive} ").to_ascii_lowercase())
            {
                let Some(end) = line[start..].find(']').map(|i| start + i) else {
                    break;
                };
                if let Some(game) = game {
                    let matches = line[start + directive.len() + 2..end]
                        .split([',', ' '])
                        .any(|v| {
                            let v = v.trim();
                            if directive == "LoadIntoEnvironment" {
                                return true;
                            }
                            if directive == "AllowLoadEnvironment" {
                                return v.eq_ignore_ascii_case("global");
                            }
                            if directive == "AllowLoad" {
                                return v.eq_ignore_ascii_case("game")
                                    || v.eq_ignore_ascii_case("both");
                            }
                            v == game
                                || (v == "mainline" && game == "camelot")
                                || (v == "classic" && !matches!(game, "mainline" | "camelot"))
                        });
                    allowed &= if directive == "ExcludeLoadGameType" {
                        !matches
                    } else {
                        matches
                    };
                }
                line.replace_range(start..=end, "");
            }
        }
        if !allowed {
            continue;
        }
        let line = line.trim();
        if let Some(meta) = line.strip_prefix("##") {
            if let Some((k, v)) = meta.split_once(':') {
                metadata.push((k.trim().to_string(), v.trim().to_string()));
            }
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else {
            files.push(line.replace('\\', "/"));
        }
    }
    Toc {
        name: name.to_string(),
        dir: dir.to_path_buf(),
        metadata,
        files,
    }
}

pub fn read_toc(dir: &Path) -> Result<Toc, String> {
    read_toc_target(dir, None)
}

pub fn read_toc_target(dir: &Path, target: Option<&str>) -> Result<Toc, String> {
    read_toc_target_client(dir, target, None, &mut |_| {})
}
pub(crate) fn read_toc_target_client(
    dir: &Path,
    target: Option<&str>,
    client: Option<(&selu_gamedata::Storage, &Path)>,
    diagnostic: &mut impl FnMut(String),
) -> Result<Toc, String> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} is not an addon folder", dir.display()))?
        .to_string();
    // The client tries its own TOC suffixes in order, then the plain name.
    let ui = target.and_then(selu_manifest::client_ui_for_target);
    let suffixes = ui.map_or(&[][..], |ui| ui.toc_suffixes);
    let path = suffixes
        .iter()
        .map(|suffix| dir.join(format!("{name}_{suffix}.toc")))
        .find(|path| path.is_file() || (client.is_some() && ui_path(path, client).is_ok()))
        .unwrap_or_else(|| dir.join(format!("{name}.toc")));
    let text = read_ui_text(&path, client, diagnostic)?;
    let mut toc = parse_toc_game(&name, dir, &text, ui.map(|ui| ui.game_type));
    if let Some(ui) = ui {
        for file in &mut toc.files {
            *file = file
                .replace("[Family]", ui.family)
                .replace("[Game]", ui.game_folder);
        }
    }
    // Bindings XML is an implicit addon file, selected by client flavour.
    for binding in suffixes
        .iter()
        .map(|suffix| format!("Bindings_{suffix}.xml"))
        .chain(std::iter::once("Bindings.xml".into()))
    {
        if ui_path(&dir.join(&binding), client).is_ok() {
            if !toc.files.contains(&binding) {
                toc.files.push(binding);
            }
            break;
        }
    }
    Ok(toc)
}

/// The `(chunk name, source)` pairs to run, in order. The chunk name is the
/// path as the `.toc` lists it, so error positions read `main.lua:3:`.
pub fn read_files(toc: &Toc) -> Result<Vec<(String, String)>, String> {
    read_files_with_diagnostics(toc, |_| {})
}

pub fn read_files_with_diagnostics(
    toc: &Toc,
    mut diagnostic: impl FnMut(String),
) -> Result<Vec<(String, String)>, String> {
    read_files_client(toc, &mut diagnostic, None)
}
pub(crate) fn read_files_client(
    toc: &Toc,
    diagnostic: &mut impl FnMut(String),
    client: Option<(&selu_gamedata::Storage, &Path)>,
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for file in &toc.files {
        let root = ui_dir(&toc.dir, client).map_err(|e| format!("{}: {e}", toc.dir.display()))?;
        let listed = toc.dir.join(file);
        let path = match ui_path(&listed, client) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                diagnostic(format!(
                    "missing TOC file: {} (continuing)",
                    listed.display()
                ));
                continue;
            }
            Err(e) => return Err(format!("cannot read {}: {e}", listed.display())),
        };
        if !path.starts_with(&root) {
            return Err(format!("file escapes addon folder: {file}"));
        }
        let lower = file.to_ascii_lowercase();
        if lower.ends_with(".xml") {
            let mut notes = std::collections::HashSet::new();
            crate::xml::read_client(
                &root,
                &path,
                &mut Vec::new(),
                &mut out,
                &mut |note| {
                    if notes.insert(note.clone()) {
                        diagnostic(note);
                    }
                },
                client,
            )?;
            continue;
        }
        if !lower.ends_with(".lua") {
            return Err(format!(
                "{}: unknown file type in the .toc: {file}",
                toc.name
            ));
        }
        let source = read_ui_text(&path, client, diagnostic)?;
        out.push((file.clone(), source));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn core_bootstrap_roots_preserve_order_and_resolve_required_dependencies() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("selu-core-order-{unique}"));
        std::fs::create_dir_all(root.join("Interface/AddOns")).unwrap();
        let mut list = String::new();
        // Client discovery order intentionally differs from bootstrap order.
        for (name, metadata) in [
            ("Consumer", "## Dep: Required\n"),
            ("Required", ""),
            ("Bootstrap", ""),
            ("Secure", "## UseSecureEnvironment: 1\n"),
        ] {
            let dir = root.join("Interface/AddOns").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{name}.toc")), metadata).unwrap();
            list.push_str(&format!("Interface/AddOns/{name}/{name}.toc\n"));
        }
        std::fs::write(root.join("Interface/ui-toc-list.txt"), list).unwrap();
        let roots = ["Bootstrap", "Consumer", "Secure"].map(str::to_owned);
        let (order, report) = default_ui_order_client(&root, None, None, Some(&roots)).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(
            order
                .iter()
                .map(|toc| toc.name.as_str())
                .collect::<Vec<_>>(),
            ["Secure", "Bootstrap", "Required", "Consumer"]
        );
        let missing = [
            "Blizzard_ObjectiveTracker".to_owned(),
            "CustomMissing".to_owned(),
        ];
        let (_, report) = default_ui_order_client(&root, None, None, Some(&missing)).unwrap();
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("Blizzard_ObjectiveTracker"))
        );
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("CustomMissing"));
        // Retain self-written fixture for diagnosis; no recursive deletion.
    }
    #[test]
    fn addon_game_types_filter_whole_packages() {
        let toc = |metadata| parse_toc("Demo", Path::new("Demo"), metadata);
        assert!(!toc("## AllowLoadGameType: plunderstorm\nmain.lua").allowed_game("camelot"));
        assert!(toc("## AllowLoadGameType: mainline\nmain.lua").allowed_game("camelot"));
        assert!(toc("## AllowLoadGameType: camelot\nmain.lua").allowed_game("camelot"));
        assert!(!toc("## ExcludeLoadGameType: camelot\nmain.lua").allowed_game("camelot"));
        assert!(!toc("## AllowLoadGameType: classic\nmain.lua").allowed_game("camelot"));
        assert!(toc("main.lua").allowed_game("camelot"));
    }
    #[test]
    fn forever_toc_fallback_preserves_current_and_legacy_suffixes() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join(format!("selu-toc-{unique}"))
            .join("Demo");
        std::fs::create_dir_all(&dir).unwrap();
        let files = [
            "Demo.toc",
            "Demo_Vanilla.toc",
            "Demo_Mainline.toc",
            "Demo_Camelot.toc",
        ];
        for file in files {
            std::fs::write(dir.join(file), format!("## Title: {file}\n")).unwrap();
        }
        for file in files.into_iter().rev() {
            for flavour in ["wowf", "forever"] {
                let toc = read_toc_target(&dir, Some(&format!("{flavour}@test"))).unwrap();
                assert_eq!(toc.get("Title"), Some(file));
            }
            std::fs::remove_file(dir.join(file)).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
        std::fs::remove_dir(dir.parent().unwrap()).unwrap();
    }
    #[test]
    fn every_product_picks_its_own_toc_and_game_folder() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join(format!("selu-toc-products-{unique}"))
            .join("Demo");
        std::fs::create_dir_all(&dir).unwrap();
        for suffix in [
            "",
            "_Mainline",
            "_Vanilla",
            "_TBC",
            "_Wrath",
            "_Mists",
            "_Classic",
        ] {
            std::fs::write(
                dir.join(format!("Demo{suffix}.toc")),
                format!(
                    "## Title: Demo{suffix}\n[Game]\\a.lua\n[Family]\\b.lua\n\
                     mists.lua [AllowLoadGameType mists]\n\
                     classic.lua [AllowLoadGameType classic]\n"
                ),
            )
            .unwrap();
        }
        for (target, title, game, family, mists, classic) in [
            (
                "wow@12.0.5.1",
                "Demo_Mainline",
                "Mainline",
                "Mainline",
                false,
                false,
            ),
            (
                "wow_classic_era@1.15.8.1",
                "Demo_Vanilla",
                "Vanilla",
                "Classic",
                false,
                true,
            ),
            (
                "wow_anniversary@2.5.5.1",
                "Demo_TBC",
                "TBC",
                "Classic",
                false,
                true,
            ),
            (
                "wow_classic_titan@3.80.0.1",
                "Demo_Wrath",
                "Wrath",
                "Classic",
                false,
                true,
            ),
            (
                "wow_classic@5.5.4.1",
                "Demo_Mists",
                "Mists",
                "Classic",
                true,
                true,
            ),
            (
                "wow_classic@4.4.2.1",
                "Demo_Classic",
                "Cata",
                "Classic",
                false,
                true,
            ),
            (
                "wowf@1.60.1.1",
                "Demo_Mainline",
                "Camelot",
                "Mainline",
                false,
                false,
            ),
            ("unknown@1.0.0.1", "Demo", "[Game]", "[Family]", true, true),
        ] {
            let toc = read_toc_target(&dir, Some(target)).unwrap();
            assert_eq!(toc.get("Title"), Some(title), "{target}");
            assert!(
                toc.files.contains(&format!("{game}/a.lua")),
                "{target}: {:?}",
                toc.files
            );
            assert!(
                toc.files.contains(&format!("{family}/b.lua")),
                "{target}: {:?}",
                toc.files
            );
            assert_eq!(
                toc.files.contains(&"mists.lua".to_string()),
                mists,
                "{target}"
            );
            assert_eq!(
                toc.files.contains(&"classic.lua".to_string()),
                classic,
                "{target}"
            );
        }
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn global_environment_directive_is_not_part_of_the_filename() {
        let toc = parse_toc_game(
            "Demo",
            Path::new("Demo"),
            "Shared\\EditModeManager.lua [AllowLoadEnvironment Global]\n[Game]\\EditModeUtil.lua [AllowLoadGameType camelot] [AllowLoadEnvironment Global]\nsecure.lua [AllowLoadEnvironment Secure]\nNativeFrame.lua [LoadIntoEnvironment secure]\n",
            Some("camelot"),
        );
        assert_eq!(
            toc.files,
            [
                "Shared/EditModeManager.lua",
                "[Game]/EditModeUtil.lua",
                "NativeFrame.lua"
            ]
        );
    }
    #[test]
    fn load_context_filters_files_and_dependencies() {
        let toc = parse_toc_game(
            "Demo",
            Path::new("Demo"),
            "## Dep: GameDep [AllowLoad game]\n## Dep: GlueDep [AllowLoad glue]\ngame.xml [AllowLoad Game]\nglue.xml [AllowLoad Glue]\n",
            Some("camelot"),
        );
        assert_eq!(toc.list("Dep"), ["GameDep"]);
        assert_eq!(toc.files, ["game.xml"]);
    }

    #[test]
    fn blizzard_game_filters_apply_to_files_and_dependencies() {
        let toc = parse_toc_game(
            "Demo",
            Path::new("Demo"),
            "## Dep: A [AllowLoadGameType mainline]\n## Dep: B [AllowLoadGameType classic]\n## LoadFirst: 1\n## LoadOnDemand: 1\n[Game]\\Demo.xml [AllowLoadGameType camelot]\nwrong.lua [ExcludeLoadGameType camelot]\n",
            Some("camelot"),
        );
        assert_eq!(toc.list("Dep"), ["A"]);
        assert_eq!(toc.files, ["[Game]/Demo.xml"]);
        assert_eq!(toc.get("LoadFirst"), Some("1"));
        assert_eq!(toc.get("LoadOnDemand"), Some("1"));
    }

    #[test]
    fn parses_metadata_comments_and_files() {
        let toc = parse_toc(
            "Demo",
            Path::new("Demo"),
            "## Interface: 120001\n## Title: Demo\n# a comment\n\n## SavedVariables: A, B\nlib\\util.lua\nmain.lua\n",
        );
        assert_eq!(toc.get("interface"), Some("120001"));
        assert_eq!(toc.list("SavedVariables"), ["A", "B"]);
        assert_eq!(toc.files, ["lib/util.lua", "main.lua"]);
    }
}
