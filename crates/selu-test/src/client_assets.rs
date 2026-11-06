use serde::Deserialize;
use std::path::PathBuf;
#[derive(Default, Deserialize)]
#[serde(default)]
struct Settings {
    disable_client_assets: bool,
    client_install: Option<PathBuf>,
    extra_roots: Vec<PathBuf>,
}
pub fn resolve(options: &crate::RunOptions, target: &str) -> Option<PathBuf> {
    if options.no_client_assets {
        return None;
    }
    let settings: Settings = std::env::var_os("APPDATA")
        .and_then(|p| std::fs::read_to_string(PathBuf::from(p).join("Selu/studio.toml")).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
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
