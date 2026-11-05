use selu_manifest::{Manifest, store::load_build};
fn retail() -> Manifest {
    load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap()
}
fn check(m: &Manifest, src: &str) -> selu_check::Checked {
    let p = selu_syntax::parse("factory.selu", src).unwrap();
    selu_check::check_program_with_options(&p, m, selu_check::CheckOptions { strict_api: true })
}
fn iterator_fixture() -> Manifest {
    let mut m = retail();
    let src = r#"
local Pool={};
function Pool:Init() self.active={ [self:CallCreate()]=true }; end
function Pool:CallCreate() return CreateFrame("Frame"); end
function Pool:EnumerateActive() return pairs(self.active); end
function CreateProvenFramePool(frameType) local p=CreateFromMixins(Pool); p:Init(); return p; end
local View={};
function View:EnumerateFrames() return ipairs({}); end
function CreateProvenScrollBoxListLinearView() return CreateFromMixins(View); end
"#;
    let mut source = selu_manifest_gen::source::Sources::default();
    source.parse("iterator-fixture.lua", src).unwrap();
    source.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &[("iterator-fixture.lua".into(), src.into())]);
    let ty = m
        .function("CreateProvenFramePool")
        .unwrap()
        .signature
        .as_ref()
        .unwrap()
        .returns[0]
        .ty
        .as_ref()
        .unwrap();
    let pool = m.widgets.iter().find(|w| &w.name == ty).unwrap();
    assert_eq!(
        pool.methods
            .iter()
            .find(|d| d.name == "EnumerateActive")
            .unwrap()
            .returns[0]
            .attributes
            .get("SeluIterator"),
        Some(&serde_json::json!(["Frame", "bool"]))
    );
    m
}
#[test]
fn toolkit_pool_and_scrollbox_shapes() {
    let m = retail();
    for factory in ["CreateDataProvider", "CreateScrollBoxListLinearView"] {
        assert!(
            m.function(factory).unwrap().signature.is_some(),
            "source-proven {factory} must stay checked"
        );
    }
    let src = r#"
const view=CreateScrollBoxListLinearView();
for (const [index,frame] of view.EnumerateFrames()) { const i:number=index; }
const box=CreateFrame("Frame",null,UIParent,"WowScrollBoxList");
for (const [index,frame] of box.EnumerateFrames()) {}
const provider=CreateDataProvider();provider.Insert(42);provider.GetSize();
GameTooltip.ClearLines();const count:number=GameTooltip.NumLines();
"#;
    let p = selu_syntax::parse("toolkit.selu", src).unwrap();
    let c = check(&m, src);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    assert!(lua.contains("in view:EnumerateFrames() do"));
    assert!(lua.contains("in box:EnumerateFrames() do"));
    assert!(!lua.contains("pairs(view:EnumerateFrames()"));
    assert!(check(&m, "GameTooltip.NumLines(42);").has_errors());
    assert!(check(&m, "GameTooltip.ClearLines(42);").has_errors());
    assert!(check(&m, "const p=CreateFramePool(\"Frame\"); p.Undeclared();").has_errors());
}
#[test]
fn sourced_toolkit_pool_contracts() {
    let m = retail();
    // DialogKeys:1066, Skin/Framework:718 and actionbar_skin_runtime:279.
    let src = r#"
const dressPool=CreateFramePool("DressUpModel");for(const [dress] of dressPool.EnumerateActive()){const typed:DressUpModel=dress;}
const pool=CreateFramePool("Button",UIParent);
for (const [button,active] of pool.EnumerateActive()) { button.SetText("hello"); const a:boolean=active; }
const [button,isNew]=pool.Acquire();
if (button != null) { button.SetText("hello"); pool.Release(button); }
pool.ReleaseAll(); const count:number=pool.GetNumActive();
const collection=CreateFramePoolCollection();
const child=collection.CreatePool("Button",UIParent,"ButtonTemplate");
for (const [button,active] of child.EnumerateActive()) { button.SetText("hello"); }
for (const frame of collection.EnumerateActive()) { frame.Hide(); }
const fonts=CreateFontStringPool(UIParent); for(const [font] of fonts.EnumerateActive()){font.SetText("hello");}
const textures=CreateTexturePool(UIParent);for(const [texture] of textures.EnumerateActive()){texture.SetTexture(123);}
const masks=CreateMaskTexturePool(UIParent);for(const [mask] of masks.EnumerateActive()){mask.SetTexture(123);}
const actors=CreateActorPool(CreateFrame("ModelScene"));for(const [actor] of actors.EnumerateActive()){actor.ClearModel();}
const unsecured=CreateUnsecuredFramePoolCollection();
const [created,isNewPool]=unsecured.GetOrCreatePool("Button",UIParent,"ButtonTemplate");const fresh:boolean=isNewPool;
for(const [button] of created.EnumerateActive()){button.SetText("hello");}
const found=collection.GetPool("ButtonTemplate");if(found!=null){found.ReleaseAll();}
"#;
    let c = check(&m, src);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    for bad in [
        r#"const p=CreateUnsecuredObjectPool(()=>UIParent);"#,
        r#"const c=CreateFramePoolCollection();c.CreatePool("Button",UIParent);"#,
        r#"const p=CreateActorPool(UIParent);"#,
        r#"const p=CreateActorPool(CreateFrame("ModelScene"));for(const [actor] of p.EnumerateActive()){actor.NotDeclared();}"#,
        r#"const p=CreateFramePool("Button");p.Release(42);"#,
        r#"const p=CreateFramePool("Button");p.Acquire(42);"#,
        r#"const p=CreateFramePool("Button");p.GetNumActive(42);"#,
        r#"const p=CreateTexturePool(UIParent);p.Release(UIParent);"#,
        r#"const p=CreateFramePool("Button");p.Release(UIParent);"#,
    ] {
        assert!(check(&m, bad).has_errors(), "{bad}");
    }
}
#[test]
fn scoped_aliases_proxy_surface_and_dynamic_results() {
    let src = r#"
local Mix = CreateFromMixins;
local Private = {};
function Private:Acquire() end
function Private:Hidden() end
local Names = {"Acquire"};
local Proxy = ProxyUtil.CreateProxyMixin(nil, Private, Names);
local function Build() local p = Mix(Private); local proxy = ProxyUtil.CreateProxy(p, Proxy); return p; end
function CreateScoped() return Build():ToProxy(); end
-- A leading comment must not become part of the alias token.
CreateAlias = CreateScoped;
do local CreateAlias = function() return {} end end
function CreateCycle() return CreateCycle(); end
function CreateDynamic() if flag then return Mix(Private); else return {}; end end
function CreateReassigned() local result = Mix(Private); if flag then result = {}; end; return result; end
"#;
    let mut m:Manifest=serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap();
    let mut s = selu_manifest_gen::source::Sources::default();
    s.parse("scoped.lua", src).unwrap();
    s.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &[("scoped.lua".into(), src.into())]);
    for name in ["CreateScoped", "CreateAlias"] {
        let f = m.function(name).unwrap();
        let ty = f.signature.as_ref().unwrap().returns[0]
            .ty
            .as_ref()
            .unwrap();
        let w = m.widgets.iter().find(|w| &w.name == ty).unwrap();
        assert!(w.methods.iter().any(|m| m.name == "Acquire"));
        assert!(!w.methods.iter().any(|m| m.name == "Hidden"));
    }
    assert!(m.function("CreateCycle").unwrap().signature.is_none());
    assert!(m.function("CreateDynamic").unwrap().signature.is_none());
    assert!(m.function("CreateReassigned").unwrap().signature.is_none());
}

#[test]
fn iterator_lowering_preserves_state_and_control_values() {
    let m = iterator_fixture();
    let src = r#"const pool = CreateProvenFramePool("Frame"); let total = 0; for (const [frame, active] of pool.EnumerateActive()) { frame.Hide(); total += 1; } const view = CreateProvenScrollBoxListLinearView(); for (const [index, frame] of view.EnumerateFrames()) { total += index; } print(total);"#;
    let wrapped = check(
        &m,
        "const p=CreateProvenFramePool(\"Frame\"); const e=()=>p.EnumerateActive(); for (const f of e()) {}",
    );
    assert!(wrapped.diagnostics.iter().any(|d| d.code == "SELU0357"));
    let p = selu_syntax::parse("iterator.selu", src).unwrap();
    let c = check(&m, src);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let generated = selu_lower::lower(&p, &m, &c.types).lua;
    let lua = mlua::Lua::new();
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(100_000),
        |_, _| Err(mlua::Error::RuntimeError("test instruction limit".into())),
    );
    lua.load(r#"
function InCombatLockdown() return false end
function CreateFrame() return {RegisterEvent=function()end, UnregisterEvent=function()end, SetScript=function()end} end
local a, b = {Hide=function()end}, {Hide=function()end}
function CreateProvenFramePool() return {EnumerateActive=function() return pairs({[a]=true,[b]=true}) end} end
function CreateProvenScrollBoxListLinearView() return {EnumerateFrames=function() return ipairs({a,b}) end} end
function print(value) result = value end
"#).exec().unwrap();
    lua.load(&generated).exec().unwrap();
    assert_eq!(lua.globals().get::<i32>("result").unwrap(), 5);
    let broken = generated.replace(
        "in pool:EnumerateActive() do",
        "in (pool:EnumerateActive()) do",
    );
    assert_ne!(broken, generated);
    assert!(
        lua.load(&broken).exec().is_err(),
        "truncating the triple must fail"
    );
}

#[test]
fn engine_inventory_respects_shadowing_and_reassignment() {
    let mut m:Manifest=serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1,"widgets":[{"name":"Frame","type":"ScriptObject"},{"name":"GameTooltip","type":"ScriptObject"}],"globals":[{"name":"GameTooltip","type":"GameTooltip","evidence":"source"}]})).unwrap();
    let mut source = selu_manifest_gen::source::Sources::default();
    source.parse("calls.lua",r#"local f = CreateFrame("Frame"); f:Missing(); f = Other(); f:NotProven(); function Probe(GameTooltip) GameTooltip:Shadowed(); end; GameTooltip:ClearLines();"#).unwrap();
    let report =
        selu_manifest_gen::coverage::inventory(&m, &source.widget_calls, &source.source_methods);
    assert_eq!(report["remainingEnginePairs"], 2);
    assert!(report["widgets"]["Frame"].get("NotProven").is_none());
    assert!(report["widgets"]["GameTooltip"].get("Shadowed").is_none());
    m.widgets[1].methods.push(
        serde_json::from_value(serde_json::json!({"name":"ClearLines","type":"Function"})).unwrap(),
    );
    assert_eq!(
        selu_manifest_gen::coverage::inventory(&m, &source.widget_calls, &source.source_methods)["remainingEnginePairs"],
        1
    );
}

#[test]
fn reset_callback_does_not_determine_allocator_type() {
    let source = r#"
local Private = {};
function Private:Init(createFunc, resetFunc) self.createFunc = createFunc; self.resetFunc = resetFunc; end
function Private:CallCreate() return self.createFunc(); end
function Private:Acquire() return self:CallCreate(), true; end
function Private:Release(object) end
local function Reset() return CreateFrame("Frame"); end
function CreateUnknown(createFunc) local pool=CreateFromMixins(Private); pool:Init(createFunc,Reset); return pool; end
local function Choice() local create = flag and OtherFactory or CreateFrame; return create(); end
function CreateUnknownChoice() local pool=CreateFromMixins(Private); pool:Init(Choice,Reset); return pool; end
local function FrameChoice() local create = flag and CreateForbiddenFrame or CreateFrame; return create(); end
function CreateKnownChoice() local pool=CreateFromMixins(Private); pool:Init(FrameChoice,Reset); return pool; end
function CreateKnown() local function Allocate() return CreateFrame("Frame"); end; local pool=CreateFromMixins(Private); pool:Init(Allocate,Reset); return pool; end
"#;
    let mut m:Manifest=serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap();
    let mut s = selu_manifest_gen::source::Sources::default();
    s.parse("callbacks.lua", source).unwrap();
    s.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &[("callbacks.lua".into(), source.into())]);
    let unknown = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateUnknown")
        .unwrap();
    assert_ne!(
        unknown
            .methods
            .iter()
            .find(|m| m.name == "Release")
            .unwrap()
            .arguments[0]
            .ty
            .as_deref(),
        Some("Frame")
    );
    let known = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateKnown")
        .unwrap();
    assert_eq!(
        known
            .methods
            .iter()
            .find(|m| m.name == "Release")
            .unwrap()
            .arguments[0]
            .ty
            .as_deref(),
        Some("Frame")
    );
    for (name, expected) in [
        ("SourceObject_CreateUnknownChoice", None),
        ("SourceObject_CreateKnownChoice", Some("Frame")),
    ] {
        let w = m.widgets.iter().find(|w| w.name == name).unwrap();
        let ty = w
            .methods
            .iter()
            .find(|d| d.name == "Release")
            .unwrap()
            .arguments[0]
            .ty
            .as_deref()
            .filter(|s| !s.is_empty());
        assert_eq!(ty, expected, "{name}");
    }
}

#[test]
fn sourced_pool_coverage_at_every_retained_build() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store");
    let store = selu_manifest::store::Store::open(&directory).unwrap();
    for target in store.builds() {
        let m = load_build(&directory, target).unwrap();
        for prefix in ["Create", "CreateSecure", "CreateUnsecured"] {
            for kind in [
                "Frame",
                "Texture",
                "FontString",
                "MaskTexture",
                "Object",
                "Actor",
            ] {
                let name = format!("{prefix}{kind}Pool");
                let Some(f) = m.function(&name) else {
                    continue;
                };
                let d = f
                    .signature
                    .as_ref()
                    .unwrap_or_else(|| panic!("{target}: {name}"));
                let pool = &d.returns[0].ty;
                assert!(
                    pool.as_ref().is_some_and(|s| s.starts_with("W3")),
                    "{target}: {name}"
                );
                let call = match kind {
                    "Frame" => format!(r#"{name}("Button",UIParent)"#),
                    "Object" => format!("{name}(()=>UIParent,()=>{{}})"),
                    "Actor" => format!("{name}(CreateFrame(\"ModelScene\"))"),
                    _ => format!("{name}(UIParent)"),
                };
                let src = format!(
                    "const p={call}; for (const [element,active] of p.EnumerateActive()) {{ const a:boolean=active; }} p.ReleaseAll(); const n:number=p.GetNumActive();"
                );
                let c = check(&m, &src);
                assert!(!c.has_errors(), "{target}: {name}: {:?}", c.diagnostics);
            }
        }
    }
}
