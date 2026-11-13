//! Game namespace reads are checked, and the M8 game data (extraction 28):
//! world map data providers, map canvas reads, the Minimap widget, quest log,
//! quest frame, taxi and group reads (real wowf game data).
use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;

fn manifest() -> Manifest {
    selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wowf@1.60.1.70790",
    )
    .unwrap()
}
fn codes(src: &str, m: &Manifest) -> Vec<String> {
    let p = selu_syntax::parse("m8.selu", src).unwrap();
    let c = check_module_targets_with_options(
        &p,
        m,
        ModuleBindings::default(),
        std::slice::from_ref(m),
        CheckOptions::default(),
    )
    .0;
    c.diagnostics.iter().map(|d| d.code.to_string()).collect()
}

#[test]
fn an_undeclared_member_of_a_game_namespace_is_an_error() {
    let m = manifest();
    for src in [
        "const a = Enum.NoSuchEnum;",
        "const a = Enum.UIMapType.NoSuchValue;",
        "const a = C_Map.NoSuchFunction;",
        "const a = Constants.NoSuchTable;",
    ] {
        assert!(codes(src, &m).contains(&"SELU0310".to_string()), "{src}");
    }
    for src in [
        "const a: number = Enum.UIMapType.Zone;",
        "const f = C_Map.GetMapInfo;",
        "const n: number = Constants.InventoryConstants.NumCharacterBankSlots;",
    ] {
        assert!(codes(src, &m).is_empty(), "{src}: {:?}", codes(src, &m));
    }
    // A local of the same name is the program's own value.
    assert!(codes("const Enum = { a: 1 }; const n: number = Enum.a;", &m).is_empty());
}
