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
            codes("const f = CreateFrame(true);", flavour),
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
                r#"const name: string = UnitName("player") ?? ""; const [a,b] = UnitName("player"); const f = UnitName; const n: string = f("player") ?? ""; const w = CreateFrame("Button"); const t = w.CreateTexture(); t.SetSize(8, 8); const label = w.CreateFontString(); label.SetText(name);"#,
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
    for flavour in [
        "classic-era",
        "classic-anniversary",
        "classic-titan",
        "classic",
    ] {
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
        let main=selu_syntax::parse("main.selu", r#"import { nameOf } from "./base"; const maybeName = readable(nameOf("player")); const name: string = maybeName != null ? maybeName : ""; const [a,b] = nameOf("player"); print(readable(nameOf("player"))); print(readable(a),readable(b));"#).unwrap();
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
                r#"const f = CreateFrame("Frame"); f.SetPoint("TOP", UIParent, "TOP", 0, -40); const health: number = UnitHealth("player") ?? 0; C_Spell.GetSpellInfo(123); C_Spell.GetSpellInfo("Fireball");"#,
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
        let modern = matches!(m.flavour.as_str(), "retail" | "forever");
        let api = if modern { "GetBuildInfo" } else { "UnitName" };
        // Model a documented API returning the same scalar union accepted by GetSpellInfo.
        let sig = m
            .functions
            .iter_mut()
            .find(|f| f.name == api)
            .unwrap()
            .signature
            .as_mut()
            .unwrap();
        sig.returns[0].ty = Some("SpellIdentifier".into());
        let source = r#"const onlyString: string = GetBuildInfo(); const onlyNumber: number = GetBuildInfo(); C_Spell.GetSpellInfo(GetBuildInfo());"#;
        let source = if modern {
            source.to_string()
        } else {
            source.replace("GetBuildInfo()", "UnitName(\"player\")")
        };
        let p = selu_syntax::parse("union.selu", &source).unwrap();
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
        let direct = diagnose("CreateFrame(true);");
        let alias = diagnose("const create = CreateFrame; create(true);");
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
    for flavour in [
        "classic-era",
        "classic-anniversary",
        "classic-titan",
        "classic",
    ] {
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
    for flavour in [
        "classic-era",
        "classic-anniversary",
        "classic-titan",
        "classic",
    ] {
        assert_eq!(
            codes(r#"const names = ["player"].map(UnitName);"#, flavour),
            vec!["SELU0485"]
        );
        let lua = compile(
            r#"global mappedName = (u: string) => UnitName(u); const names = ["player", "target"].map(mappedName); print(names.length, names[0], names[1]);"#,
            flavour,
        );
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("function UnitName(unit) return unit, 'Realm' end")
            .unwrap();
        sim.load_files(&[("map.lua".into(), lua)]).unwrap();
        assert_eq!(sim.prints(), ["2\tplayer\ttarget"]);
        sim.eval("print(select('#', mappedName('player')))")
            .unwrap();
        assert_eq!(sim.prints(), ["2\tplayer\ttarget", "1"]);
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

fn review_manifest(flavour: &str) -> &'static selu_manifest::Manifest {
    static ALL: std::sync::OnceLock<Vec<selu_manifest::Manifest>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        selu_manifest::load_manifest_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
        )
        .unwrap()
    })
    .iter()
    .find(|m| m.flavour == flavour && !m.fixture)
    .unwrap()
}
fn review_checked(src: &str, flavour: &str) -> selu_check::Checked {
    selu_check::check_program(
        &selu_syntax::parse("review.selu", src).unwrap(),
        review_manifest(flavour),
    )
}
#[test]
fn review_api_mismatches_are_warnings() {
    for flavour in ["retail", "forever"] {
        let c = review_checked(r#"C_Timer.After("soon", () => {});"#, flavour);
        assert_eq!(c.diagnostics.len(), 1);
        assert_eq!(c.diagnostics[0].code, "SELU0481");
        assert_eq!(
            c.diagnostics[0].severity,
            selu_diagnostics::Severity::Warning
        );
        assert!(!c.has_errors());
    }
}
#[test]
fn review_widgets_are_open_and_unknown_frames_fall_back() {
    let c = review_checked(
        r#"const f = CreateFrame("Frame", null, UIParent, "BackdropTemplate"); f.customFlag = true; f.SetBackdrop({}); f.SetSze(20, 30); const unknown = CreateFrame("UnrecordedFrame"); unknown.custom = 1;"#,
        "retail",
    );
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    assert_eq!(
        c.diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["SELU0482", "SELU0483"]
    );
}
#[test]
fn review_frame_types_and_factory_aliases() {
    for flavour in [
        "retail",
        "forever",
        "classic",
        "classic-era",
        "classic-anniversary",
        "classic-titan",
    ] {
        for name in [
            "CheckButton",
            "Cooldown",
            "MessageFrame",
            "ScrollingMessageFrame",
            "PlayerModel",
            "Model",
            "DressUpModel",
            "SimpleHTML",
            "ColorSelect",
            "EditBox",
            "Slider",
            "StatusBar",
            "ScrollFrame",
            "GameTooltip",
        ] {
            let p = selu_syntax::parse(
                "review.selu",
                &format!("export const f = CreateFrame(\"{name}\");"),
            )
            .unwrap();
            let (c, ex) =
                selu_check::check_module(&p, review_manifest(flavour), Default::default());
            assert!(
                c.diagnostics.is_empty(),
                "{flavour} {name}: {:?}",
                c.diagnostics
            );
            assert!(
                matches!(&ex["f"].ty,selu_check::Type::Widget(n,_) if n==name),
                "{:?}",
                ex["f"].ty
            );
        }
        let p = selu_syntax::parse(
            "review.selu",
            r#"const create = CreateFrame; export const button = create("Button");"#,
        )
        .unwrap();
        let (c, ex) = selu_check::check_module(&p, review_manifest(flavour), Default::default());
        assert!(c.diagnostics.is_empty());
        assert!(matches!(&ex["button"].ty,selu_check::Type::Widget(n,_) if n=="Button"));
    }
}
#[test]
fn review_region_tooltip_overloads_and_numeric_text() {
    for flavour in [
        "retail",
        "forever",
        "classic",
        "classic-era",
        "classic-anniversary",
        "classic-titan",
    ] {
        let c = review_checked(
            r#"
const f = CreateFrame("Frame");
f.SetParent(UIParent);
f.SetPoint("CENTER"); f.SetPoint("TOPLEFT", 10, -10); f.SetPoint("CENTER", UIParent);
f.SetPoint("CENTER", "UIParent", "CENTER", 0, 0); f.SetAllPoints();
f.CreateTexture().SetTexture(123); f.CreateTexture().SetColorTexture(1, 0, 0, 1);
f.CreateTexture().GetParent(); f.CreateAnimationGroup().Play();
const tip = CreateFrame("GameTooltip"); tip.SetOwner(UIParent, "ANCHOR_NONE"); tip.SetUnit("player"); tip.AddLine("hi"); tip.AddDoubleLine("left", "right"); tip.SetHyperlink("item:6948");
tip.SetText("hello"); tip.SetText("hello", 1); tip.SetText("hello", 1, 1); tip.SetText("hello", 1, 1, 1);
f.CreateFontString().SetText(42);
"#,
            flavour,
        );
        assert!(c.diagnostics.is_empty(), "{flavour}: {:?}", c.diagnostics);
    }
}
#[test]
fn review_callback_advice_and_excess_event_span() {
    let c = review_checked("C_Timer.After(1, GetServerTime);", "retail");
    assert_eq!(c.diagnostics.len(), 1);
    let d = &c.diagnostics[0];
    assert!(d.message.contains("callback") && d.message.contains("retail@"));
    assert!(d.fix.as_deref().unwrap_or("").contains("GetServerTime"));
    let src = "on PLAYER_LOGIN(extra) {}";
    let c = review_checked(src, "retail");
    assert_eq!(
        c.diagnostics[0].span.as_ref().unwrap().start,
        src.find("extra").unwrap()
    );
    assert!(c.diagnostics[0].message.contains("extra"));
}
#[test]
fn review_parent_and_parentheses() {
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("local p = CreateFrame('Frame'); local child = CreateFrame('Frame', nil, p); print(child:GetParent() == p)").unwrap();
    assert_eq!(sim.prints(), ["true"]);
}
#[test]
fn review_parentheses_only_in_expanding_positions() {
    let lua = compile(
        r#"print(GetBuildInfo(), "end"); const o = { name: GetBuildInfo() }; print(GetBuildInfo() + "!");"#,
        "retail",
    );
    assert!(lua.contains("print(GetBuildInfo(), \"end\")"), "{lua}");
    assert!(lua.contains("name = GetBuildInfo()"), "{lua}");
    assert!(lua.contains("GetBuildInfo() .. \"!\""), "{lua}");
}

#[test]
fn review_strict_api_option_and_selu_types() {
    let p = selu_syntax::parse("strict.selu", r#"C_Timer.After("soon", () => {});"#).unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        review_manifest("retail"),
        selu_check::CheckOptions { strict_api: true },
    );
    assert_eq!(c.diagnostics.len(), 1);
    assert_eq!(c.diagnostics[0].severity, selu_diagnostics::Severity::Error);
    assert_eq!(c.diagnostics[0].code, "SELU0481");
    let c = review_checked(
        r#"function text(x: string) {} text(42); class C { method(x: string) {} } const c = new C(); c.method(42);"#,
        "retail",
    );
    assert_eq!(
        c.diagnostics
            .iter()
            .map(|d| (&d.code, d.severity))
            .collect::<Vec<_>>(),
        [
            (&"SELU0335".to_string(), selu_diagnostics::Severity::Error),
            (&"SELU0335".to_string(), selu_diagnostics::Severity::Error)
        ]
    );
}
#[test]
fn review_strict_api_project_option() {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let root = std::env::temp_dir().join(format!(
        "selu-t1-api-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        r#"C_Timer.After("soon", () => {});"#,
    )
    .unwrap();
    for strict in [false, true] {
        std::fs::write(root.join("selu.toml"),format!("[addon]\nname = \"ApiPolicy\"\ntargets = [\"retail\"]\n[build]\nstrict_api = {strict}\n")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_selu"))
            .arg("build")
            .arg(&root)
            .arg("--manifest-dir")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        eprintln!("strict_api CLI PID {}", child.id());
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("CLI timeout");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert_eq!(status.success(), !strict, "{stderr}");
        assert!(
            stderr.contains(if strict {
                "error[SELU0481]"
            } else {
                "warning[SELU0481]"
            }),
            "{stderr}"
        );
    }
    let owned = root.canonicalize().unwrap();
    assert!(owned.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(owned).unwrap();
}

#[test]
fn review_numeric_text_is_converted_by_the_simulator() {
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("local f = CreateFrame('Frame'):CreateFontString(); f:SetText(42); print(type(f:GetText()), f:GetText()); f:SetText(nil); print(f:GetText())").unwrap();
    assert_eq!(sim.prints(), ["string\t42", "nil"]);
}

#[test]
fn review_region_factory_return_identities() {
    for flavour in [
        "retail",
        "forever",
        "classic",
        "classic-era",
        "classic-anniversary",
        "classic-titan",
    ] {
        let p=selu_syntax::parse("factories.selu",r#"const f = CreateFrame("Frame"); export const line = f.CreateLine(); export const mask = f.CreateMaskTexture(); export const group = f.CreateAnimationGroup();"#).unwrap();
        let (c, ex) = selu_check::check_module(&p, review_manifest(flavour), Default::default());
        assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
        for (binding, name) in [
            ("line", "Line"),
            ("mask", "MaskTexture"),
            ("group", "AnimationGroup"),
        ] {
            assert!(
                matches!(&ex[binding].ty,selu_check::Type::Widget(n,_) if n==name),
                "{flavour} {binding}: {:?}",
                ex[binding].ty
            );
        }
    }
}

#[test]
fn review_scalar_conditions_and_index_keys() {
    let lua = compile(
        r#"const names = new Map<string, string>(); const name = names.get(UnitName("player") ?? ""); if (UnitExists("player")) { print("exists"); } while (UnitExists("target")) { break; }"#,
        "retail",
    );
    use selu_lower::lua::LExpr;
    let key = LExpr::Paren(Box::new(LExpr::Call(
        Box::new(LExpr::Name("f".into())),
        vec![],
    )));
    let index = LExpr::Index(Box::new(LExpr::Name("names".into())), Box::new(key));
    assert_eq!(selu_lower::lua::print_expr(&index), "names[f()]");
    assert!(!lua.contains("if (UnitExists"), "{lua}");
    assert!(!lua.contains("while (UnitExists"), "{lua}");
}

#[test]
fn review_nonfinal_array_field_is_scalar() {
    let lua = compile(r#"const values = [GetBuildInfo(), "end"];"#, "retail");
    assert_eq!(lua, "local values = { GetBuildInfo(), \"end\" }\n");
}
