//! Reading an addon folder through its `.toc` (docs/SIMULATOR.md decisions 5
//! and 18). Owner: package S2.
//!
//! Groundwork (S0): `<Folder>.toc` only, `## Key: Value` metadata, comment
//! and blank lines, and `.lua` files in order. XML files are an explicit
//! "not supported yet" error (decision 18). S2 adds flavour `.toc` files
//! (`Name_Mainline.toc`, ...), `## Dependencies`, `## LoadOnDemand`, and
//! `## SavedVariables` loading and writing.

use std::path::{Path, PathBuf};

pub(crate) fn read_ui_text(
    path: &Path,
    client: Option<(&selu_gamedata::Storage, &Path)>,
    diagnostic: &mut impl FnMut(String),
) -> Result<String, String> {
    if let Some((storage, source)) = client {
        let relative = path
            .strip_prefix(source)
            .map_err(|_| "UI path escapes source root")?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if !name.starts_with("Interface/") {
            return Err("UI input is outside Interface".into());
        }
        match storage.read_path(&name) {
            Ok(bytes) => return String::from_utf8(bytes).map_err(|e| e.to_string()),
            Err(e) => diagnostic(format!("CASC UI fallback: {name}: {e}")),
        }
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
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
    default_ui_order_client(source, target, None)
}
pub(crate) fn default_ui_order_client(
    source: &Path,
    target: Option<&str>,
    storage: Option<&selu_gamedata::Storage>,
) -> Result<(Vec<Toc>, DefaultUiReport), String> {
    let root = source.canonicalize().map_err(|e| e.to_string())?;
    let mut report = DefaultUiReport::default();
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
        let dir = path
            .parent()
            .ok_or("invalid UI TOC path")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !dir.starts_with(&root) {
            return Err("UI TOC path escapes source".into());
        }
        match read_toc_target_client(&dir, target, client, &mut |n| report.notes.push(n)) {
            Ok(toc) => {
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
    names.sort_by_key(|name| all[name].get("LoadFirst") != Some("1"));
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
    for name in names {
        if all[&name].get("LoadOnDemand") != Some("1") {
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
        for directive in ["AllowLoadGameType", "ExcludeLoadGameType", "AllowLoad"] {
            while let Some(start) = line.find(&format!("[{directive} ")) {
                let Some(end) = line[start..].find(']').map(|i| start + i) else {
                    break;
                };
                if let Some(game) = game {
                    let matches = line[start + directive.len() + 2..end]
                        .split([',', ' '])
                        .any(|v| {
                            let v = v.trim();
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
fn read_toc_target_client(
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
    let suffix = match target.unwrap_or("").split('@').next().unwrap_or("") {
        "retail" => "Mainline",
        "wowf" | "forever" => "Camelot",
        "classic-era" => "Vanilla",
        "classic" => "Mists",
        "classic-anniversary" => "TBC",
        "classic-titan" => "Wrath",
        _ => "",
    };
    let flavoured = dir.join(format!("{name}_{suffix}.toc"));
    let family = dir.join(format!("{name}_Mainline.toc"));
    let path = if !suffix.is_empty() && flavoured.is_file() {
        flavoured
    } else if matches!(suffix, "Camelot" | "Mainline") && family.is_file() {
        family
    } else {
        dir.join(format!("{name}.toc"))
    };
    let text = read_ui_text(&path, client, diagnostic)?;
    let game = match target.unwrap_or("").split('@').next().unwrap_or("") {
        "wowf" | "forever" => Some("camelot"),
        "retail" | "wow" => Some("mainline"),
        "classic-era" => Some("vanilla"),
        _ => None,
    };
    let mut toc = parse_toc_game(&name, dir, &text, game);
    if let Some(game) = game {
        let family = if game == "camelot" || game == "mainline" {
            "Mainline"
        } else {
            "Classic"
        };
        let folder = match game {
            "camelot" => "Camelot",
            "mainline" => "Mainline",
            _ => "Vanilla",
        };
        for file in &mut toc.files {
            *file = file.replace("[Family]", family).replace("[Game]", folder);
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
        let root = toc.dir.canonicalize().map_err(|e| e.to_string())?;
        let listed = toc.dir.join(file);
        let path = match listed.canonicalize() {
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
