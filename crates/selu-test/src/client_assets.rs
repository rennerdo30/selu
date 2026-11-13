use serde::Deserialize;
use std::path::PathBuf;
#[derive(Default, Deserialize)]
#[serde(default)]
struct Settings {
    disable_client_assets: bool,
    client_install: Option<PathBuf>,
    extra_roots: Vec<PathBuf>,
    default_ui_source: Option<PathBuf>,
    core_ui_addons: Option<Vec<String>>,
}
fn settings() -> Settings {
    std::env::var_os("APPDATA")
        .and_then(|p| std::fs::read_to_string(PathBuf::from(p).join("Selu/studio.toml")).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// A Blizzard UI source folder and, for Core, the addon roots to load.
pub type UiSource = (PathBuf, Option<Vec<String>>);

/// Where Blizzard's UI for `target` is read, and the Core roots, using the
/// same lookup and Studio settings as Studio. `Err` explains why there is none.
pub fn ui_source(options: &crate::RunOptions, target: &str) -> Result<Option<UiSource>, String> {
    if options.ui == crate::TestUi::None {
        return Ok(None);
    }
    let settings = settings();
    let install = resolve(options, target);
    let (source, skipped) = selu_sim::ui_source::ui_source(
        settings.default_ui_source.as_deref(),
        install.as_deref(),
        target,
    );
    let Some(source) = source else {
        return Err(skipped.unwrap_or_else(|| "no Blizzard UI source".into()));
    };
    let roots = (options.ui == crate::TestUi::Core).then(|| {
        settings.core_ui_addons.unwrap_or_else(|| {
            selu_sim::loader::CORE_UI_ADDONS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        })
    });
    Ok(Some((source, roots)))
}
/// The installed client a build reads game files from: the Studio setting,
/// else the first discovered installation with this product. Off when
/// client assets are disabled or `SELU_NO_CLIENT` is set.
pub fn build_install(target: &str) -> Option<PathBuf> {
    if std::env::var_os("SELU_NO_CLIENT").is_some() {
        return None;
    }
    let settings = settings();
    if settings.disable_client_assets {
        return None;
    }
    if let Some(root) = settings.client_install {
        return Some(root);
    }
    let product = target.split('@').next()?;
    let mut roots = selu_gamedata::discovery::registry_roots();
    roots.extend(selu_gamedata::discovery::common_roots());
    roots.extend(settings.extra_roots);
    roots.into_iter().find(|root| {
        std::fs::read_to_string(root.join(".build.info"))
            .ok()
            .is_some_and(|s| {
                s.lines()
                    .skip(1)
                    .any(|line| line.split('|').any(|cell| cell == product))
            })
    })
}

/// Blizzard's global strings (`DPS_TEMPLATE`, `ITEM_MOD_STAMINA_SHORT`...)
/// as typed string globals, named by the user's installed client at build
/// time (owner, 2026-11-13). Nothing is written to the project; names the
/// game data already declares keep their declaration. Returns a note for
/// the build output.
pub fn add_global_strings(manifest: &mut selu_manifest::Manifest) -> Option<String> {
    let product = selu_manifest::product(&manifest.flavour).to_string();
    let root = build_install(&product)?;
    let installed = selu_sim::interface_text::installed_names(&root, &product)?;
    // Own names exist on this client; borrowed ones may not (`string?`).
    let ty = if installed.borrowed_from.is_some() {
        "string?"
    } else {
        "string"
    };
    let known: std::collections::HashSet<String> =
        manifest.globals.iter().map(|g| g.name.clone()).collect();
    let mut added = 0;
    for name in installed.names {
        if known.contains(&name) || manifest.function(&name).is_some() {
            continue;
        }
        manifest.globals.push(selu_manifest::GlobalEntry {
            value: None,
            templates: Vec::new(),
            ty: Some(ty.into()),
            name,
            evidence: selu_manifest::Evidence::Verified,
        });
        added += 1;
    }
    Some(match installed.borrowed_from {
        None => format!(
            "global strings: {added} names from the installed {product} {}",
            installed.version
        ),
        Some(donor) => format!(
            "global strings: {added} names (may be absent: string?) from {donor}; the installed {product} {} table is unreadable",
            installed.version
        ),
    })
}
pub fn resolve(options: &crate::RunOptions, target: &str) -> Option<PathBuf> {
    if options.no_client_assets {
        return None;
    }
    let settings = settings();
    if settings.disable_client_assets {
        return None;
    }
    if let Some(root) = &options.client_install {
        return Some(root.clone());
    }
    if let Some(root) = settings.client_install {
        return Some(root);
    }
    let product = target.split('@').next()?;
    let mut roots = selu_gamedata::discovery::registry_roots();
    roots.extend(selu_gamedata::discovery::common_roots());
    roots.extend(settings.extra_roots);
    roots.into_iter().find(|root| {
        std::fs::read_to_string(root.join(".build.info"))
            .ok()
            .is_some_and(|s| {
                s.lines()
                    .skip(1)
                    .any(|line| line.split('|').any(|cell| cell == product))
            })
    })
}
