//! Review regressions execute generated Lua against synthetic saved graphs.
mod common;
use selu_sim::Sim;

fn source(defaults: &str, version: u32, migrations: &str) -> String {
    format!(
        r#"saved global DB version {version} using dbVersion legacy OldDB = {defaults};
        {migrations}
        function attach() {{ DB.whenReady((data) => {{ print("writer"); }}); }}
        attach();
        on PLAYER_LOGIN() {{ print(DB.status); attach(); }}"#
    )
}
fn vm(input: &str) -> Sim {
    let sim = Sim::new().unwrap();
    sim.eval("__selu_saved_names.DB=true; __selu_saved_names.OldDB=true")
        .unwrap();
    sim.eval(input).unwrap();
    sim
}
fn load(sim: &Sim, source: &str) {
    sim.load_addon("Addon", &[("main.lua".into(), common::compile(source))])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
}

fn assert_alias_refused(defaults: &str) {
    let sim =
        vm("shared={}; DB={dbVersion=1,obj=shared,map=shared,extension={keep=false}}; root=DB");
    let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
    load(&sim, &source(defaults, 1, ""));
    assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
    sim.eval("assert(DB==root and DB.obj==shared and DB.map==shared and shared.x==nil)")
        .unwrap();
    assert!(!sim.prints().iter().any(|s| s == "writer"));
    assert!(sim.prints().iter().any(|s| s == "corrupt"));
}
#[test]
fn aliased_record_and_map_defaults_are_transactional() {
    assert_alias_refused(r#"{ obj:{x:"oops"}, map:new Map<string,number>() }"#);
}
#[test]
fn aliased_records_with_conflicting_defaults_are_transactional() {
    assert_alias_refused(r#"{ obj:{x:"oops"}, map:{x:42} }"#);
}
#[test]
fn compatible_aliased_defaults_preserve_identity() {
    let sim = vm("shared={}; DB={dbVersion=1,obj=shared,map=shared}; root=DB");
    load(
        &sim,
        &source("{obj:{x:9},map:new Map<string,number>()}", 1, ""),
    );
    sim.eval("assert(DB==root and DB.obj==shared and DB.map==shared and shared.x==9)")
        .unwrap();
    assert_eq!(sim.prints(), vec!["writer", "ready", "writer"]);
}

#[test]
fn adoption_warns_once_with_database_reason_and_step() {
    for (input, version, migrations, status, reason, step, writers) in [
        (
            "DB={dbVersion=2,count=7}",
            1,
            "",
            "newer",
            "newer",
            "load",
            0,
        ),
        (
            "DB={dbVersion=1,count='bad'}",
            1,
            "",
            "corrupt",
            "expected number",
            "validation",
            0,
        ),
        (
            "DB={dbVersion=1,count=7}; OldDB={history={kept=false}}",
            1,
            "",
            "collision",
            "distinct current and legacy",
            "publication",
            2,
        ),
        (
            "DB={dbVersion=1,count=7}",
            2,
            r#"migrate DB 1 -> 2 (old:{count:number}) { old.count+=1; throw new Error("SA migration boom"); }"#,
            "failed",
            "SA migration boom",
            "1 -> 2",
            0,
        ),
    ] {
        let sim = vm(input);
        let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        load(&sim, &source("{count:0}", version, migrations));
        assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
        let messages = sim.prints();
        let warnings = messages
            .iter()
            .filter(|s| s.starts_with("[saved] DB:"))
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 1, "{messages:?}");
        assert!(warnings[0].contains(reason), "{messages:?}");
        assert!(warnings[0].contains(step), "{messages:?}");
        assert!(messages.iter().any(|s| s == status));
        assert_eq!(
            messages.iter().filter(|s| s.as_str() == "writer").count(),
            writers
        );
    }
}

#[test]
fn migration_context_names_are_checked_before_lua_emission() {
    for context in ["repeat", "__selu_context"] {
        let src = source(
            "{count:0}",
            2,
            &format!(
                "migrate DB 1 -> 2 (old:{{count:number}}, {context}:{{count:number}}) {{ old.count+=1; }}"
            ),
        );
        let diagnostics = common::codes(&src);
        if diagnostics.is_empty() {
            let sim = vm("DB={dbVersion=1,count=7}");
            let lua = common::compile(&src);
            assert!(
                sim.load_addon("Addon", &[("main.lua".into(), lua)]).is_ok(),
                "reserved context emitted invalid Lua: {context}"
            );
        }
        assert!(
            diagnostics.iter().any(|c| c == "SELU0463"),
            "reserved context accepted: {context}"
        );
    }
    let sim = vm("DB={dbVersion=1,count=7}");
    load(
        &sim,
        &source(
            "{count:0}",
            2,
            "migrate DB 1 -> 2 (old:{count:number}, original:{count:number}) { old.count+=1; }",
        ),
    );
    sim.eval("assert(DB.dbVersion==2 and DB.count==8)").unwrap();
}

#[test]
fn diagnostics_use_existing_warning_runtime_without_persistent_attachment() {
    for enabled in [true, false] {
        let sim = vm("DB={dbVersion=2,count=7}; warnings={}");
        let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        let logger = format!(
            r#"local _, ns = ...
            ns.__selu_wow = {{
                enabled=function(level) assert(level=="warn"); return {enabled} end,
                write=function(level,message) assert(level=="warn"); warnings[#warnings+1]=message end
            }}"#
        );
        sim.load_addon(
            "Addon",
            &[
                ("logger.lua".into(), logger),
                (
                    "main.lua".into(),
                    common::compile(&source("{count:0}", 1, "")),
                ),
            ],
        )
        .unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(sim.prints(), vec!["newer"]);
        sim.eval(if enabled {
            "assert(#warnings==1 and string.find(warnings[1], '[saved] DB:', 1, true))"
        } else {
            "assert(#warnings==0)"
        })
        .unwrap();
        assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
    }
}
