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
pub fn ui_source(
    options: &crate::RunOptions,
    target: &str,
) -> Result<Option<UiSource>, String> {
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
