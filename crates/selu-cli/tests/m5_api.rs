//! Documented constants (`Constants.<Table>.<Field>`), intrinsic XML frame
//! types as CreateFrame types, tooltip ownership and frame attribute values
//! (real wowf game data, extraction 22).
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
    let p = selu_syntax::parse("m5.selu", src).unwrap();
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
fn a_documented_constant_is_typed_by_its_table() {
    let m = manifest();
    assert!(codes("const n: number = Constants.InventoryConstants.NumCharacterBankSlots;", &m).is_empty());
    assert!(
        codes("const s: string = Constants.InventoryConstants.NumCharacterBankSlots;", &m)
            .contains(&"SELU0336".to_string())
    );
    // An unknown field of a known constants table is no value.
    assert!(!codes("const n: number = Constants.InventoryConstants.NoSuchField;", &m).is_empty());
}

#[test]
fn an_intrinsic_frame_is_a_frame_type_of_its_own() {
    let m = manifest();
    let src = r#"
        const b = CreateFrame("ItemButton", null, UIParent, "ContainerFrameItemButtonTemplate");
        b.SetBagID(0);
        b.SetHasItem(true);
        b.Cooldown.Clear();
        SetItemButtonCount(b, 3);
    "#;
    let found = codes(src, &m);
    assert!(found.is_empty(), "{found:?}");
    // A name that is no widget and no intrinsic still warns.
    assert!(codes(r#"const f = CreateFrame("NoSuchFrameType");"#, &m).contains(&"SELU0483".to_string()));
}

#[test]
fn tooltip_ownership_and_attribute_values_are_typed() {
    let m = manifest();
    let src = r#"
        const owner: Frame? = GameTooltip.GetOwner();
        const mine: boolean = GameTooltip.IsOwned(UIParent);
        const button = CreateFrame("Button", null, UIParent, "SecureActionButtonTemplate");
        button.SetAttribute("type", "click");
        button.SetAttribute("clickbutton", UIParent);
        button.SetAttribute("checkselfcast", true);
        button.SetAttribute("unit", null);
    "#;
    let found = codes(src, &m);
    assert!(found.is_empty(), "{found:?}");
}
