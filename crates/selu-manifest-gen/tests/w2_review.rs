use selu_manifest::Manifest;
fn empty() -> Manifest {
    serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap()
}
fn extracted(src: &str) -> Manifest {
    let mut m = empty();
    let mut s = selu_manifest_gen::source::Sources::default();
    s.parse("probe.lua", src).unwrap();
    s.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &[("probe.lua".into(), src.into())]);
    m
}
#[test]
fn backdrop_forwarded_results() {
    let m = selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap();
    let p=selu_syntax::parse("probe.selu",r#"const f=CreateFrame("Frame",null,UIParent,"BackdropTemplate"); const [r,g,b,a]=f.GetBackdropColor();"#).unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        &m,
        selu_check::CheckOptions { strict_api: true },
    );
    assert!(
        !c.has_errors(),
        "valid forwarded RGBA results rejected: {:?}",
        c.diagnostics
    );
}
#[test]
fn loop_mutation_must_invalidate_proof() {
    let m = extracted(
        "local Mix={}; function Mix:Hide() end; function CreateLoop() local p=CreateFromMixins(Mix); for i=1,1 do p=17 end; return p end",
    );
    assert!(
        m.function("CreateLoop").unwrap().signature.is_none(),
        "number result falsely typed as object: {:?}",
        m.function("CreateLoop").unwrap().signature
    );
}
#[test]
fn branch_member_write_must_invalidate_allocator() {
    let m = extracted(
        "local Mix={}; function Mix:Init() self.createFunc=CreateFrame; if flag then self.createFunc=OtherFactory end end; function Mix:CallCreate() return self.createFunc() end; function Mix:Acquire() return self:CallCreate(),true end; function CreateBranch() local p=CreateFromMixins(Mix); p:Init(); return p end",
    );
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateBranch")
        .unwrap();
    let acquire = w.methods.iter().find(|d| d.name == "Acquire").unwrap();
    assert_ne!(
        acquire.returns[0].ty.as_deref(),
        Some("Frame"),
        "conditional unknown allocator falsely becomes Frame"
    );
}
#[test]
fn unresolved_mixin_cannot_preserve_checked_methods() {
    let m = extracted(
        "local Mix={}; function Mix:Hide() end; function CreatePartial(extra) return CreateFromMixins(Mix,extra) end",
    );
    assert!(
        m.function("CreatePartial").unwrap().signature.is_none(),
        "unknown override ignored: {:?}",
        m.function("CreatePartial").unwrap().signature
    );
}
#[test]
fn shadowed_pairs_is_not_an_iterator() {
    let m = extracted(
        "local pairs=function() return 7 end; TestMixin={}; function TestMixin:Enumerate() return pairs({}) end",
    );
    let d = m
        .function("TestMixin.Enumerate")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert!(
        !d.returns[0].attributes.contains_key("SeluIterator"),
        "shadowed pairs falsely typed as iterator"
    );
    assert_ne!(d.returns[0].ty.as_deref(), Some("Function"));
    let annotated = extracted(
        "local pairs=Other; TestMixin={};\n---@return number\nfunction TestMixin:Read() return 1 end",
    );
    assert!(
        !annotated
            .function("TestMixin.Read")
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .attributes
            .contains_key("SeluUntypedReturnTail")
    );
}
#[test]
fn emmy_comma_returns_and_optional_param() {
    let m = extracted(
        "TestMixin={};\n---@param value number?\n---@return number, string\nfunction TestMixin:Read(value) return 1, 'x' end",
    );
    let d = m
        .function("TestMixin.Read")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert_eq!(
        d.returns.len(),
        2,
        "comma-separated returns truncated: {d:?}"
    );
    assert_eq!(d.returns[0].ty.as_deref(), Some("number"));
    assert_eq!(
        d.arguments[0].attributes.get("Nilable"),
        Some(&serde_json::json!(true))
    );
}
#[test]
fn table_builder_keeps_both_margins() {
    let m = selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap();
    let p = selu_syntax::parse(
        "probe.selu",
        "const builder=CreateTableBuilder(); const [left,right]=builder.GetTableMargins();",
    )
    .unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        &m,
        selu_check::CheckOptions { strict_api: true },
    );
    assert!(
        !c.has_errors(),
        "valid two-result method rejected: {:?}",
        c.diagnostics
    );
}
#[test]
fn iterator_wrapper_must_preserve_triple_or_be_rejected() {
    let m = selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap();
    let src = r#"const pool=CreateFramePool("Frame"); const enumerate=()=>pool.EnumerateActive(); for(const [frame] of enumerate()) { frame.Hide(); }"#;
    let p = selu_syntax::parse("probe.selu", src).unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        &m,
        selu_check::CheckOptions { strict_api: true },
    );
    if c.has_errors() {
        return;
    }
    let generated = selu_lower::lower(&p, &m, &c.types).lua;
    let lua = mlua::Lua::new();
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(10000),
        |_, _| Err(mlua::Error::RuntimeError("instruction limit".into())),
    );
    lua.load(r#"function InCombatLockdown() return false end; function CreateFrame() return {RegisterEvent=function()end,UnregisterEvent=function()end,SetScript=function()end} end; function CreateFramePool() return {EnumerateActive=function() return pairs({[{Hide=function()end}]=true}) end} end"#).exec().unwrap();
    let result = lua.load(&generated).exec();
    assert!(
        result.is_ok(),
        "accepted iterator wrapper loses state/control: {result:?}\n{generated}"
    );
}

#[test]
fn chained_pairs_is_not_a_builtin_iterator() {
    let m = extracted("TestMixin={}; function TestMixin:Enumerate() return pairs({})() end");
    let d = m
        .function("TestMixin.Enumerate")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert!(!d.returns[0].attributes.contains_key("SeluIterator"));
}
#[test]
fn parameter_pairs_is_not_a_builtin_iterator() {
    let m = extracted("TestMixin={}; function TestMixin:Enumerate(pairs) return pairs({}) end");
    let d = m
        .function("TestMixin.Enumerate")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert!(!d.returns[0].attributes.contains_key("SeluIterator"));
}
#[test]
fn unresolved_forwarding_keeps_open_results() {
    let m = extracted("TestMixin={}; function TestMixin:Read() return Other() end");
    let d = m
        .function("TestMixin.Read")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert_eq!(
        d.attributes.get("SeluUntypedReturnTail"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn annotation_nil_union_and_optional_return() {
    let m = extracted(
        "TestMixin={};\n---@param value number | nil\n---@return number?, string\nfunction TestMixin:Read(value) return value, 'x' end",
    );
    let d = m
        .function("TestMixin.Read")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert_eq!(d.arguments[0].ty.as_deref(), Some("number"));
    assert_eq!(
        d.arguments[0].attributes.get("Nilable"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(d.returns[0].ty.as_deref(), Some("number"));
    assert_eq!(
        d.returns[0].attributes.get("Nilable"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn destructuring_cannot_save_an_iterator_value() {
    let m = selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap();
    let p = selu_syntax::parse(
        "probe.selu",
        "const pool=CreateFramePool(\"Frame\"); const [iterator]=pool.EnumerateActive();",
    )
    .unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        &m,
        selu_check::CheckOptions { strict_api: true },
    );
    assert!(c.has_errors());
}

#[test]
fn factory_iterator_requires_lexical_builtin_proof() {
    let m = extracted(
        "local pairs=Other; local Mix={}; function Mix:Enumerate() return pairs({}) end; function CreateShadow() return CreateFromMixins(Mix) end",
    );
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateShadow")
        .unwrap();
    assert!(
        !w.methods
            .iter()
            .find(|d| d.name == "Enumerate")
            .unwrap()
            .returns[0]
            .attributes
            .contains_key("SeluIterator")
    );
}
