use selu_manifest::Manifest;
use selu_sim::Sim;
fn manifest() -> Manifest {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let mut m = selu_manifest::load_manifest(&root.join("retail/12.1.5.70391.json")).unwrap();
    for name in ["Free", "Restricted", "First", "Second", "Third"] {
        let mut f = m.function("GetTime").unwrap().clone();
        f.name = name.into();
        let d = f.signature.as_mut().unwrap();
        d.name = name.into();
        d.returns.clear();
        d.attributes
            .insert("IsProtectedFunction".into(), (name != "Free").into());
        m.functions.push(f);
    }
    m
}
fn checked(src: &str, m: &Manifest) -> (selu_syntax::Program, selu_check::Checked) {
    let p = selu_syntax::parse("review.selu", src).unwrap();
    let c = selu_check::check_program(&p, m);
    (p, c)
}
fn compile(src: &str, m: &Manifest) -> String {
    let (p, c) = checked(src, m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    selu_lower::lower(&p, m, &c.types).lua
}
#[test]
fn review_plain_frames_run_directly_in_combat() {
    let m = manifest();
    for src in [
        "const f=CreateFrame(\"Frame\"); f.Hide();",
        "const f=CreateFrame(\"Frame\",null,null,\"BackdropTemplate\"); const x=f.ClearAttribute(\"x\");",
    ] {
        let lua = compile(src, &m);
        assert!(!lua.contains("__selu_defer"), "{lua}");
    }
    let lua = compile(
        "const f=CreateFrame(\"Frame\"); f.Hide(); print(f.IsShown() ?? false);",
        &m,
    );
    let sim = Sim::new().unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["false"]);
}
#[test]
fn review_unknown_receivers_check_protection_once() {
    let m = manifest();
    let lua = compile("UIParent.Hide();", &m);
    for protected in [false, true] {
        let sim = Sim::new().unwrap();
        sim.eval(&format!("calls=0; UIParent.IsProtected=function() calls=calls+1; return {} end; UIParent.Hide=function() print('hidden') end",protected)).unwrap();
        sim.set_combat(true).unwrap();
        sim.load_files(&[("test.lua".into(), lua.clone())]).unwrap();
        assert_eq!(sim.prints().len(), usize::from(!protected));
        sim.eval("print(calls)").unwrap();
        assert_eq!(sim.prints().last().unwrap(), "1");
        sim.set_combat(false).unwrap();
        assert_eq!(sim.prints().iter().filter(|x| *x == "hidden").count(), 1);
    }
}
#[test]
fn review_secret_passthrough_and_explicit_reads() {
    let m = manifest();
    let lua = compile(
        r#"const s: Secret<string> = UnitName("player"); const label=CreateFrame("Frame").CreateFontString(); label.SetText(s); function keep(x: Secret<string>): Secret<string> { return x; } const xs: Secret<string>[] = [keep(s)]; const values=new Map<string, Secret<string>>(); values.set("x",s); print(s ?? "fallback"); const value=readable(s); print(value ?? "fallback");"#,
        &m,
    );
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("function UnitName() return hidden end; local base=CreateFrame; CreateFrame=function(...) local f=base(...); f.CreateFontString=function() return {SetText=function(self,v) assert(issecretvalue(v)); print('passed') end} end; return f end").unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["passed", "fallback", "fallback"]);
    for source in [
        "const n: number=UnitHealth(\"player\");",
        "print(UnitHealth(\"player\") + 1);",
        "if(UnitHealth(\"player\")) {}",
        "const n=UnitHealth(\"player\"); print(n == 0);",
        "function f(x:number) {} f(UnitHealth(\"player\"));",
    ] {
        let (_, c) = checked(source, &m);
        assert!(
            c.diagnostics.iter().any(|d| d.code.as_str() == "SELU0601"),
            "{source}: {:?}",
            c.diagnostics
        );
    }
}
#[test]
fn review_aspect_getters_are_secret() {
    let m = manifest();
    for source in [
        "const label=CreateFrame(\"Frame\").CreateFontString(); const s:string=label.GetText();",
        "const n:number=CreateFrame(\"Frame\").GetID();",
    ] {
        let (_, c) = checked(source, &m);
        assert!(
            c.diagnostics.iter().any(|d| d.code.as_str() == "SELU0601"),
            "{:?}",
            c.diagnostics
        );
    }
}
#[test]
fn review_mutable_function_policy_survives_storage() {
    let m = manifest();
    for source in [
        "let f=Free; f=Restricted; f();",
        "const o={f:Free}; o.f=Restricted; o.f();",
        "const xs=[Free]; xs[0]=Restricted; xs[0]();",
        "let f=Free; f=Restricted; const g=f; g();",
        "const o={f:Free}; o.f=Restricted; const g=o.f; g();",
        "const xs=[Free]; xs[0]=Restricted; const g=xs[0]; g();",
    ] {
        let lua = compile(source, &m);
        let sim = Sim::new().unwrap();
        sim.eval("function Free() end; function Restricted() print('ran') end")
            .unwrap();
        sim.set_combat(true).unwrap();
        sim.load_files(&[("test.lua".into(), lua)]).unwrap();
        assert!(sim.prints().is_empty(), "{source}");
        sim.set_combat(false).unwrap();
        assert_eq!(sim.prints(), ["ran"]);
    }
}
#[test]
fn review_deferred_error_does_not_strand_following_work() {
    let m = manifest();
    let lua = compile("First(); Second();", &m);
    let sim = Sim::new().unwrap();
    sim.eval("function First() error('bad') end; function Second() print('second') end; function geterrorhandler() return function(e) print('error') end end").unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["error", "second"]);
}
#[test]
fn review_modules_share_one_fifo_and_registration() {
    let m = manifest();
    let a = compile("First(); global later=()=> { Third(); };", &m);
    let b = compile("Second();", &m);
    let sim = Sim::new().unwrap();
    sim.eval("function First() print('first') end; function Second() print('second') end; function Third() print('third') end; registrations=0; local base=CreateFrame; CreateFrame=function(...) local f=base(...); local register=f.RegisterEvent; f.RegisterEvent=function(self,e) if e=='PLAYER_REGEN_ENABLED' then registrations=registrations+1 end; register(self,e) end; return f end").unwrap();
    sim.set_combat(true).unwrap();
    sim.load_addon("Queue", &[("a.lua".into(), a), ("b.lua".into(), b)])
        .unwrap();
    sim.eval("later()").unwrap();
    sim.set_combat(false).unwrap();
    sim.eval("print(registrations)").unwrap();
    assert_eq!(sim.prints(), ["first", "second", "third", "1"]);
}
#[test]
fn review_coverage_gap_is_explicit() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/lang/restrictions.md"),
    )
    .unwrap();
    for name in ["CastSpellByName", "TargetUnit", "UseAction"] {
        assert!(doc.contains(name));
    }
}

#[test]
fn review_simulated_text_keeps_secret_identity() {
    let m = manifest();
    let lua = compile(
        r#"global label=CreateFrame("Frame").CreateFontString(); const name=UnitName("player"); label.SetText(name); print(label.GetText() ?? "hidden");"#,
        &m,
    );
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("function UnitName() return hidden end").unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["hidden"]);
    sim.eval("assert(issecretvalue(label:GetText()))").unwrap();
}
#[test]
fn review_secret_capabilities_across_targets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let classic =
        selu_manifest::load_manifest(&root.join("classic-era/1.15.9.70576.json")).unwrap();
    let (p, c) = checked(
        "const s:Secret<string>=GetLocale(); const plain:string=s;",
        &classic,
    );
    let _ = p;
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let targets = [classic, manifest()];
    for primary in &targets {
        let p=selu_syntax::parse("review.selu",r#"const name: Secret<string> = UnitName("player"); const label=CreateFrame("Frame").CreateFontString(); label.SetText(name); const text: Secret<string> = label.GetText(); print(text ?? "hidden");"#).unwrap();
        let (c, _) = selu_check::check_module_targets(&p, primary, Default::default(), &targets);
        assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
        let p = selu_syntax::parse(
            "review.selu",
            "const n:number=CreateFrame(\"Frame\").GetID();",
        )
        .unwrap();
        let (c, _) = selu_check::check_module_targets(&p, primary, Default::default(), &targets);
        assert!(c.diagnostics.iter().any(|d| d.code == "SELU0601"));
    }
}

#[test]
fn review_plain_provenance_survives_only_proven_values() {
    let m = manifest();
    for source in [
        r#"const f=false ? CreateFrame("Frame") : UIParent; f.Hide();"#,
        r#"const o={f:CreateFrame("Frame")}; o.f=UIParent; const f=o.f; f.Hide();"#,
        r#"function plain(){return CreateFrame("Frame");} function unknown(){return UIParent;} let make=plain; make=unknown; const f=make(); f.Hide();"#,
    ] {
        let lua = compile(source, &m);
        let sim = Sim::new().unwrap();
        sim.eval("UIParent.__protected=true; UIParent.Hide=function() print('hidden') end")
            .unwrap();
        sim.set_combat(true).unwrap();
        sim.load_files(&[("test.lua".into(), lua)]).unwrap();
        assert!(sim.prints().is_empty(), "{source}");
        sim.set_combat(false).unwrap();
        assert_eq!(sim.prints(), ["hidden"]);
    }
}

#[test]
fn review_secret_policy_does_not_require_a_known_inner_type() {
    let mut m = manifest();
    let mut f = m.function("GetTime").unwrap().clone();
    f.name = "UnknownSecret".into();
    f.returns_secret = true;
    let d = f.signature.as_mut().unwrap();
    d.name = "UnknownSecret".into();
    d.returns[0].ty = Some("UndocumentedEngineValue".into());
    d.attributes.insert("SecretReturns".into(), true.into());
    m.functions.push(f);
    let (_, c) = checked("const value=UnknownSecret(); print(value);", &m);
    assert!(
        c.diagnostics.iter().any(|d| d.code == "SELU0601"),
        "{:?}",
        c.diagnostics
    );
    let lua = compile(
        "const value=UnknownSecret(); const label=CreateFrame(\"Frame\").CreateFontString(); label.SetText(value); print(value ?? \"read\");",
        &m,
    );
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("function UnknownSecret() return hidden end")
        .unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["read"]);
}

#[test]
fn review_unspecified_function_results_keep_unknown_policy() {
    let mut m = manifest();
    let mut f = m.function("GetTime").unwrap().clone();
    f.name = "GetCallback".into();
    let d = f.signature.as_mut().unwrap();
    d.name = "GetCallback".into();
    d.returns[0].ty = Some("LuaFunctionReference".into());
    m.functions.push(f);
    let lua = compile("const callback=GetCallback(); callback();", &m);
    let sim = Sim::new().unwrap();
    sim.eval(
        "function Restricted() print('ran') end; function GetCallback() return Restricted end",
    )
    .unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert!(sim.prints().is_empty());
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["ran"]);
}

#[test]
fn review_hooks_cannot_invalidate_function_protection() {
    let m = manifest();
    let lua = compile(
        "hook(\"Restricted\",()=>{}); let f=Free; f=Restricted; f();",
        &m,
    );
    let sim = Sim::new().unwrap();
    sim.eval("function Restricted() print('ran') end; function Free() end")
        .unwrap();
    sim.set_combat(true).unwrap();
    sim.load_addon("Hooked", &[("test.lua".into(), lua)])
        .unwrap();
    assert!(sim.prints().is_empty());
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["ran"]);
}
#[test]
fn review_unspecified_callbacks_can_refer_to_widget_operations_on_classic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let mut m = selu_manifest::load_manifest(&root.join("classic-era/1.15.9.70576.json")).unwrap();
    let mut f = manifest().function("GetTime").unwrap().clone();
    f.name = "GetCallback".into();
    let d = f.signature.as_mut().unwrap();
    d.name = "GetCallback".into();
    d.returns[0].ty = Some("LuaFunctionReference".into());
    m.functions.push(f);
    let lua = compile("const cb=GetCallback(); cb(UIParent);", &m);
    let sim = Sim::new().unwrap();
    sim.eval("UIParent.__protected=true; UIParent.Hide=function(self) print('hidden') end; function GetCallback() return UIParent.Hide end").unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert!(sim.prints().is_empty());
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["hidden"]);
}

#[test]
fn review_secret_category_survives_untyped_joins() {
    let mut m = manifest();
    let mut f = m.function("GetTime").unwrap().clone();
    f.name = "UnknownSecret".into();
    f.returns_secret = true;
    let d = f.signature.as_mut().unwrap();
    d.name = "UnknownSecret".into();
    d.returns[0].ty = Some("UndocumentedEngineValue".into());
    d.attributes.insert("SecretReturns".into(), true.into());
    m.functions.push(f);
    let sources = [
        r#"const value=UnitExists("player") ? UnitName("player") : GetUnitName("player"); print(value);"#,
        r#"const value=UnknownSecret() ?? UnitName("player"); print(value);"#,
    ];
    let rejected: Vec<_> = sources
        .iter()
        .map(|source| {
            checked(source, &m)
                .1
                .diagnostics
                .iter()
                .any(|d| d.code == "SELU0601")
        })
        .collect();
    assert_eq!(rejected, [true, true]);
    let lua = compile(
        r#"const name=UnitName("player"); const value=UnitExists("player") ? name : GetUnitName("player"); global label=CreateFrame("Frame").CreateFontString(); label.SetText(value); print(value ?? "read"); const alternative=UnknownSecret() ?? name; label.SetText(alternative); print(alternative ?? "read");"#,
        &m,
    );
    assert!(
        !lua.contains("and name or"),
        "opaque branches must not be tested for truthiness: {lua}"
    );
    for choose in [true, false] {
        let sim = Sim::new().unwrap();
        sim.set_secret_global("hidden").unwrap();
        sim.eval(&format!("function UnitName() return hidden end; function UnknownSecret() return hidden end; function UnitExists() return {choose} end; function GetUnitName() return 'public' end")).unwrap();
        sim.load_files(&[("test.lua".into(), lua.clone())]).unwrap();
        assert_eq!(
            sim.prints(),
            [if choose { "read" } else { "public" }, "read"]
        );
        sim.eval("assert(issecretvalue(label:GetText()))").unwrap();
    }
}

#[test]
fn review_receiver_security_state_is_read_before_branching() {
    let m = manifest();
    let mut no_detector = m.clone();
    no_detector.functions.retain(|f| f.name != "canaccessvalue");
    assert!(!compile("UIParent.Hide();", &no_detector).contains("canaccessvalue"));
    let lua = compile("UIParent.Hide();", &m);
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("reads=0; local access=canaccessvalue; canaccessvalue=function(v) reads=reads+1; return access(v) end; UIParent.IsProtected=function() return hidden end; UIParent.Hide=function() print('hidden') end").unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert!(sim.prints().is_empty());
    sim.eval("print(reads)").unwrap();
    assert_eq!(sim.prints(), ["1"]);
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["1", "hidden"]);
}
