use selu_manifest::{Manifest, load_manifest};
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
fn direct_api_array_loop_remains_valid() {
    let m = load_manifest(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/retail/12.1.5.70391.selum"),
    )
    .unwrap();
    let p = selu_syntax::parse(
        "probe.selu",
        "for (const id of C_MountJournal.GetMountIDs()) { const n: number = id; }",
    )
    .unwrap();
    let c = selu_check::check_program_with_options(
        &p,
        &m,
        selu_check::CheckOptions { strict_api: true },
    );
    assert!(
        !c.has_errors(),
        "direct API array iteration regressed: {:?}",
        c.diagnostics
    );
}
#[test]
fn conditional_call_must_invalidate_object_proof() {
    let m = extracted(
        "local Mix={}; function Mix:Hide() end; function Mix:Break() self.Hide=17 end; function CreateConditional() local p=CreateFromMixins(Mix); if flag then p:Break() end; return p end",
    );
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateConditional");
    assert!(
        !w.is_some_and(|w| w.methods.iter().any(|d| d.name == "Hide")),
        "skipped effectful call preserves callable Hide: {w:?}"
    );
}
#[test]
fn loop_local_alias_write_must_invalidate_object_proof() {
    let m = extracted(
        "local Mix={}; function Mix:Hide() end; function CreateAlias() local p=CreateFromMixins(Mix); for i=1,1 do local q=p; q.Hide=17 end; return p end",
    );
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateAlias");
    assert!(
        !w.is_some_and(|w| w.methods.iter().any(|d| d.name == "Hide")),
        "skipped local alias preserves callable Hide: {w:?}"
    );
}
#[test]
fn iterator_shadowing_is_independent_between_builtins() {
    let m = extracted(
        "local ipairs=Other; TestMixin={}; function TestMixin:Enumerate() return pairs({}) end",
    );
    let d = m
        .function("TestMixin.Enumerate")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert!(
        d.returns[0].attributes.contains_key("SeluIterator"),
        "unrelated ipairs shadowing erased pairs proof: {d:?}"
    );
}

#[test]
fn escaped_objects_do_not_retain_callable_members() {
    for effect in [
        "if flag then Other(p) end",
        "if flag then Other({p}) end",
        "if flag then local q=p; q.Hide=17 end",
        "while flag do local q=p; q.Hide=17 end",
        "repeat local q=p; q.Hide=17 until flag",
        "for k,v in pairs({}) do local q=p; q.Hide=17 end",
        "local function Break() p.Hide=17 end; if flag then Break() end",
        "Other(p)",
        "Shared=p; Other()",
        "Unknown.target=p; Other()",
        "local box={child=p}; local q=flag and box or {}; Other(q['child'])",
        "local box={child=p}; local q=flag and box or {}; Other(q.child)",
        "local box={}; if flag then box.child=p end; Other(box.child)",
        "Other({p})",
        "Other {p}",
        "local container={child=p}; if flag then Other(container) end",
        "local container={child=p}; Other(container)",
        "Other(function() p.Hide=17 end)",
        "local q={child=p}; if flag then q.child.Hide=17 end",
        "for i=1,Other(p) do end",
    ] {
        let m = extracted(&format!(
            "local Mix={{}}; function Mix:Hide() end; function CreateEscape() local p=CreateFromMixins(Mix); {effect}; return p end"
        ));
        assert!(
            !m.widgets
                .iter()
                .find(|w| w.name == "SourceObject_CreateEscape")
                .is_some_and(|w| w.methods.iter().any(|d| d.name == "Hide")),
            "false Hide after {effect}"
        );
    }
}
#[test]
fn shadowing_is_independent_in_both_directions_and_scopes() {
    for src in [
        "local pairs=Other; TestMixin={}; function TestMixin:Enumerate() return ipairs({}) end",
        "TestMixin={}; function TestMixin:Enumerate(ipairs) return pairs({}) end",
        "TestMixin={}; function TestMixin:Enumerate() local ipairs=Other; return pairs({}) end",
        "TestMixin={}; function TestMixin:Enumerate() do local pairs=Other; pairs=Other end; return pairs({}) end",
        "TestMixin={}; function TestMixin:Enumerate() local function Unused(pairs) return pairs({}) end; return pairs({}) end",
    ] {
        let m = extracted(src);
        assert!(
            m.function("TestMixin.Enumerate")
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .returns[0]
                .attributes
                .contains_key("SeluIterator"),
            "{src}"
        );
    }
}

#[test]
fn proof_simulations_preserve_aliases_without_polluting_other_proofs() {
    let m = extracted(
        "local Mix={}; function Mix:Hide() end; function Mix:Break() self.Hide=17 end; function CreateInitialA() return CreateFromMixins(Mix) end; function CreateInitialB() return CreateFromMixins(Mix) end; function CreateMutated() local p=CreateFromMixins(Mix); if flag then p:Break() end; return p end",
    );
    for n in ["SourceObject_CreateInitialA", "SourceObject_CreateInitialB"] {
        let w = m.widgets.iter().find(|w| w.name == n).unwrap();
        assert!(w.methods.iter().any(|d| d.name == "Hide"));
    }
    assert!(
        !m.widgets
            .iter()
            .any(|w| w.name == "SourceObject_CreateMutated"
                && w.methods.iter().any(|d| d.name == "Hide"))
    );
}

#[test]
fn helpers_are_followed_across_files_without_assuming_missing_effects() {
    for (helper, expected) in [
        (
            "function Initialize(p) p.counter=17 end; if flag then function Initialize(p) p.Hide=17 end end; External={Init=Initialize}",
            false,
        ),
        (
            "External={}; local function Initialize(p) p.counter=17 end; External.Init=Initialize",
            true,
        ),
        (
            "local saved={}; External={}; function External.Init(p) saved[1]=p end",
            true,
        ),
        (
            "local saved={}; External={}; function External.Init(p) saved[1]=p; Unknown(saved[1]) end",
            false,
        ),
        (
            "local saved={}; External={}; function External.Init(p) saved[1]=p; local q=saved[1]; q.Hide=17 end",
            false,
        ),
        (
            "External={}; function External.Init(p) p.cache={}; p.cache[1]=17 end",
            true,
        ),
        (
            "External={}; function External.Init(p) if false then Unknown(p) end end",
            true,
        ),
        (
            "External={}; function External.Init(p) p.cache={}; for i=1,1 do p.cache[i]=17 end end",
            true,
        ),
        (
            "External={}; function External:Init() self.counter=17 end",
            true,
        ),
        (
            "External={}; function External:Init() local q=self; q.Hide=17 end",
            false,
        ),
        (
            "External={}; function External:Init() Unknown(self) end",
            false,
        ),
        (
            "External={}; function External:Init() Unknown:Call(self) end",
            false,
        ),
        (
            "External={}; function External:Init() return External.Init(self) end",
            false,
        ),
    ] {
        for call in ["External.Init(p)", "if flag then External.Init(p) end"] {
            let main = format!(
                "local M={{}}; function M:Hide() end; function CreateWithHelper() local p=CreateFromMixins(M); {call}; return p end"
            );
            let files: Vec<(String, String)> = vec![
                ("helper.lua".into(), helper.into()),
                ("main.lua".into(), main),
            ];
            let mut m = empty();
            let mut source = selu_manifest_gen::source::Sources::default();
            for (file, text) in &files {
                source.parse(file, text).unwrap();
            }
            source.extend_manifest(&mut m);
            selu_manifest_gen::factory::extend(&mut m, &files);
            let actual = m
                .widgets
                .iter()
                .find(|w| w.name == "SourceObject_CreateWithHelper")
                .is_some_and(|w| w.methods.iter().any(|d| d.name == "Hide"));
            assert_eq!(actual, expected, "{helper}");
        }
    }
}

#[test]
fn allocator_refinement_is_not_an_extra_constructor_effect() {
    let m = extracted(
        "local M={}; function M:Init() self.count=0 end; function M:Hide() end; function CreateNoAllocator() local p=CreateFromMixins(M); p:Init(); return p end",
    );
    assert!(
        m.widgets
            .iter()
            .find(|w| w.name == "SourceObject_CreateNoAllocator")
            .is_some_and(|w| w.methods.iter().any(|d| d.name == "Hide"))
    );
}

#[test]
fn lazy_helpers_keep_factory_ownership_in_their_source_file() {
    let files:Vec<(String,String)>=vec![
 ("main.lua".into(),"local unused=External.Read(); local M={}; function M:Hide() end; function CreateMain() return CreateFromMixins(M) end".into()),
 ("helper.lua".into(),"local M={}; function M:Read() return 1 end; function CreateHelper() return CreateFromMixins(M) end; External={}; function External.Read() return CreateHelper() end".into())];
    let mut m = empty();
    let mut source = selu_manifest_gen::source::Sources::default();
    for (file, text) in &files {
        source.parse(file, text).unwrap();
    }
    source.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &files);
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateHelper")
        .unwrap();
    assert_eq!(
        w.attributes.get("file"),
        Some(&serde_json::json!("helper.lua"))
    );
}

#[test]
fn explicit_constructor_methods_extend_the_proven_proxy_surface() {
    let m = extracted(
        "local M={}; function M:Acquire() end; function M:Hidden() end; local P=ProxyUtil.CreateProxyMixin(nil,M,{'Acquire'}); P.__index=P; function CreateExtended() local private=CreateFromMixins(M); local p=ProxyUtil.CreateProxy(private,P); p.Extra=function() return 1 end; return p end",
    );
    let w = m
        .widgets
        .iter()
        .find(|w| w.name == "SourceObject_CreateExtended")
        .unwrap();
    assert!(w.methods.iter().any(|d| d.name == "Acquire"));
    assert!(w.methods.iter().any(|d| d.name == "Extra"));
    assert!(!w.methods.iter().any(|d| d.name == "Hidden"));
}

#[test]
fn ambiguous_factory_bodies_do_not_publish_a_single_positive_contract() {
    let files:Vec<(String,String)>=vec![
 ("one.lua".into(),"local M={}; function M:Hide() end; function CreateAmbiguous() return CreateFromMixins(M) end".into()),
 ("two.lua".into(),"local M={}; function CreateAmbiguous() return 17 end; function CreateOther() return CreateFromMixins(M) end".into())];
    let mut m = empty();
    let mut source = selu_manifest_gen::source::Sources::default();
    for (file, text) in &files {
        source.parse(file, text).unwrap();
    }
    source.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &files);
    assert!(m.function("CreateAmbiguous").unwrap().signature.is_none());
}
