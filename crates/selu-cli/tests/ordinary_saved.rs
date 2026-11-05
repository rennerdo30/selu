mod common;
use selu_sim::{Sim, SimValue};

fn source() -> &'static str {
    r#"type Settings = {value: string | number; pos: {p?:string; x?:number; y?:number}};
    saved global DB: Settings = {value:0,pos:{}};
    function attach() { DB.whenReady((data) => { print("writer"); data.pos.x = 0; }); }
    attach();
    on PLAYER_LOGIN() { print(DB.status); attach(); }
    "#
}
#[test]
fn ordinary_union_valid_invalid_and_future_are_closed_and_client_ordered() {
    let code = common::compile(source());
    for (value, version, status) in [
        ("17", 1, "ready"),
        ("'text'", 1, "ready"),
        ("false", 1, "corrupt"),
        ("17", 2, "newer"),
    ] {
        let sim = Sim::new().unwrap();
        sim.eval("__selu_saved_names.DB=true").unwrap();
        let initial = format!(
            "DB={{__selu_version={version},value={value},pos={{p='BOTTOM',x=0}},opaque={{[0]='zero',['0']=false}}}}; root=DB; position=DB.pos"
        );
        let baseline = common::SavedVm::new();
        baseline.eval(&initial).unwrap();
        let before = baseline.logout_saved(&["DB"]).unwrap();
        sim.load_addon_with_restore("Addon", &[("main.lua".into(), code.clone())], |sim| {
            sim.eval("assert(DB==nil)")?;
            sim.fire(
                "ADDON_LOADED",
                vec![SimValue::String(sim.lua().create_string("Other")?)],
            )?;
            assert!(sim.prints().is_empty());
            sim.eval(&initial)
        })
        .unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        sim.eval("assert(DB==root and DB.pos==position and DB.pos.y==nil and DB.pos.p=='BOTTOM')")
            .unwrap();
        assert_eq!(
            sim.prints().iter().filter(|s| *s == "writer").count(),
            if status == "ready" { 2 } else { 0 }
        );
        assert!(sim.prints().iter().any(|s| s == status));
        assert_eq!(
            sim.prints()
                .iter()
                .filter(|s| s.starts_with("[saved] DB:"))
                .count(),
            if status == "ready" { 0 } else { 1 }
        );
        assert_eq!(
            serde_json::to_vec(&sim.logout_saved(&["DB"]).unwrap().0).unwrap(),
            serde_json::to_vec(&before.0).unwrap()
        );
    }
}
#[test]
fn inferred_schema_refuses_malformed_data_and_preserves_aliases() {
    let src = r#"saved prefs = {left:{x:1}, right:{y:2}};
        function attach(){prefs.whenReady((data)=>{print("writer");});} attach();
        on PLAYER_LOGIN(){print(prefs.status);}"#;
    for (input, status) in [
        (
            "local shared={}; Addon_prefs={left=shared,right=shared}",
            "ready",
        ),
        (
            "local shared={x='bad'}; Addon_prefs={left=shared,right=shared}",
            "corrupt",
        ),
        ("Addon_prefs=false", "corrupt"),
    ] {
        let sim = common::SavedVm::new();
        sim.eval(input).unwrap();
        sim.eval("root=Addon_prefs").unwrap();
        let before = sim.logout_saved(&["Addon_prefs"]).unwrap();
        sim.load_addon("Addon", &[("main.lua".into(), common::compile(src))])
            .unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert!(sim.prints().iter().any(|s| s == status));
        sim.eval("assert(Addon_prefs==root)").unwrap();
        if status == "ready" {
            sim.eval("assert(Addon_prefs.left==Addon_prefs.right and Addon_prefs.left.x==1 and Addon_prefs.left.y==2)").unwrap();
        } else {
            assert!(!sim.prints().iter().any(|s| s == "writer"));
            assert_eq!(
                serde_json::to_vec(&sim.logout_saved(&["Addon_prefs"]).unwrap().0).unwrap(),
                serde_json::to_vec(&before.0).unwrap()
            );
        }
    }
}
#[test]
fn ordinary_failed_migration_preserves_original_graph_and_suppresses_writer() {
    let src = r#"saved global DB version 3 = {x:0};
    migrate DB 1 -> 2(old:{x:number}) {old.x=8;}
    migrate DB 2 -> 3(old:{x:number}) {throw new Error("stop");}
    function attach(){DB.whenReady((data)=>{data.x=99;print("writer");});}attach();
    on PLAYER_LOGIN(){print(DB.status);}"#;
    let sim = common::SavedVm::new();
    sim.eval("DB={x=7,__selu_version=1};root=DB").unwrap();
    let before = sim.logout_saved(&["DB"]).unwrap();
    sim.load_addon("Addon", &[("main.lua".into(), common::compile(src))])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("assert(DB==root and DB.x==7 and DB.__selu_version==1)")
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&sim.logout_saved(&["DB"]).unwrap().0).unwrap(),
        serde_json::to_vec(&before.0).unwrap()
    );
    assert!(sim.prints().iter().any(|s| s == "failed"));
    assert!(!sim.prints().iter().any(|s| s == "writer"));
}

#[test]
fn ordinary_values_and_migration_inputs_cannot_bypass_checking() {
    assert!(
        !common::codes("saved settings={x:1}; on PLAYER_LOGIN(){print(settings.x);}").is_empty()
    );
    assert!(
        common::codes("saved settings version 2={x:1}; migrate settings 1 -> 2(old){old.x=2;}")
            .contains(&"SELU0901".to_string())
    );
}
#[test]
fn mapped_reserved_spelling_keeps_mapped_last_success_contract() {
    let src = r#"saved global DB version 3 using __selu_version = {x:0};
        migrate DB 1 -> 2(old:{x:number}) {old.x=8;}
        migrate DB 2 -> 3(old:{x:number}) {throw new Error("stop");}
        on PLAYER_LOGIN(){print(DB.status);}"#;
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.DB=true; DB={x=7,__selu_version=1}; root=DB")
        .unwrap();
    sim.load_addon("Addon", &[("main.lua".into(), common::compile(src))])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("assert(DB==root and DB.x==8 and DB.__selu_version==2)")
        .unwrap();
    assert!(sim.prints().iter().any(|s| s == "failed"));
}
