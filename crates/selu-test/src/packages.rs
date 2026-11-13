//! Packages that ship inside the compiler (docs/lang/packages.md).
//!
//! A package's modules join a project's module graph under the key
//! `@selu/<name>/<path>`; its entry point is `@selu/<name>/index`. The
//! sources are embedded from `<repo>/packages/` at compile time.
//! `SELU_PACKAGES_DIR` replaces the embedded set with a folder on disk,
//! for package authors (no compiler rebuild per edit) and tests.
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/packages.rs"));
}

/// The key prefix of every package module.
pub const PREFIX: &str = "@selu/";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageToml {
    name: String,
    #[allow(dead_code)]
    description: String,
    products: Vec<String>,
}

#[derive(Debug)]
pub struct Package {
    pub products: Vec<String>,
    /// Module path below `src/` without `.selu` -> source.
    modules: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Packages {
    pub packages: BTreeMap<String, Package>,
    /// Problems found while reading the set (bad package.toml, no entry).
    pub errors: Vec<String>,
}

fn read_set(files: Vec<(String, String, String)>) -> Packages {
    let mut grouped = BTreeMap::<String, Vec<(String, String)>>::new();
    for (name, rel, text) in files {
        grouped.entry(name).or_default().push((rel, text));
    }
    let mut set = Packages::default();
    for (name, files) in grouped {
        let Some((_, toml_text)) = files.iter().find(|(rel, _)| rel == "package.toml") else {
            set.errors
                .push(format!("package `{name}` has no package.toml"));
            continue;
        };
        let meta: PackageToml = match toml::from_str(toml_text) {
            Ok(meta) => meta,
            Err(e) => {
                set.errors
                    .push(format!("package `{name}` has an invalid package.toml: {e}"));
                continue;
            }
        };
        if meta.name != name {
            set.errors.push(format!(
                "package folder `{name}` declares the name `{}`",
                meta.name
            ));
            continue;
        }
        let modules: BTreeMap<_, _> = files
            .into_iter()
            .filter_map(|(rel, text)| {
                let path = rel.strip_prefix("src/")?.strip_suffix(".selu")?;
                Some((path.to_string(), text))
            })
            .collect();
        if !modules.contains_key("index") {
            set.errors
                .push(format!("package `{name}` has no src/index.selu"));
            continue;
        }
        set.packages.insert(
            name,
            Package {
                products: meta.products,
                modules,
            },
        );
    }
    set
}

fn read_dir(root: &std::path::Path) -> Vec<(String, String, String)> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                if rel != "tests" {
                    walk(&path, base, out);
                }
            } else if (rel == "package.toml" || (rel.starts_with("src/") && rel.ends_with(".selu")))
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                out.push((rel, text));
            }
        }
    }
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.filter_map(Result::ok) {
        if entry.path().is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut found = Vec::new();
            walk(&entry.path(), &entry.path(), &mut found);
            files.extend(
                found
                    .into_iter()
                    .map(|(rel, text)| (name.clone(), rel, text)),
            );
        }
    }
    files
}

/// The package set for this process: `SELU_PACKAGES_DIR` if set, otherwise
/// the packages embedded in the compiler.
pub fn packages() -> &'static Packages {
    static SET: OnceLock<Packages> = OnceLock::new();
    SET.get_or_init(|| match std::env::var_os("SELU_PACKAGES_DIR") {
        Some(dir) => read_set(read_dir(std::path::Path::new(&dir))),
        None => read_set(
            embedded::EMBEDDED
                .iter()
                .map(|(n, r, t)| (n.to_string(), r.to_string(), t.to_string()))
                .collect(),
        ),
    })
}

/// Whether a module key belongs to a package.
pub fn is_package_key(key: &str) -> bool {
    key.starts_with(PREFIX)
}

/// The package name of a package module key.
pub fn package_of(key: &str) -> Option<&str> {
    key.strip_prefix(PREFIX)?.split('/').next()
}

/// The source of a package module key such as `@selu/ui/button`.
pub fn source(key: &str) -> Option<&'static str> {
    let rest = key.strip_prefix(PREFIX)?;
    let (name, path) = rest.split_once('/')?;
    packages()
        .packages
        .get(name)?
        .modules
        .get(path)
        .map(String::as_str)
}

/// Output path of a module below the built addon folder, without `.lua`.
/// Package modules go under `selu_packages/<name>/`.
pub fn output_key(key: &str) -> String {
    if let Some(name) = key.strip_prefix(selu_syntax::ADDON_IMPORT_PREFIX) {
        return format!("selu_addons/{name}");
    }
    match key.strip_prefix(PREFIX) {
        Some(rest) => format!("selu_packages/{rest}"),
        None => key.to_string(),
    }
}

/// A declared other addon's module (`addons/<Name>.selu`).
pub fn is_addon_key(key: &str) -> bool {
    key.starts_with(selu_syntax::ADDON_IMPORT_PREFIX)
}

/// The source path a source map and diagnostics name for a module.
pub fn source_name(key: &str) -> String {
    if let Some(name) = key.strip_prefix(selu_syntax::ADDON_IMPORT_PREFIX) {
        return format!("addons/{name}.selu");
    }
    if is_package_key(key) {
        format!("{key}.selu")
    } else {
        format!("src/{key}.selu")
    }
}
