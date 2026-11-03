use std::path::Path;

fn codes(src: &str, flavour: &str) -> Vec<String> {
    let all = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    let manifest = all
        .iter()
        .find(|m| m.flavour == flavour && !m.fixture)
        .unwrap();
    let program = selu_syntax::parse("api.selu", src).unwrap();
    selu_check::check(&program, manifest)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn documented_argument_types() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes("const f = CreateFrame(42);", flavour),
            vec!["SELU0481"]
        );
    }
}

#[test]
fn results_widgets_events_and_namespaces() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes(
                r#"const f = CreateFrame("Button"); f.SetSize("bad", 10);"#,
                flavour
            ),
            vec!["SELU0481"]
        );
        assert_eq!(
            codes(
                r#"const f = CreateFrame("Button"); f.SetSze(10, 10);"#,
                flavour
            ),
            vec!["SELU0482"]
        );
        assert_eq!(
            codes(r#"const f = CreateFrame("Buton");"#, flavour),
            vec!["SELU0483"]
        );
        assert_eq!(codes("C_NotReal.Nope();", flavour), vec!["SELU0301"]);
        assert_eq!(
            codes("on UNIT_HEALTH(unit: number) {}", flavour),
            vec!["SELU0486"]
        );
        assert_eq!(
            codes("on PLAYER_LOGIN(extra) {}", flavour),
            vec!["SELU0484"]
        );
        assert_eq!(
            codes(
                r#"on UNIT_HEALTH(unit) { const x: number = unit; }"#,
                flavour
            ),
            vec!["SELU0336"]
        );
        assert_eq!(
            codes(
                r#"const name: string = UnitName("player"); const [a,b] = UnitName("player"); const f = UnitName; const n: string = f("player"); const w = CreateFrame("Button"); const t = w.CreateTexture(); t.SetSize(8, 8); const label = w.CreateFontString(); label.SetText(name);"#,
                flavour
            ),
            Vec::<String>::new()
        );
    }
}

fn compile(src: &str, flavour: &str) -> String {
    let all = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    let m = all
        .iter()
        .find(|m| m.flavour == flavour && !m.fixture)
        .unwrap();
    let p = selu_syntax::parse("api.selu", src).unwrap();
    let checked = selu_check::check_program(&p, m);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    selu_lower::lower(&p, m, &checked.types).lua
}

#[test]
fn multiple_results_adjust_in_all_value_positions() {
    let src = r#"
const alias = UnitName;
const first: string = alias("player");
const [name, realm] = alias("player");
print(first, name, realm);
print(UnitName("player"));
print(UnitName("player"), "end");
const names = [UnitName("player")];
print(names.length);
function one() { return UnitName("player"); }
print(one());
function identity(f) { return f; }
const passed = identity(alias);
print(passed("player"));
"#;
    for flavour in ["retail", "forever"] {
        let lua = compile(src, flavour);
        let sim = selu_sim::Sim::new().unwrap();
        sim.load_files(&[
            (
                "stub.lua".into(),
                "function UnitName(unit) return 'Ada', 'Realm' end".into(),
            ),
            ("api.lua".into(), lua.clone()),
        ])
        .unwrap();
        assert_eq!(
            sim.prints(),
            vec!["Ada\tAda\tRealm", "Ada", "Ada\tend", "1", "Ada", "Ada"],
            "{lua}"
        );
    }
}

#[test]
fn structures_enums_callbacks_and_optional_results() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes(
                r#"C_Timer.After(1, () => {}); const info = C_Map.GetMapInfo(1); if (info != null) { const name: string = info.name; } const slots: number = C_Container.GetContainerNumSlots(Enum.BagIndex.Backpack);"#,
                flavour
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            codes(
                r#"const info = C_Map.GetMapInfo(1); const name: string = info.name;"#,
                flavour
            ),
            vec!["SELU0331"]
        );
        assert_eq!(codes(r#"C_Timer.After(1, 42);"#, flavour), vec!["SELU0481"]);
    }
}

#[test]
fn imported_multiple_result_function_keeps_its_convention() {
    use std::collections::BTreeMap;
    let all = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    for m in all.iter().filter(|m| !m.fixture) {
        let base = selu_syntax::parse("base.selu", "export const nameOf = UnitName;").unwrap();
        let (checked, exports) = selu_check::check_module(&base, m, Default::default());
        assert!(checked.diagnostics.is_empty());
        let base_lua = selu_lower::lower_module(
            &base,
            m,
            &checked.types,
            selu_lower::ModuleBindings::new("base".into(), BTreeMap::new(), &base),
        )
        .lua;
        let main=selu_syntax::parse("main.selu", r#"import { nameOf } from "./base"; const name: string = nameOf("player"); const [a,b] = nameOf("player"); print(nameOf("player")); print(a,b);"#).unwrap();
        let imports = BTreeMap::from([("./base".into(), exports.clone())]);
        let (checked, _) = selu_check::check_module(
            &main,
            m,
            selu_check::ModuleBindings {
                imports,
                ..Default::default()
            },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let main_lua = selu_lower::lower_module(
            &main,
            m,
            &checked.types,
            selu_lower::ModuleBindings::new(
                "main".into(),
                BTreeMap::from([("./base".into(), ("base".into(), exports))]),
                &main,
            ),
        )
        .lua;
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("function UnitName() return 'Ada', 'Realm' end")
            .unwrap();
        sim.load_addon(
            "Api",
            &[("base.lua".into(), base_lua), ("main.lua".into(), main_lua)],
        )
        .unwrap();
        assert_eq!(sim.prints(), ["Ada", "Ada\tRealm"]);
    }
}

#[test]
fn conditions_use_the_first_result_including_false() {
    for flavour in ["retail", "forever"] {
        let lua = compile(
            r#"if (UnitExists("player")) { print("wrong"); } else { print("absent"); } const exists = UnitExists; if (!exists("player")) { print("false kept"); } const both: boolean = UnitExists("player") && UnitExists("target"); print(both);"#,
            flavour,
        );
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("function UnitExists() return false, true end")
            .unwrap();
        sim.load_files(&[("condition.lua".into(), lua)]).unwrap();
        assert_eq!(sim.prints(), ["absent", "false kept", "false"]);
    }
}

#[test]
fn primitive_aliases_and_scalar_unions() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes(
                r#"const f = CreateFrame("Frame"); f.SetPoint("TOP", UIParent, "TOP", 0, -40); const health: number = UnitHealth("player"); C_Spell.GetSpellInfo(123); C_Spell.GetSpellInfo("Fireball");"#,
                flavour
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            codes("C_Spell.GetSpellInfo(false);", flavour),
            vec!["SELU0481"]
        );
    }
}

#[test]
fn returned_scalar_unions_require_all_members_to_fit() {
    let all = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    for mut m in all.into_iter().filter(|m| !m.fixture) {
        // Model a documented API returning the same scalar union accepted by GetSpellInfo.
        let sig = m
            .functions
            .iter_mut()
            .find(|f| f.name == "UnitName")
            .unwrap()
            .signature
            .as_mut()
            .unwrap();
        sig.returns[0].ty = Some("SpellIdentifier".into());
        let p=selu_syntax::parse("union.selu",r#"const onlyString: string = UnitName("player"); const onlyNumber: number = UnitName("player"); C_Spell.GetSpellInfo(UnitName("player"));"#).unwrap();
        assert_eq!(
            selu_check::check(&p, &m)
                .iter()
                .map(|d| d.code.as_ref())
                .collect::<Vec<&str>>(),
            ["SELU0336", "SELU0336"]
        );
    }
}

#[test]
fn api_origin_is_diagnostic_only_and_survives_aliases() {
    let all = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    for m in all.iter().filter(|m| !m.fixture) {
        let diagnose = |src: &str| {
            let p = selu_syntax::parse("origin.selu", src).unwrap();
            selu_check::check(&p, m)
        };
        let direct = diagnose("CreateFrame(42);");
        let alias = diagnose("const create = CreateFrame; create(42);");
        assert_eq!(
            direct
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>(),
            alias
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
        let method = diagnose(r#"const f = CreateFrame("Frame"); f.SetSize("bad", 2);"#);
        assert_eq!(method.len(), 1);
        assert_eq!(method[0].code, "SELU0481");
        assert_eq!(
            method[0].message,
            format!(
                "parameter `x` of `SetSize` on {} expects number, got string",
                m.target_string()
            )
        );
        let annotated = diagnose(r#"const nameOf: (unit: string) => string = UnitName;"#);
        assert_eq!(
            annotated
                .iter()
                .map(|d| d.code.as_ref())
                .collect::<Vec<&str>>(),
            ["SELU0485"]
        );
        assert_eq!(
            annotated[0].fix.as_deref(),
            Some("wrap it: `(unit) => UnitName(unit)`")
        );
        assert_eq!(
            diagnose(r#"const tupleFn: (u: string) => [string, string] = UnitName;"#)
                .iter()
                .map(|d| d.code.as_ref())
                .collect::<Vec<&str>>(),
            ["SELU0485"]
        );
        let argument = diagnose(
            r#"function use(cb: (u: string) => string) { print(cb("player")); } use(UnitName);"#,
        );
        assert_eq!(
            argument
                .iter()
                .map(|d| d.code.as_ref())
                .collect::<Vec<&str>>(),
            ["SELU0485"]
        );
        let p = selu_syntax::parse("base.selu", "export const nameOf = UnitName;").unwrap();
        let (_, exports) = selu_check::check_module(&p, m, Default::default());
        let ty = exports["nameOf"].ty.clone();
        let mut erased = ty.clone();
        if let selu_check::Type::Function { api, .. } = &mut erased {
            *api = Default::default();
        }
        assert_eq!(ty, erased);
        assert!(ty.assignable_to(&erased) && erased.assignable_to(&ty));
        assert!(ty.same(&erased));
        assert_eq!(selu_check::Type::join(&ty, &erased), Some(ty));
    }
}

#[test]
fn explicit_wrapper_returns_exactly_one_value() {
    for flavour in ["retail", "forever"] {
        let lua = compile(
            r#"function one(unit: string): string { return UnitName(unit); } global wrapped = one;"#,
            flavour,
        );
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("function UnitName() return 'Ada', 'Realm' end")
            .unwrap();
        sim.load_files(&[("wrapper.lua".into(), lua)]).unwrap();
        sim.eval("print(select('#', wrapped('player')))").unwrap();
        assert_eq!(sim.prints(), ["1"]);
    }
}

#[test]
fn collections_reject_multiple_result_callbacks_and_accept_wrappers() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes(r#"const names = ["player"].map(UnitName);"#, flavour),
            vec!["SELU0485"]
        );
        let lua = compile(
            r#"const names = ["player", "target"].map(u => UnitName(u)); print(names.length, names[0], names[1]);"#,
            flavour,
        );
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("function UnitName(unit) return unit, 'Realm' end")
            .unwrap();
        sim.load_files(&[("map.lua".into(), lua)]).unwrap();
        assert_eq!(sim.prints(), ["2\tplayer\ttarget"]);
    }
}

#[test]
fn unspecified_script_callbacks_accept_functions_and_null() {
    for flavour in ["retail", "forever"] {
        assert_eq!(
            codes(
                r#"const f = CreateFrame("Frame"); f.SetScript("OnShow", (self) => {}); function shown(self: number, flag: boolean): number { return self; } f.SetScript("OnShow", shown); f.SetScript("OnShow", null);"#,
                flavour
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            codes(
                r#"const f = CreateFrame("Frame"); f.SetScript("OnShow", 42);"#,
                flavour
            ),
            vec!["SELU0481"]
        );
    }
}

macro_rules! undefined_api_type_case {
    ($name:ident, $source:expr) => {
        #[test]
        fn $name() {
            assert_eq!(
                [codes($source, "retail"), codes($source, "forever")],
                [Vec::<String>::new(), Vec::<String>::new()]
            );
            for flavour in ["retail", "forever"] {
                compile($source, flavour);
            }
        }
    };
}
undefined_api_type_case!(
    undefined_item_info_accepts_id,
    "const info = C_Item.GetItemInfo(12345);"
);
undefined_api_type_case!(
    undefined_item_info_accepts_link,
    r#"const info = C_Item.GetItemInfo("item:6948");"#
);
undefined_api_type_case!(
    undefined_item_count_accepts_id,
    "const count = C_Item.GetItemCount(6948);"
);
undefined_api_type_case!(
    undefined_club_id_accepts_string,
    r#"const club = C_Club.GetClubInfo("12345");"#
);
