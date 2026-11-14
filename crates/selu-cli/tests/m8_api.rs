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

// A game value the game data gives no type is an error (SELU0360): its
// value would be unchecked. Typed FrameXML tables (extractions 30 and 31)
// pass.
#[test]
fn an_untyped_game_value_is_an_error() {
    let m = manifest();
    for src in [
        "const names = LOCALIZED_CLASS_NAMES_MALE;",
        "const container = WorldMapFrame.GetCanvasContainer();",
    ] {
        assert!(codes(src, &m).contains(&"SELU0360".to_string()), "{src}: {:?}", codes(src, &m));
    }
    for src in [
        "const id: number = SOUNDKIT.RAID_WARNING;",
        "tinsert(UISpecialFrames, \"MyFrame\");",
        "const r: number = QuestDifficultyColors.trivial.r;",
        "const zone: number = Enum.UIMapType.Zone;",
        "const consts: number = Constants.QuestLogConsts.MAXIMUM_NUM_QUESTS_LOG_CAN_ACCEPT;",
    ] {
        assert!(codes(src, &m).is_empty(), "{src}: {:?}", codes(src, &m));
    }
    // A sound kit name the client does not declare is no value.
    assert!(!codes("const id: number = SOUNDKIT.NO_SUCH_SOUND;", &m).is_empty());
}
