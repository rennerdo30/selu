//! Where a session reads Blizzard's interface code: the product's client
//! folder inside an install root, or a source checkout on the product's
//! branch. Shared by Studio and `selu test`; never runs git.
use std::path::{Path, PathBuf};

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
