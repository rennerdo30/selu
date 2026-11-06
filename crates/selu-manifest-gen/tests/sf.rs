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
    assert!(codes("const angle: number = math.atan2(1, 2); const pi: number = math.pi; const huge: number = math.huge;").is_empty());
    assert!(codes("math.atan2(true, 2);").contains(&"SELU0481".into()));
    assert!(!codes("const wrong: boolean = math.pi;").is_empty());
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
    assert!(codes("StaticPopupDialogs.set(\"TEST\", { text: \"Test\", button1: \"Yes\", timeout: 0, whileDead: 1, hideOnEscape: true });").is_empty());
    assert!(!codes("StaticPopupDialogs.set(\"TEST\", { timeout: \"bad\" });").is_empty());
}

#[test]
fn sourced_animation_factories_and_parent() {
    assert!(codes("const group = CreateFrame(\"Frame\").CreateTexture().CreateAnimationGroup(); group.CreateAnimation(\"Alpha\").SetFromAlpha(0.15); group.CreateAnimation(\"Translation\").SetOffset(1, 2); group.CreateAnimation(\"Scale\").SetScale(1, 1); group.CreateAnimation(\"Rotation\").SetDegrees(30);").is_empty());
    assert!(codes("CreateFrame(\"Frame\").CreateAnimationGroup().CreateAnimation(\"Alpha\").SetToAlpha(true);").contains(&"SELU0481".into()));
    assert!(!codes("const p = CreateFrame(\"Frame\").GetParent(); p.GetName();").is_empty());
    assert!(!codes("const p = CreateFrame(\"Frame\").GetParent(); p.GetDebugName();").is_empty());
    assert!(!codes("const p = readable(CreateFrame(\"Frame\").GetParent()); if (p != null) { p.GetName(); }").is_empty());
    assert!(codes("const p = readable(CreateFrame(\"Frame\").GetParent()); if (p != null) { p.GetDebugName(); }").is_empty());
}

#[test]
fn sourced_script_parameters() {
    assert!(codes_on("CreateFrame(\"Frame\").HookScript(\"OnUpdate\", (self, elapsed) => { const n: number = elapsed; });", "forever", "wowf@1.60.1.70790").is_empty());
    assert!(codes("CreateFrame(\"Frame\").HookScript(\"OnEnter\", (self, motion) => { if (typeof motion == \"boolean\") { const m: boolean = motion; } });").is_empty());
    assert!(codes("CreateFrame(\"Button\").SetScript(\"OnClick\", (self, button, down) => { const b: string = button; const d: boolean? = down; });").is_empty());
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

#[test]
fn qualified_scalar_support_does_not_invent_xml_child_fields() {
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    m.globals.push(selu_manifest::GlobalEntry {
        name: "UIParent.UnsourcedChild".into(),
        ty: Some("Frame".into()),
        evidence: selu_manifest::Evidence::Source,
        templates: vec![],
        value: None,
    });
    let p = selu_syntax::parse(
        "literal-global.selu",
        "const child = UIParent.UnsourcedChild;",
    )
    .unwrap();
    assert!(
        selu_check::check(&p, &m)
            .iter()
            .any(|d| d.code == "SELU0582")
    );
}
#[test]
fn documented_base_object_interface_accepts_frame_receivers() {
    let result = codes(
        r#"CreateFrame("Frame").HookScript("OnEnter", (self: CScriptObject, motion) => { self.GetDebugName(); });"#,
    );
    assert!(result.is_empty(), "{result:?}");
}

#[test]
fn script_receiver_keeps_the_runtime_protection_check() {
    use selu_syntax::{ExprKind, FnBody, StmtKind};
    let m = manifest_on("retail", "wow@12.1.5.70391");
    let p = selu_syntax::parse("script-frame.selu", r#"CreateFrame("Frame").HookScript("OnUpdate", (self, elapsed) => { self.SetSize(10,20); });"#).unwrap();
    let checked = selu_check::check_program(&p, &m);
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let StmtKind::Expr(expr) = &p.stmts[0].kind else {
        panic!("call")
    };
    let ExprKind::Call { args, .. } = &expr.kind else {
        panic!("registration")
    };
    let ExprKind::Function {
        body: FnBody::Block(body),
        ..
    } = &args[1].kind
    else {
        panic!("handler")
    };
    let StmtKind::Expr(body) = &body[0].kind else {
        panic!("statement")
    };
    let ExprKind::Call { callee, .. } = &body.kind else {
        panic!("method")
    };
    assert!(
        matches!(checked.types.get(callee.id), Some(selu_check::Type::Function { api_semantics, .. }) if api_semantics.protected)
    );
}

#[test]
fn review_opaque_payload_cannot_acquire_a_concrete_type() {
    for source in [
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { const p:{itemGUID:string}=value; const s:string=p.itemGUID; });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value:number) => {});"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { const s = value.itemGUID; });"#,
        r#"StaticPopupDialogs.set("TEST", { OnShow:(self,data) => { const n:number=data; } });"#,
        r#"StaticPopupDialogs.set("TEST", { OnAccept:(self,data) => { const n:number=data; } });"#,
        r#"StaticPopupDialogs.set("TEST", { OnHide:(self,data) => { const n:number=data; } });"#,
        r#"StaticPopupDialogs.set("TEST", { OnCancel:(self,data) => { const n:number=data; } });"#,
    ] {
        assert!(
            !codes(source).is_empty(),
            "unsourced callback accepted: {source}"
        );
    }
}

#[test]
fn review_popup_numeric_flags_follow_native_storage() {
    for (family, target) in [
        ("retail", "wow@12.1.5.70391"),
        ("forever", "wowf@1.60.1.70790"),
        ("classic", "wow_classic@5.5.4.70631"),
        ("classic-era", "wow_classic_era@1.15.9.70576"),
        ("classic-anniversary", "wow_anniversary@2.5.6.70577"),
        ("classic-titan", "wow_classic_titan@3.80.2.70522"),
    ] {
        let result = codes_on(
            r#"StaticPopupDialogs.set("COPY",{ hideOnEscape:1, multiple:1, whileDead:1 });"#,
            family,
            target,
        );
        assert!(result.is_empty(), "{target}: {result:?}");
    }
}

#[test]
fn review_popup_read_flags_cannot_claim_boolean_storage() {
    assert!(!codes(r#"const popup = StaticPopupDialogs.get("GENERIC_CONFIRMATION"); if(popup != null) { const b:boolean?=popup.hideOnEscape; }"#).is_empty());
}

#[test]
fn review_click_down_is_optional_and_scalar_claims_are_withheld() {
    for source in [
        r#"CreateFrame("Button").SetScript("OnClick", (self,button,down:boolean) => {});"#,
        r#"CreateFrame("Frame").HookScript("OnEnter", (self,motion:boolean) => {});"#,
        r#"CreateFrame("Frame").HookScript("OnLeave", (self,motion:boolean) => {});"#,
        r#"CreateFrame("Frame").HookScript("OnKeyDown", (self,key:string) => {});"#,
        r#"CreateFrame("Frame").HookScript("OnKeyUp", (self,key:string) => {});"#,
    ] {
        assert!(
            !codes(source).is_empty(),
            "unsupported scalar contract: {source}"
        );
    }
}

#[test]
fn review_opaque_payloads_allow_ignoring_narrowing_and_matching_round_trips() {
    for source in [
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", () => {});"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { self.SetAttribute(name,value); });"#,
        r#"StaticPopupDialogs.set("TEST", { OnAccept:() => {}, OnCancel:() => {}, OnShow:() => {}, OnHide:() => {} });"#,
        r#"StaticPopupDialogs.set("TEST", { OnShow:(self,data) => { StaticPopup_Show("OTHER",null,null,data); } });"#,
        r#"StaticPopup_Show("TEST",null,null,{itemGUID:"item:1"});"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { if(typeof value == "number") { const n:number=value; math.abs(value); } if(value != null) { self.SetAttribute(name,value); } });"#,
        r#"StaticPopupDialogs.set("TEST", { OnShow:(self,data) => { if(typeof data == "string") { const s:string=data; } if(typeof data == "boolean") { const b:boolean=data; } if(data != null && typeof data == "number") { const n:number=data; } } });"#,
        r#"CreateFrame("Button").SetScript("OnClick", (self,button,down) => { const b:string=button; const d:boolean?=down; if(down != null) { const checked:boolean=down; } });"#,
        r#"CreateFrame("Frame").HookScript("OnEnter", (self,motion) => { if(typeof motion == "boolean") { const b:boolean=motion; } });"#,
        r#"CreateFrame("Frame").HookScript("OnKeyDown", (self,key) => { if(typeof key == "string") { const s:string=key; } });"#,
    ] {
        let result = codes(source);
        assert!(result.is_empty(), "{source}: {result:?}");
    }
}

#[test]
fn review_opaque_payloads_cannot_escape_through_aliases_or_other_native_slots() {
    for source in [
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { const alias=value; const n:number=alias; });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { if(value != null) { const n:number=value; } });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { const n:number=value ?? 0; });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { const n:unknown=value; });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { math.abs(value); });"#,
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged", (self,name,value) => { StaticPopup_Show("TEST",null,null,value); });"#,
        r#"StaticPopupDialogs.set("TEST", { OnShow:(self,data) => { const x = true ? data : tonumber("1"); const n:number?=x; } });"#,
        r#"function typed(self:Frame,name:string,value:number):void {} CreateFrame("Frame").HookScript("OnAttributeChanged",typed);"#,
    ] {
        let m = manifest_on("retail", "wow@12.1.5.70391");
        let p = selu_syntax::parse("opaque.selu", source).unwrap();
        let checked = selu_check::check_program(&p, &m);
        assert!(
            checked.has_errors(),
            "opaque boundary was advisory or erased: {source}: {:?}",
            checked.diagnostics
        );
    }
}

#[test]
fn review_button_group_invocation_can_omit_down_and_preserves_false() {
    let m = manifest_on("retail", "wow@12.1.5.70391");
    let p=selu_syntax::parse("button-group.selu",r#"let missing = false; let observed:boolean?=null; const button=CreateFrame("Button"); button.SetScript("OnClick",(self,mouseButton,down)=>{ missing=down==null; observed=down; });"#).unwrap();
    let checked = selu_check::check_program(&p, &m);
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let generated = selu_lower::lower(&p, &m, &checked.types).lua;
    let lua = mlua::Lua::new();
    lua.set_memory_limit(32 * 1024 * 1024).unwrap();
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(100_000),
        |_, _| Err(mlua::Error::RuntimeError("instruction limit".into())),
    );
    let prefix = r#"local nativeButton={}; function nativeButton:SetScript(name,script) self[name]=script end; function nativeButton:GetScript(name) return self[name] end; function CreateFrame() return nativeButton end; "#;
    let suffix = r#"local click=nativeButton:GetScript("OnClick"); click(nativeButton,"LeftButton"); local absent=missing and observed==nil; click(nativeButton,"LeftButton",false); return absent,missing,observed"#;
    let result = lua
        .load(format!("{prefix}\n{generated}\n{suffix}"))
        .eval::<(bool, bool, bool)>()
        .unwrap();
    assert_eq!(result, (true, false, false));
}

#[test]
fn review_opaque_round_trip_capabilities_preserve_ordinary_api_constraints() {
    assert!(codes(r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data)=>{const send=StaticPopup_Show;send("NEXT",null,null,data);}});"#).is_empty());
    assert!(!codes(r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data)=>{let send=StaticPopup_Show;send("NEXT",null,null,data);}});"#).is_empty());
    assert!(!codes(r#"CreateFrame("Frame").SetAttribute("x",{invented:true});"#).is_empty());
    assert!(codes(r#"CreateFrame("Frame").SetAttribute("x","text");"#).is_empty());
    assert!(!codes(r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data)=>{const box={value:data};const erased:unknown=box;}});"#).is_empty());
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    m.functions.push(
        serde_json::from_value(
            serde_json::json!({"name":"ExistingUntypedResult","evidence":"source"}),
        )
        .unwrap(),
    );
    let p = selu_syntax::parse(
        "legacy-untyped.selu",
        "const retained: number = ExistingUntypedResult();",
    )
    .unwrap();
    assert!(
        selu_check::check(&p, &m).is_empty(),
        "unrelated legacy API result policy changed"
    );
}

#[test]
fn review_opaque_payloads_cannot_be_laundered_through_untyped_calls() {
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    m.functions.push(
        serde_json::from_value(
            serde_json::json!({"name":"ExistingUntypedResult","evidence":"source"}),
        )
        .unwrap(),
    );
    for source in [
        r#"CreateFrame("Frame").HookScript("OnAttributeChanged",(self,name,value)=>{const n:number=ExistingUntypedResult(value);});"#,
        r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data)=>{const dynamic=ExistingUntypedResult();const n:number=dynamic(data);}});"#,
    ] {
        let p = selu_syntax::parse("untyped-call.selu", source).unwrap();
        let checked = selu_check::check_program(&p, &m);
        assert!(
            checked.has_errors(),
            "opaque laundered through unchecked call: {source}"
        );
    }
}

#[test]
fn review_finite_callback_arity_does_not_create_untyped_extra_values() {
    for source in [
        r#"CreateFrame("Frame").SetScript("OnUpdate",(self,elapsed,extra)=>{const n:number=extra;});"#,
        r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data,extra)=>{const n:number=extra;}});"#,
    ] {
        assert!(
            !codes(source).is_empty(),
            "absent extra parameter was permissive: {source}"
        );
    }
    assert!(codes(r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data,extra)=>{const absent:null=extra;}});"#).is_empty());
    assert!(codes(r#"StaticPopupDialogs.set("TEST",{OnShow:(self,data,extra=1)=>{const fallback:number=extra;}});"#).is_empty());
}

#[test]
fn review_registration_aliases_cannot_erase_opaque_callback_boundaries() {
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    m.functions.push(
        serde_json::from_value(
            serde_json::json!({"name":"ExistingUntypedResult","evidence":"source"}),
        )
        .unwrap(),
    );
    for source in [
        r#"const frame=CreateFrame("Frame");let register=frame.HookScript;register("OnAttributeChanged",(self,name,value)=>{const n:number=value;});"#,
        r#"const frame=CreateFrame("Frame");const button=CreateFrame("Button");const register=true?frame.HookScript:button.HookScript;register("OnAttributeChanged",(self,name,value)=>{const n:number=value;});"#,
        r#"const frame=CreateFrame("Frame");const erased:unknown=frame.HookScript;if(typeof erased=="function"){erased("OnAttributeChanged",(self,name,value)=>{const n:number=value;});}"#,
        r#"const frame=CreateFrame("Frame");const register=true?frame.HookScript:(name,callback)=>{};register("OnAttributeChanged",(self,name,value)=>{const n:number=value;});"#,
        r#"const frame=CreateFrame("Frame");const register=true?frame.HookScript:ExistingUntypedResult();register("OnAttributeChanged",(self,name,value)=>{const n:number=value;});"#,
    ] {
        let p = selu_syntax::parse("registration-alias.selu", source).unwrap();
        let checked = selu_check::check_program(&p, &m);
        assert!(
            checked.has_errors(),
            "registration alias erased the boundary: {source}: {:?}",
            checked.diagnostics
        );
    }
    assert!(codes(r#"const frame=CreateFrame("Frame");const register=frame.HookScript;register("OnAttributeChanged",(self,name,value)=>{if(typeof value=="number"){const n:number=value;}});"#).is_empty());
    assert!(codes(r#"const frame=CreateFrame("Frame");let register=frame.HookScript;register("OnAttributeChanged",()=>{});"#).is_empty());
    assert!(codes(r#"const frame=CreateFrame("Frame");let register=frame.HookScript;register("UnreviewedScript",(self,value)=>{const n:number=value;});"#).is_empty());
}

#[test]
fn review_callback_slots_without_overlay_markers_are_opaque_too() {
    let mut m = manifest_on("retail", "wow@12.1.5.70391");
    m.tables.push(serde_json::from_value(serde_json::json!({"name":"SourcedCallbackFixture","type":"CallbackType","arguments":[{"name":"value","type":"MissingCallbackType","attributes":{"Nilable":true}}]})).unwrap());
    m.functions.push(serde_json::from_value(serde_json::json!({"name":"RegisterCallbackFixture","evidence":"documented","signature":{"name":"RegisterCallbackFixture","type":"Function","arguments":[{"name":"callback","type":"SourcedCallbackFixture"}]}})).unwrap());
    let p = selu_syntax::parse(
        "callback-slot.selu",
        "RegisterCallbackFixture((value)=>{ const n:number=value; });",
    )
    .unwrap();
    assert!(
        selu_check::check_program(&p, &m).has_errors(),
        "undefined callback type manufactured a checked number"
    );
    let p=selu_syntax::parse("callback-slot.selu","RegisterCallbackFixture(()=>{}); RegisterCallbackFixture((value)=>{if(typeof value==\"number\"){const n:number=value;}});").unwrap();
    assert!(selu_check::check(&p, &m).is_empty());
}
