use std::path::Path;
fn codes(source: &str) -> Vec<String> {
    codes_on(source, "retail", "wow@12.1.5.70391")
}
fn codes_on(source: &str, family: &str, target: &str) -> Vec<String> {
    let m = manifest_on(family, target);
    let p = selu_syntax::parse("sf.selu", source).unwrap();
    selu_check::check(&p, &m)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}
fn manifest_on(family: &str, target: &str) -> selu_manifest::Manifest {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut m = selu_manifest::store::load_build(&root.join("manifests/store"), target).unwrap();
    selu_manifest_gen::apply_overlay(
        &mut m,
        &std::fs::read_to_string(root.join(format!("manifests/{family}/overlay.toml"))).unwrap(),
    )
    .unwrap();
    selu_manifest_gen::native::extend(&mut m, &[("Interface/AddOns/Blizzard_SharedXML/UI.xsd".into(), r#"<schema><element name="Alpha" substitutionGroup="AnimationRef"/><element name="Translation" substitutionGroup="AnimationRef"/><element name="Scale" substitutionGroup="AnimationRef"/><element name="Rotation" substitutionGroup="AnimationRef"/></schema>"#.into())]);
    m
}
#[test]
fn fixed_standard_library_contracts() {
    assert!(codes("tostring(UnitHealth(\"player\"));").contains(&"SELU0481".into()));
    assert!(codes("tonumber(UnitHealth(\"player\"));").contains(&"SELU0481".into()));
    assert!(codes("math.random(null);").contains(&"SELU0481".into()));
    assert!(codes("const x: number = math.random();").is_empty());
    assert!(codes("const [first, second, third] = strsplit(\":\", \"a:b\"); const s: string? = third; const j: string = strjoin(\",\", \"x\", 2);").is_empty());
    assert!(
        !codes("const [first, second] = strsplit(\":\", \"a:b\"); const n: number? = second;")
            .is_empty()
    );
    assert!(codes("const n: number? = tonumber(\"bad\"); const s: string = string.format(\"%s %d\", \"x\", 3); const m: number = math.min(1,2,3);").is_empty());
    assert!(codes("const n: number = tonumber(\"bad\");").contains(&"SELU0336".into()));
    assert!(codes("string.format(true);").contains(&"SELU0481".into()));
    assert!(codes("math.sqrt(false);").contains(&"SELU0481".into()));
}
#[test]
fn sourced_item_positions() {
    assert!(codes("const [name, link, quality, level, minLevel, itemType, subtype, count, equip, icon, price] = C_Item.GetItemInfo(6948); const n: string? = name; const p: number? = price;").is_empty());
    assert!(codes("C_Item.GetItemInfo(true);").contains(&"SELU0481".into()));
    assert!(codes("const [id, ty, sub, loc, icon, classId, subclassId] = C_Item.GetItemInfoInstant(\"item:6948\"); const n: number? = id; const s: string? = ty;").is_empty());
}
#[test]
fn date_variants_and_popup_signature() {
    assert!(codes("const now: string = date();").is_empty());
    assert!(codes("const s: string = date(\"%H:%M\"); const d = date(\"*t\"); const y: number = d.year; const t: number = time();").is_empty());
    assert!(codes("date(true);").contains(&"SELU0481".into()));
    assert!(codes("StaticPopup_Show(false);").contains(&"SELU0481".into()));
    assert!(codes("const dialog = StaticPopup_Show(\"TEST\");").is_empty());
}
#[test]
fn popup_entry_shape() {
    assert!(codes("StaticPopupDialogs.set(\"TEST\", { text: \"Test\", OnAccept: () => {}, OnCancel: (self, data, reason) => {} });").is_empty());
    assert!(
        !codes("StaticPopupDialogs.set(\"TEST\", { OnAccept: (self: number) => {} });").is_empty()
    );
    assert!(
        !codes("const bad = { invented: true }; StaticPopupDialogs.set(\"TEST\", bad);").is_empty()
    );
    assert!(!codes("StaticPopupDialogs.set(\"TEST\", { invented: true });").is_empty());
    assert!(codes("StaticPopupDialogs.set(\"TEST\", { text: \"Test\", button1: \"Yes\", timeout: 0, whileDead: true, hideOnEscape: true });").is_empty());
    assert!(!codes("StaticPopupDialogs.set(\"TEST\", { timeout: \"bad\" });").is_empty());
}

#[test]
fn sourced_animation_factories_and_parent() {
    assert!(codes("const group = CreateFrame(\"Frame\").CreateTexture().CreateAnimationGroup(); group.CreateAnimation(\"Alpha\").SetFromAlpha(0.15); group.CreateAnimation(\"Translation\").SetOffset(1, 2); group.CreateAnimation(\"Scale\").SetScale(1, 1); group.CreateAnimation(\"Rotation\").SetDegrees(30);").is_empty());
    assert!(codes("CreateFrame(\"Frame\").CreateAnimationGroup().CreateAnimation(\"Alpha\").SetToAlpha(true);").contains(&"SELU0481".into()));
    assert!(!codes("const p = CreateFrame(\"Frame\").GetParent(); p.GetName();").is_empty());
    assert!(codes("const p = readable(CreateFrame(\"Frame\").GetParent()); if (p != null) { p.GetDebugName(); }").is_empty());
}

#[test]
fn sourced_script_parameters() {
    assert!(codes_on("CreateFrame(\"Frame\").HookScript(\"OnUpdate\", (self, elapsed) => { const n: number = elapsed; });", "forever", "wowf@1.60.1.70790").is_empty());
    assert!(codes("CreateFrame(\"Frame\").HookScript(\"OnEnter\", (self, motion) => { const m: boolean = motion; });").is_empty());
    assert!(codes("CreateFrame(\"Button\").SetScript(\"OnClick\", (self, button, down) => { const b: string = button; const d: boolean = down; });").is_empty());
    assert!(
        codes("CreateFrame(\"Frame\").HookScript(\"OnUpdate\", (self, elapsed: string) => {});")
            .contains(&"SELU0481".into())
    );
}

#[test]
fn contracts_at_retained_and_toolkit_pins() {
    for (family, target) in [
        ("retail", "wow@12.1.5.70391"),
        ("forever", "wowf@1.60.1.70532"),
        ("forever", "wowf@1.60.1.70790"),
        ("classic", "wow_classic@5.5.4.70631"),
        ("classic-era", "wow_classic_era@1.15.9.70576"),
        ("classic-anniversary", "wow_anniversary@2.5.6.70577"),
        ("classic-titan", "wow_classic_titan@3.80.2.70522"),
    ] {
        assert!(codes_on("const s: string = date(\"%H\"); const t: number = time(); const n: number? = tonumber(\"bad\"); StaticPopup_Show(\"TEST\"); C_Item.GetItemInfo(6948); CreateFrame(\"Frame\").HookScript(\"OnMouseWheel\", (self, delta) => { const n: number = delta; });",family,target).is_empty(), "{target}");
        assert!(
            codes_on("C_Item.GetItemInfo(true);", family, target).contains(&"SELU0481".into()),
            "{target}"
        );
        assert!(
            codes_on("strtrim(false);", family, target).contains(&"SELU0481".into()),
            "{target}"
        );
    }
}

#[test]
fn gmatch_native_closure_preserves_multiple_returns() {
    let m = manifest_on("retail", "wow@12.1.5.70391");
    let p = selu_syntax::parse(
        "gmatch.selu",
        r#"const iter = string.gmatch("a:b", "(%a):(%a)"); const [a,b] = iter();"#,
    )
    .unwrap();
    let checked = selu_check::check_program(&p, &m);
    assert!(
        !checked.has_errors(),
        "{:?}\n{:?}",
        checked.diagnostics,
        checked.types
    );
    let generated = selu_lower::lower(&p, &m, &checked.types).lua;
    let lua = mlua::Lua::new();
    lua.set_memory_limit(32 * 1024 * 1024).unwrap();
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(100_000),
        |_, _| Err(mlua::Error::RuntimeError("instruction limit".into())),
    );
    let values = lua
        .load(format!("{generated}\nreturn a,b"))
        .eval::<(String, String)>()
        .unwrap();
    assert_eq!(values, ("a".into(), "b".into()));
    let mut broken = m;
    broken
        .functions
        .iter_mut()
        .find(|f| f.name == "string.gmatch")
        .unwrap()
        .signature
        .as_mut()
        .unwrap()
        .returns[0]
        .ty = Some("LuaFunctionReference".into());
    let checked = selu_check::check_program(&p, &broken);
    assert!(
        checked.has_errors(),
        "an unspecified callback must not claim the checked iterator contract"
    );
}

#[test]
fn factory_specialization_preserves_source_secrecy() {
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    let method = m
        .widgets
        .iter_mut()
        .find(|w| w.name == "SimpleAnimGroupAPI")
        .unwrap()
        .methods
        .iter_mut()
        .find(|d| d.name == "CreateAnimation")
        .unwrap();
    method
        .attributes
        .insert("SecretReturns".into(), true.into());
    let p = selu_syntax::parse("secret-animation.selu", "const group = CreateFrame(\"Frame\").CreateAnimationGroup(); const alpha = group.CreateAnimation(\"Alpha\"); alpha.SetFromAlpha(0.2);").unwrap();
    assert!(
        selu_check::check(&p, &m)
            .iter()
            .any(|d| d.code == "SELU0601")
    );
}
#[test]
fn standard_table_element_contracts() {
    assert!(
        codes(r#"const xs = [UnitHealth("player")]; table.insert(xs, UnitHealth("player"));"#)
            .contains(&"SELU0481".into())
    );
    assert!(
        codes(r#"const xs = [UnitHealth("player")]; const n: number? = table.remove(xs);"#)
            .contains(&"SELU0601".into())
    );
    assert!(codes(r#"const xs = ["a", "b"]; table.insert(xs, "c"); const s: string? = table.remove(xs); const [a,b] = unpack(xs); const x: string? = b; table.sort(xs, (a,b) => a < b); const joined: string = table.concat(xs, ",");"#).is_empty());
    assert!(!codes(r#"const xs = ["a"]; table.insert(xs, 3);"#).is_empty());
    assert!(
        !codes(r#"const xs = ["a"]; table.sort(xs, (a: number,b: number) => a < b);"#).is_empty()
    );
    assert!(!codes("const xs = [true]; table.sort(xs);").is_empty());
    assert!(!codes("const xs = [true]; table.sort(xs, null);").is_empty());
    let m = manifest_on("retail", "wow@12.1.5.70391");
    let p = selu_syntax::parse("tables.selu", r#"const xs = ["a", "b"]; table.insert(xs, "c"); const value = table.remove(xs); const [a,b] = unpack(xs);"#).unwrap();
    let checked = selu_check::check_program(&p, &m);
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let generated = selu_lower::lower(&p, &m, &checked.types).lua;
    let lua = mlua::Lua::new();
    lua.set_memory_limit(32 * 1024 * 1024).unwrap();
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(100_000),
        |_, _| Err(mlua::Error::RuntimeError("instruction limit".into())),
    );
    let values = lua
        .load(format!("{generated}\nreturn value,a,b"))
        .eval::<(String, String, String)>()
        .unwrap();
    assert_eq!(values, ("c".into(), "a".into(), "b".into()));
}
#[test]
fn table_results_do_not_claim_stored_frames_are_unprotected() {
    let m = manifest_on("retail", "wow@12.1.5.70391");
    let p = selu_syntax::parse("stored-frame.selu", r#"const frames = [CreateFrame("Frame")]; const frame = table.remove(frames) ?? CreateFrame("Frame"); frame.SetSize(10,20);"#).unwrap();
    let checked = selu_check::check_program(&p, &m);
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let selu_syntax::StmtKind::Expr(expr) = &p.stmts.last().unwrap().kind else {
        panic!("call statement")
    };
    let selu_syntax::ExprKind::Call { callee, .. } = &expr.kind else {
        panic!("call")
    };
    assert!(
        matches!(checked.types.get(callee.id), Some(selu_check::Type::Function { api_semantics, .. }) if api_semantics.protected)
    );
}
