//! Reading an addon folder through its `.toc` (docs/SIMULATOR.md decisions 5
//! and 18). Owner: package S2.
//!
//! Groundwork (S0): `<Folder>.toc` only, `## Key: Value` metadata, comment
//! and blank lines, and `.lua` files in order. XML files are an explicit
//! "not supported yet" error (decision 18). S2 adds flavour `.toc` files
//! (`Name_Mainline.toc`, ...), `## Dependencies`, `## LoadOnDemand`, and
//! `## SavedVariables` loading and writing.

use std::path::{Path, PathBuf};

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
        self.get(key)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn parse_toc(name: &str, dir: &Path, text: &str) -> Toc {
    let mut metadata = Vec::new();
    let mut files = Vec::new();
    for line in text.lines() {
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
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} is not an addon folder", dir.display()))?
        .to_string();
    let suffix = match target.unwrap_or("").split('@').next().unwrap_or("") {
        "retail" => "Mainline",
        "forever" | "classic-era" => "Vanilla",
        "classic" => "Mists",
        "classic-anniversary" => "TBC",
        "classic-titan" => "Wrath",
        _ => "",
    };
    let flavoured = dir.join(format!("{name}_{suffix}.toc"));
    let path = if !suffix.is_empty() && flavoured.is_file() {
        flavoured
    } else {
        dir.join(format!("{name}.toc"))
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(parse_toc(&name, dir, &text))
}

/// The `(chunk name, source)` pairs to run, in order. The chunk name is the
/// path as the `.toc` lists it, so error positions read `main.lua:3:`.
pub fn read_files(toc: &Toc) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for file in &toc.files {
        let lower = file.to_ascii_lowercase();
        if lower.ends_with(".xml") {
            return Err(format!(
                "{}: XML files are not supported by the simulator yet: {file}",
                toc.name
            ));
        }
        if !lower.ends_with(".lua") {
            return Err(format!(
                "{}: unknown file type in the .toc: {file}",
                toc.name
            ));
        }
        let root = toc.dir.canonicalize().map_err(|e| e.to_string())?;
        let path = toc
            .dir
            .join(file)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&root) {
            return Err(format!("file escapes addon folder: {file}"));
        }
        let source = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        out.push((file.clone(), source));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
