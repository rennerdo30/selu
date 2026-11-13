//! Frozen extraction contracts. Preserve previous entries; a source/overlay change
//! requires a new EXTRACTION_VERSION and a new digest, never replacing the old one.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CONTRACTS: &[(u32, &str)] = &[
    (
        1,
        "65607550cd6a16c24eb6c75bedc4211ab3e71749d9135fb6e0d7c5e52df2fa8c",
    ),
    (
        2,
        "f84cf31b73fe2bbc63e9171ea7bca0efe2de4028ae3e58230d0d7095e1efa8b7",
    ),
    (
        3,
        "354b4541b86c03a5ee4d28660d9135ec365537f965cef033672aa0cf1576aea6",
    ),
    (
        4,
        "cfebb4fabdb14b597b7e1ca9ed4a81859e537ce539ff03a0cc16803362ac9006",
    ),
    (
        5,
        "9c47aa2b30292afb33a54168ad54d8bcad8908ef3c6287e68eacb3bf58d661c6",
    ),
    (
        6,
        "7401a317d8a4cfe87273b452029fe74ba4a1bce636c20003477c82f665758cf8",
    ),
    (
        7,
        "8be8a86335ffdbf4c30f6b99a6665a710a132b0cd3da2d00352f719aadd2722f",
    ),
    (
        8,
        "8ed1136d5e4f71f4c0cd857ceda5c69493c4215e6b6e7970169721e843d4223d",
    ),
    (
        9,
        "d73944069bb36d1bb63f74c0152275d872ac5238a66fb030249d0f036b723a2d",
    ),
    (
        10,
        "5fe7249d85501f6b1708ab450485361a6516b981d045124c159a920eb8ea5a7c",
    ),
    (
        11,
        "ddbf884fc3ad023d5be03491826ae594f579b44adf6329ef115a3c75ea18a4e4",
    ),
    // 12: frames a mixin creates per XML frame (mixin_loops.rs); the digest
    // input now runs up to the product table (the old marker comment is gone).
    (
        12,
        "b6d0a91b476472a10154dcef78b75fe26dbe15b27e8ecde6af3188c7eaf86ec9",
    ),
    // 13: the overlays type GetNumClasses and the colour object's GetRGB/GetRGBA.
    (
        13,
        "a4a0fa80693d771d79374629a5dac0c77e98f8ab4cb930859ba932babd6a765e",
    ),
    // 14: the overlays type the vector's GetXY, InCombatLockdown and UiMapPoint.
    (
        14,
        "39ecbc96e343600764cbd29f6c1e6d5a7ab87ec0cc4f7f9b38f4c7843af2eda5",
    ),
    // 15: keyed children (XML parentKey) per frame, template and nested child;
    // the overlays alias SimpleRegion to Region.
    (
        15,
        "90c02798899cba4e32d00cfae797a4a5322893aee9b0039481351d6412d9f14b",
    ),
    // 16: lists (XML parentArray); reviewed Lua fields of mixins (`[[fields]]`);
    // the overlays type the scroll list, tab system, NineSlice and panel tab
    // accessors and the eight-coordinate SetTexCoord.
    (
        16,
        "8f22d7d25172a72720db83fb9ff0c0fbecbce08ac10907ad9fbf7d3d14fd91b1",
    ),
    // 17: a named frame's own XML mixins follow its templates; the overlays
    // type TooltipData and its post-call, GetActionInfo, GetRaidRosterInfo,
    // GetShapeshiftForm, GetMacroSpell, SetItemButtonTexture and the bag
    // frames' item buttons.
    (
        17,
        "bb8b2f71e574697c6b7d87de8655bbfd898593cabace3d132f3947e89d0a345f",
    ),
    // 18: `update` takes the source clone as `--source`; widget overlays may
    // carry templates (NamePlateFrame with NamePlateBaseMixin); the overlays
    // type the plate's UnitFrame and GetUnit and ItemButtonMixin's
    // GetItemLink and GetBagID, and CompactUnitFrame_UpdateHealthColor.
    (
        18,
        "1557385c8363eb15722981a50618c018abdefc873fbb808578d5b7a52833aa12",
    ),
    // 19: the overlays type the tooltip line colours, GetInventoryItemLink,
    // the quest item reads and the quest reward buttons' `type`, and the
    // profession recipe list rows, output icon and schematic form.
    (
        19,
        "4512da51f2b30ff544fa1b077128f0b3cba3a60e6680669b231660125b942a85",
    ),
    // 20: the overlays type AuraData (C_UnitAuras), IsInRaid,
    // GetNumGroupMembers and GetNumSubgroupMembers, and let
    // GameTooltip:SetText take nil colours with alpha and wrap.
    (
        20,
        "1ef24e8c90487e9765401e585fa777de2131ba674873d5b2e5bef8aac421786b",
    ),
    (
        21,
        "55580fb27c0c90dfb8ac7231650c3502dc7720d799fba192b2bb9f50402e0b95",
    ),
    (
        22,
        "fbe90e98bfee6ed3294ecb988154212007f9934002145586930b582e193af62f",
    ),
];

fn collect(directory: &Path, files: &mut Vec<PathBuf>, accept: fn(&Path) -> bool) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files, accept);
        } else if accept(&path) {
            files.push(path);
        }
    }
}

#[test]
fn extraction_sources_and_overlays_require_version_bump() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    // Discover modules and overlays, so adding a file also changes the contract.
    collect(
        &root.join("crates/selu-manifest-gen/src"),
        &mut files,
        |p| p.extension().is_some_and(|e| e == "rs"),
    );
    collect(&root.join("manifests"), &mut files, |p| {
        p.file_name().is_some_and(|n| n == "overlay.toml")
    });
    files.extend([
        root.join("crates/selu-manifest/src/lib.rs"),
        root.join("crates/selu-manifest/src/format.rs"),
        root.join("crates/selu-manifest/src/resolution.rs"),
        root.join("crates/selu-manifest-gen/Cargo.toml"),
    ]);
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let mut content = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        if name == "crates/selu-manifest-gen/src/main.rs" {
            // CLI imports/reports are not extraction rules. Cover generation and
            // its git/archive helpers, branch identity, and product/overlay setup.
            // Everything before the product table: the git/archive helpers
            // and `generate`.
            let generation = content.split_once("const CLIENTS:").unwrap().0;
            let clients = content
                .split_once("const CLIENTS:")
                .unwrap()
                .1
                .split_once("#[cfg(test)]")
                .unwrap()
                .0;
            let product = content
                .split_once("fn extract_product(")
                .unwrap()
                .1
                .split_once("fn retained_reextractions(")
                .unwrap()
                .0;
            content =
                format!("{generation}\nconst CLIENTS:{clients}\nfn extract_product({product}");
        }
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((content.len() as u64).to_le_bytes());
        hash.update(content.as_bytes());
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let version = selu_manifest_gen::EXTRACTION_VERSION;
    let expected = CONTRACTS
        .iter()
        .find(|(v, _)| *v == version)
        .expect("add a frozen digest for the new EXTRACTION_VERSION")
        .1;
    assert_eq!(
        digest, expected,
        "extraction sources/overlays changed without an extraction version bump; bump EXTRACTION_VERSION and append its digest to CONTRACTS, preserving old entries"
    );
}
