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
fn vm(input: &str) -> common::SavedVm {
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.DB=true; __selu_saved_names.OldDB=true")
        .unwrap();
    sim.eval(input).unwrap();
    sim
}
fn load(sim: &common::SavedVm, source: &str) {
    sim.load_addon("Addon", &[("main.lua".into(), common::compile(source))])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
}

#[test]
fn client_order_future_current_and_corrupt_logout_preserve_restored_root() {
    let migrations = (1..10)
        .map(|v| format!("migrate DB {v} -> {} (old:{{count:number}}) {{}}", v + 1))
        .collect::<String>();
    let src = source("{count:0}", 10, &migrations);
    for (input, status) in [
        (
            "DB={dbVersion=11,count=73,extension={['0']=false,[0]='kept'}}; root=DB",
            "newer",
        ),
        (
            "DB={dbVersion=10,count=73,extension={['0']=false,[0]='kept'}}; root=DB",
            "ready",
        ),
        (
            "DB={dbVersion=10,count='corrupt',extension={['0']=false,[0]='kept'}}; root=DB",
            "corrupt",
        ),
    ] {
        let sim = vm(input);
        let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        let before_bytes = serde_json::to_vec(&before.0).unwrap();
        load(&sim, &src);
        sim.eval("assert(DB==root)").unwrap();
        assert!(sim.prints().iter().any(|s| s == status));
        assert_eq!(
            sim.prints().iter().filter(|s| *s == "writer").count(),
            if status == "ready" { 2 } else { 0 }
        );
        let after = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        assert_eq!(serde_json::to_vec(&after.0).unwrap(), before_bytes);
    }
}

#[test]
fn adoption_waits_for_own_event_and_publishes_the_saved_root() {
    for input in [
        "",
        "DB={dbVersion=1,count=7}",
        "OldDB={dbVersion=1,count=7}",
    ] {
        let sim = Sim::new().unwrap();
        sim.eval("__selu_saved_names.DB=true; __selu_saved_names.OldDB=true")
            .unwrap();
        let src = "saved global DB using dbVersion legacy OldDB = {count:0}; function attach() { DB.whenReady(data => { data.count+=1; print(\"writer\"); }); } attach(); on PLAYER_LOGIN() { print(DB.status); }";
        sim.load_addon_with_restore(
            "Addon",
            &[("main.lua".into(), common::compile(src))],
            |sim| {
                sim.eval("assert(DB==nil and OldDB==nil)")?;
                sim.fire(
                    "ADDON_LOADED",
                    vec![selu_sim::SimValue::String(
                        sim.lua().create_string("OtherAddon")?,
                    )],
                )?;
                sim.eval("assert(DB==nil and OldDB==nil)")?;
                assert!(sim.prints().is_empty());
                sim.eval(input)?;
                sim.eval("root=DB or OldDB or false")?;
                sim.fire(
                    "ADDON_LOADED",
                    vec![selu_sim::SimValue::String(
                        sim.lua().create_string("OtherAddon")?,
                    )],
                )?;
                sim.eval("assert((DB or OldDB or false)==root)")?;
                assert!(sim.prints().is_empty());
                Ok(())
            },
        )
        .unwrap();
        sim.eval("assert(OldDB==nil and DB.dbVersion==1); if root then assert(DB==root and root.count==8) else assert(DB.count==1) end").unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        sim.fire(
            "ADDON_LOADED",
            vec![selu_sim::SimValue::String(
                sim.lua().create_string("Addon").unwrap(),
            )],
        )
        .unwrap();
        assert_eq!(sim.prints(), ["writer", "ready"]);
    }
}

#[test]
fn saved_repair_requires_a_closed_shape_and_kind_guard() {
    for historical in [
        "SavedRepair<unknown>",
        "SavedRepair<number>",
        "SavedRepair<() => number>",
        "SavedRepair<Map<string,unknown>>",
    ] {
        let src = source(
            "{count:0}",
            2,
            &format!("migrate DB 1 -> 2 (old:{{value:{historical}}}) {{}}"),
        );
        assert!(
            common::codes(&src).iter().any(|c| c == "SELU0901"),
            "{historical}"
        );
    }
    for statement in [
        "print(value.count);",
        "const narrowed:{count:number}=value;",
    ] {
        let src = source(
            "{count:0}",
            2,
            &format!(
                "migrate DB 1 -> 2 (old:{{value:SavedRepair<{{count:number}}>}}) {{ const value=old.value; {statement} }}"
            ),
        );
        assert!(
            !common::codes(&src).is_empty(),
            "unguarded repair input accepted: {statement}"
        );
    }
}

#[test]
fn toolkit_migration_five_repairs_only_declared_wrong_kinds() {
    let src = r#"
        type Character = { ownedValue?:string; restoreValue?:string };
        type Characters = Map<string, Character>;
        type Cooldown = { characters?:SavedRepair<Characters> };
        type Historical = { cooldownText?:SavedRepair<Cooldown> };
        saved global DB version 5 using dbVersion = {
            cooldownText:{ characters:new Map<string,Character>() }
        };
        migrate DB 1 -> 2 (old:Historical) {}
        migrate DB 2 -> 3 (old:Historical) {}
        migrate DB 3 -> 4 (old:Historical) {}
        migrate DB 4 -> 5 (old:Historical) {
            const container = old.cooldownText;
            if (typeof container != "table") { old.cooldownText = {}; }
            const settings = old.cooldownText;
            if (typeof settings == "table") {
                const characters = settings.characters;
                if (typeof characters != "table") {
                    settings.characters = new Map<string,Character>();
                }
            }
        }
        on PLAYER_LOGIN() { print(DB.status); }
    "#;
    for value in [
        "nil",
        "false",
        "42",
        "'broken'",
        "{}",
        "{characters=false}",
        "{characters=42}",
        "{characters='broken'}",
        "{characters={Alice={ownedValue='1',restoreValue='0'}}}",
    ] {
        let sim = vm(&format!(
            "DB={{dbVersion=4,cooldownText={value},extension={{keep=false}}}}; root=DB; cooldown=DB.cooldownText; characters=type(cooldown)=='table' and cooldown.characters; extension=DB.extension"
        ));
        load(&sim, src);
        sim.eval("assert(DB==root and DB.dbVersion==5 and DB.extension==extension and extension.keep==false); assert(type(DB.cooldownText)=='table' and type(DB.cooldownText.characters)=='table'); if type(cooldown)=='table' then assert(DB.cooldownText==cooldown) end; if type(characters)=='table' then assert(DB.cooldownText.characters==characters and characters.Alice.restoreValue=='0') end").unwrap();
        assert_eq!(sim.prints(), ["ready"]);
    }
    // A malformed table element is outside this union; migration 5 never repairs it.
    let sim = vm("DB={dbVersion=4,cooldownText={characters={Alice=false}}}; root=DB");
    let before = sim.logout_saved(&["DB"]).unwrap();
    load(&sim, src);
    assert_eq!(sim.logout_saved(&["DB"]).unwrap(), before);
    assert!(sim.prints().iter().any(|s| s == "corrupt"));
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
fn empty_current_root_does_not_hide_a_filled_legacy_root() {
    let sim = vm("DB={}; OldDB={dbVersion=1,count=7}; root=OldDB");
    load(&sim, &source("{count:0}", 1, ""));
    sim.eval("assert(DB==root and OldDB==nil and DB.count==7)")
        .unwrap();
    assert_eq!(sim.prints(), vec!["writer", "ready", "writer"]);
    // An empty legacy table changes nothing: the current root stays.
    let sim = vm("DB={dbVersion=1,count=3}; OldDB={}; root=DB");
    load(&sim, &source("{count:0}", 1, ""));
    sim.eval("assert(DB==root and DB.count==3)").unwrap();
}

#[test]
fn text_version_stamp_is_read_as_its_number() {
    let sim = vm("DB={dbVersion='1',count=7}; root=DB");
    load(
        &sim,
        &source(
            "{count:0}",
            2,
            "migrate DB 1 -> 2 (old:{count:number}) { old.count+=1; }",
        ),
    );
    sim.eval("assert(DB==root and DB.count==8 and DB.dbVersion==2)")
        .unwrap();
    let sim = vm("DB={dbVersion='2',count=7}; root=DB");
    load(
        &sim,
        &source("{count:0}", 2, "migrate DB 1 -> 2 (old:{count:number}) {}"),
    );
    sim.eval("assert(DB==root and DB.count==7 and DB.dbVersion==2)")
        .unwrap();
    for stamp in ["'twelve'", "'1.5'", "1.5", "true"] {
        let sim = vm(&format!("DB={{dbVersion={stamp},count=7}}; root=DB"));
        let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        load(&sim, &source("{count:0}", 1, ""));
        assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
        assert!(sim.prints().iter().any(|s| s == "corrupt"), "{stamp}");
    }
}

#[test]
fn scalar_where_a_default_table_belongs_is_replaced_with_one_warning() {
    let defaults = "{count:0, box:{a:1, inner:{b:2}}, names:new Map<string,number>()}";
    let sim = vm("DB={dbVersion=1,count=7,box={a=5,inner='gone'},names=3}; root=DB; box=DB.box");
    load(&sim, &source(defaults, 1, ""));
    sim.eval("assert(DB==root and DB.box==box and box.a==5 and box.inner.b==2 and next(DB.names)==nil and DB.count==7)")
        .unwrap();
    let messages = sim.prints();
    let warnings = messages
        .iter()
        .filter(|s| s.starts_with("[saved] DB:"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 2, "{messages:?}");
    assert!(
        warnings
            .iter()
            .any(|s| s.contains("$.box.inner held a string")),
        "{messages:?}"
    );
    assert!(
        warnings.iter().any(|s| s.contains("$.names held a number")),
        "{messages:?}"
    );
    assert!(messages.iter().any(|s| s == "ready"));
    // A scalar of the wrong type in a scalar field is still refused untouched.
    let sim = vm("DB={dbVersion=1,count='bad',box=4}; root=DB");
    let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
    load(&sim, &source(defaults, 1, ""));
    assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
    assert!(sim.prints().iter().any(|s| s == "corrupt"));
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

fn shared_map_validation_visits(lua: String) -> (common::SavedVm, Vec<String>, usize) {
    let sim = vm(r#"
        node=1;
        for i=1,6 do node={a=node,b=node,c=node,d=node,e=node,f=node,g=node,h=node} end;
        DB={dbVersion=1,items=node}; root=DB; top=node;
    "#);
    // Install instrumentation before generated chunks capture pairs. Saved input
    // remains staged in SavedVm until after all chunks have executed.
    let counter = r#"
        visits=0; counting=true;
        local original=pairs;
        pairs=function(t)
            local iter,state,key=original(t);
            return function(state,key)
                local next_key,value=iter(state,key);
                if next_key~=nil and counting then
                    visits=visits+1;
                    if visits>10000 then counting=false; error('10000 visit safety cap') end
                end
                return next_key,value;
            end,state,key;
        end
    "#;
    sim.load_addon(
        "Addon",
        &[
            ("counter.lua".into(), counter.into()),
            ("main.lua".into(), lua),
        ],
    )
    .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("counting=false; print(visits)").unwrap();
    let messages = sim.prints();
    let visits = messages.last().unwrap().parse::<usize>().unwrap();
    (sim, messages, visits)
}

#[test]
fn shared_map_validation_visits_are_bounded_including_strict_pass() {
    let mut ty = "number".to_string();
    for _ in 0..6 {
        ty = format!("Map<string,{ty}>");
    }
    let src = source(&format!("{{items:new {ty}()}}"), 1, "");
    let lua = common::compile(&src);
    let (sim, messages, visits) = shared_map_validation_visits(lua.clone());
    // 50 unique input entries: two root keys and six maps of eight edges.
    // Eight traversals cover both validation passes, clones, commit and defaults.
    let visit_bound = 8 * (2 + 6 * 8);
    println!("shared six-level graph: {visits} iterator visits; bound {visit_bound}");
    assert!(
        visits > 0 && visits <= visit_bound,
        "{visits} visits outside 1..={visit_bound}: {messages:?}"
    );
    assert_eq!(
        &messages[..messages.len() - 1],
        ["writer", "ready", "writer"]
    );
    sim.eval("assert(DB==root and DB.items==top and DB.dbVersion==1); local cursor=top; for i=1,6 do assert(cursor.a==cursor.b and cursor.a==cursor.c and cursor.a==cursor.d and cursor.a==cursor.e and cursor.a==cursor.f and cursor.a==cursor.g and cursor.a==cursor.h); cursor=cursor.a end; assert(cursor==1)").unwrap();

    // The same measured workload must fail the bound if successful-validation
    // memoisation is removed. The counter's safety cap bounds the mutation run.
    let memo_guard = "if __selu_modes and __selu_modes[__selu_missing] then return true end";
    assert_eq!(lua.matches(memo_guard).count(), 1);
    let mutated = lua.replacen(memo_guard, "if false then return true end", 1);
    let (_, messages, visits) = shared_map_validation_visits(mutated);
    println!("memo disabled: {visits} iterator visits; bound {visit_bound}");
    assert!(
        visits > visit_bound,
        "mutation did not fail the bound: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|s| s.contains("10000 visit safety cap")),
        "mutation did not hit its safety cap: {messages:?}"
    );
}

#[test]
fn validation_memo_distinguishes_schema_mode_and_active_paths() {
    // Expose the private validator only in this test VM, without a source API.
    let lua = common::compile(&source("{count:0}", 1, ""));
    let marker = "-- Copy with provenance";
    let instrumented = lua.replace(
        marker,
        &format!("__selu_test_validate=__selu_saved_validate\n{marker}"),
    );
    assert_ne!(lua, instrumented);
    let sim = vm("DB={dbVersion=1,count=7}");
    sim.load_addon("Addon", &[("main.lua".into(), instrumented)])
        .unwrap();
    sim.eval(
        r#"
        local memo={}; local numeric={fields={x="number"}}; local text={fields={x="string"}};
        local empty={};
        assert(__selu_test_validate(empty,numeric,true,{},"$",memo));
        assert(not __selu_test_validate(empty,numeric,false,{},"$",memo));
        local active={[empty]=true};
        local ok,reason=__selu_test_validate(empty,numeric,true,active,"$",memo);
        assert(not ok and string.find(reason,"cyclic record",1,true));
        assert(__selu_test_validate(empty,numeric,true,{},"$",memo));
        local shared={x=7};
        assert(__selu_test_validate(shared,numeric,false,{},"$",memo));
        assert(not __selu_test_validate(shared,text,false,{},"$",memo));
        local failed={x=false};
        assert(not __selu_test_validate(failed,numeric,false,{},"$",memo));
        failed.x=7;
        assert(__selu_test_validate(failed,numeric,false,{},"$",memo));
    "#,
    )
    .unwrap();
}
#[test]
fn sd_contextual_empty_defaults() {
    for options in ["", "using dbVersion"] {
        let src = format!(
            r#"saved global DB {options}: {{ items:number[]; nested:{{ rows:number[][]; partial:{{x?:number;y?:number}} }}; maps:Map<string,number[]>; optional?:number[] }} = {{ items:[], nested:{{ rows:[[]], partial:{{x:2}} }}, maps:new Map<string,number[]>(), optional:[] }};
        function attach() {{ DB.whenReady(data => {{ print(data.items.length); }}); }} attach();"#
        );
        let sim = vm("");
        load(&sim, &src);
        sim.eval("assert(#DB.items==0 and #DB.nested.rows==1 and #DB.nested.rows[1]==0 and DB.nested.partial.x==2 and DB.nested.partial.y==nil and #DB.optional==0)").unwrap();
    }
}

#[test]
fn sd_primitive_keys_migrate_without_coercion() {
    let src = r#"
        type Key = string | number | boolean;
        type Value = string | number | boolean | {};
        type Values = Map<Key,Value>;
        saved global DB version 2 using dbVersion legacy OldDB: { values:Values } = { values:new Map<Key,Value>() };
        migrate DB 1 -> 2 (old:{values:Values; incoming:SavedRepair<Values>}) {
            const incoming=old.incoming;
            if (typeof incoming == "table") {
                for (const [key,value] of incoming) {
                    if (old.values.get(key) == null) { old.values.set(key,value); }
                }
            }
        }
        function attach() { DB.whenReady(data => { print("ready"); }); } attach();
    "#;
    let sim = vm(
        "OldDB={dbVersion=1,values={[17]='existing'},incoming={[17]='replace',['17']='string',[true]={nested={[false]='kept'}},[false]=false}}; root=OldDB; opaque=OldDB.incoming[true]",
    );
    load(&sim, src);
    sim.eval("assert(DB==root and OldDB==nil and DB.dbVersion==2); assert(DB.values[17]=='existing' and DB.values['17']=='string' and DB.values[true]==opaque and DB.values[false]==false and opaque.nested[false]=='kept')").unwrap();
    assert_eq!(sim.prints(), ["ready"]);
}

#[test]
fn sd_opaque_tables_do_not_expose_fields() {
    for access in ["value.secret", "value[\"secret\"]"] {
        let src = format!(
            "saved global DB using dbVersion:{{value:{{}}}}={{value:{{}}}}; function read() {{ DB.whenReady(data => {{ const value=data.value; print({access}); }}); }}"
        );
        assert!(!common::codes(&src).is_empty());
    }
}
#[test]
fn sd_opaque_values_require_serializable_tables() {
    let src = "saved global DB using dbVersion:{value:{}}={value:{}}; function attach(){DB.whenReady(data=>{print(\"ready\");});} attach(); on PLAYER_LOGIN(){print(DB.status);}";
    // A scalar where the default is a table is replaced by the default.
    let sim = vm("DB={dbVersion=1,value=7}");
    load(&sim, src);
    sim.eval("assert(type(DB.value)=='table' and next(DB.value)==nil)")
        .unwrap();
    assert_eq!(sim.prints().last().unwrap(), "ready");
    for input in [
        "DB={dbVersion=1,value={bad=function() end}}",
        "DB={dbVersion=1,value={}}; DB.value.self=DB.value",
        "DB={dbVersion=1,value={[{}]='bad'}}",
    ] {
        let sim = vm(input);
        load(&sim, src);
        assert_eq!(sim.prints().last().unwrap(), "corrupt");
        assert!(!sim.prints().iter().any(|s| s == "ready"));
    }
}
#[test]
fn sd_key_alternatives_are_closed_and_exact() {
    for key in ["number?", "{}", "unknown", "string | {}", "string | null"] {
        let src = format!(
            "saved global DB using dbVersion:{{values:Map<{key},number>}}={{values:new Map<{key},number>()}};"
        );
        assert!(common::codes(&src).iter().any(|c| c == "SELU0901"), "{key}");
    }
    let src = r#"type Key = 17 | "17" | true;
        saved global DB using dbVersion: {values:Map<Key,number>}={values:new Map<Key,number>()};
        on PLAYER_LOGIN(){print(DB.status);}"#;
    for (input, status) in [
        (
            "DB={dbVersion=1,values={[17]=1,['17']=2,[true]=3}}",
            "ready",
        ),
        ("DB={dbVersion=1,values={[false]=4}}", "corrupt"),
        ("DB={dbVersion=1,values={[18]=4}}", "corrupt"),
    ] {
        let sim = vm(input);
        load(&sim, src);
        assert_eq!(sim.prints().last().unwrap(), status);
    }
}
