mod common;
use common::*;
fn accepted(s: &str) -> bool {
    selu_syntax::parse("probe.selu", s).is_ok_and(|p| {
        !selu_check::check(&p, &manifest())
            .iter()
            .any(|d| d.severity == selu_diagnostics::Severity::Error)
    })
}
#[test]
fn attack_array_guard_valid() {
    let source = include_str!("fixtures/sc-recheck2/attack-array-guard-valid.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_array_push_context() {
    let source = include_str!("fixtures/sc-recheck2/attack-array-push-context.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_array_typeof_guard() {
    let source = include_str!("fixtures/sc-recheck2/attack-array-typeof-guard.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_array_union_write() {
    let source = include_str!("fixtures/sc-recheck2/attack-array-union-write.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_array_widen() {
    let source = include_str!("fixtures/sc-recheck2/attack-array-widen.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_async() {
    let source = include_str!("fixtures/sc-recheck2/attack-async.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_closure_guard_stable() {
    let source = include_str!("fixtures/sc-recheck2/attack-closure-guard-stable.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_closure_read_const() {
    let source = include_str!("fixtures/sc-recheck2/attack-closure-read-const.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_closure_read_stable() {
    let source = include_str!("fixtures/sc-recheck2/attack-closure-read-stable.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_const_object_field() {
    let source = include_str!("fixtures/sc-recheck2/attack-const-object-field.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_const_object_safe() {
    let source = include_str!("fixtures/sc-recheck2/attack-const-object-safe.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_delayed_callback() {
    let source = include_str!("fixtures/sc-recheck2/attack-delayed-callback.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_delayed_read_const() {
    let source = include_str!("fixtures/sc-recheck2/attack-delayed-read-const.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_delayed_read_stable() {
    let source = include_str!("fixtures/sc-recheck2/attack-delayed-read-stable.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_discriminant_write() {
    let source = include_str!("fixtures/sc-recheck2/attack-discriminant-write.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_event_delayed_read() {
    let source = include_str!("fixtures/sc-recheck2/attack-event-delayed-read.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_event_delayed_write() {
    let source = include_str!("fixtures/sc-recheck2/attack-event-delayed-write.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_event_guard_inside() {
    let source = include_str!("fixtures/sc-recheck2/attack-event-guard-inside.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_field_same_name_reassign() {
    let source = include_str!("fixtures/sc-recheck2/attack-field-same-name-reassign.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_generic_alias_callback() {
    let source = include_str!("fixtures/sc-recheck2/attack-generic-alias-callback.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_generic_callback() {
    let source = include_str!("fixtures/sc-recheck2/attack-generic-callback.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_getter() {
    let source = include_str!("fixtures/sc-recheck2/attack-getter.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_map_context_workaround() {
    let source = include_str!("fixtures/sc-recheck2/attack-map-context-workaround.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_map_context() {
    let source = include_str!("fixtures/sc-recheck2/attack-map-context.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_map_union_write() {
    let source = include_str!("fixtures/sc-recheck2/attack-map-union-write.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_map_widen() {
    let source = include_str!("fixtures/sc-recheck2/attack-map-widen.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_module_cache_log() {
    let source = include_str!("fixtures/sc-recheck2/attack-module-cache-log.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_module_cache() {
    let source = include_str!("fixtures/sc-recheck2/attack-module-cache.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_mutual_recursive() {
    let source = include_str!("fixtures/sc-recheck2/attack-mutual-recursive.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_recursive() {
    let source = include_str!("fixtures/sc-recheck2/attack-recursive.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_reexport_syntax() {
    let source = include_str!("fixtures/sc-recheck2/attack-reexport-syntax.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_stable_const() {
    let source = include_str!("fixtures/sc-recheck2/attack-stable-const.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_stable_local() {
    let source = include_str!("fixtures/sc-recheck2/attack-stable-local.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_this_discriminant() {
    let source = include_str!("fixtures/sc-recheck2/attack-this-discriminant.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_this_guard() {
    let source = include_str!("fixtures/sc-recheck2/attack-this-guard.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn attack_this_local_alias() {
    let source = include_str!("fixtures/sc-recheck2/attack-this-local-alias.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn attack_tuple_widen() {
    let source = include_str!("fixtures/sc-recheck2/attack-tuple-widen.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn extra_generic_alias_callback_valid() {
    let source = include_str!("fixtures/sc-recheck2/extra-generic-alias-callback-valid.selu");
    assert!(!accepted(source), "{}", source);
}
#[test]
fn extra_generic_alias_callback_positive() {
    let source = include_str!("fixtures/sc-recheck2/extra-generic-alias-callback-positive.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn extra_this_snapshot() {
    let source = include_str!("fixtures/sc-recheck2/extra-this-snapshot.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn extra_stable_local_in_closure() {
    let source = include_str!("fixtures/sc-recheck2/extra-stable-local-in-closure.selu");
    assert!(accepted(source), "{}", source);
}
#[test]
fn build_export_own() {
    project(
        &[(
            "main.selu",
            include_str!("fixtures/sc-recheck2/build-export-own/main.selu"),
        )],
        false,
    );
}
#[test]
fn build_import_const() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/build-import-const/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/build-import-const/state.selu"),
            ),
        ],
        true,
    );
}
#[test]
fn build_import_mutable() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/build-import-mutable/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/build-import-mutable/state.selu"),
            ),
        ],
        false,
    );
}
#[test]
fn build_import_union() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/build-import-union/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/build-import-union/state.selu"),
            ),
        ],
        false,
    );
}

fn project(files: &[(&str, &str)], success: bool) {
    use common::BoundedCommand;
    use std::{path::Path, process::Command};
    let root = std::env::temp_dir().join(format!(
        "selu-paths-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname=\"Recheck2\"\ntargets=[\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    for (name, source) in files {
        std::fs::write(root.join("src").join(name), source).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(["build", root.to_str().unwrap(), "--manifest-dir"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .bounded_output()
        .unwrap();
    assert_eq!(
        out.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn fresh_collection_records_run() {
    assert_eq!(
        run(
            r#"const cache=new Map<string,{value:string|number}>();cache.set("x",{value:4});const items:{value:string|number}[]=[];items.push({value:4});for(const [key,value] of cache){print(key,value.value);}print(items[0].value);"#
        ),
        vec!["x\t4", "4"]
    );
}
#[test]
fn collection_aliases_stay_invariant() {
    for source in [
        r#"const entry={value:4};const cache=new Map<string,{value:string|number}>();cache.set("x",entry);"#,
        r#"const entry={value:4};const items:{value:string|number}[]=[];items.push(entry);"#,
    ] {
        assert!(!codes(source).is_empty(), "{source}");
    }
}
#[test]
fn access_paths_run_without_effects() {
    assert_eq!(
        run(
            r#"type V={kind:"a";value:string}|{kind:"b";value:number};class C{x:number?=2;v:V={kind:"a",value:"ok"};read():number{if(this.x!=null){return this.x;}return 0;}text():string{if(this.v.kind=="a"){return this.v.value;}return "other";}}function field(x:{n?:number}):number{if(x.n!=null){return x.n;}return 0;}function item(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){return xs[i];}return "other";}const c=new C();print(c.read(),c.text(),field({n:3}),item(["yes"],0));"#
        ),
        vec!["2\tok\t3\tyes"]
    );
}
#[test]
fn unwritten_closure_guards_run() {
    assert_eq!(
        run(
            "function f(x:number?):number{if(x!=null){const read=():number=>x;return read();}return 0;}function g():number{let x:number?=3;if(x!=null){const read=():number=>x;return read();}return 0;}print(f(2),g());"
        ),
        vec!["2\t3"]
    );
}
#[test]
fn later_assignments_exclude_closure_inheritance() {
    for source in [
        "function f(x:number?):number{if(x!=null){const read=():number=>x;x=null;return read();}return 0;}",
        "function f(x:number?):number{if(x!=null){const read=():number=>x;const clear=()=>{x=null;};return read();}return 0;}",
        "function f():number{let x:number?=1;if(x!=null){const read=():number=>x;x=null;return read();}return 0;}",
    ] {
        assert!(!codes(source).is_empty(), "{source}");
    }
}
#[test]
fn calls_kill_access_path_guards() {
    for source in [
        "function noop(){}class C{x:number?=1;read():number{if(this.x!=null){noop();return this.x;}return 0;}}",
        r#"function noop(){}type V={kind:"a";value:string}|{kind:"b";value:number};class C{v:V={kind:"a",value:"ok"};read():string{if(this.v.kind=="a"){noop();return this.v.value;}return "other";}}"#,
        r#"function noop(){}function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){noop();return xs[i];}return "other";}"#,
        "function noop(){}function f(x:{n?:number}):number{if(x.n!=null){noop();return x.n;}return 0;}",
    ] {
        assert!(!codes(source).is_empty(), "{source}");
    }
}
#[test]
fn assignments_kill_access_path_guards() {
    for source in [
        "class C{x:number?=1;read():number{if(this.x!=null){this.x=null;return this.x;}return 0;}}",
        r#"type V={kind:"a";value:string}|{kind:"b";value:number};class C{v:V={kind:"a",value:"ok"};read():string{if(this.v.kind=="a"){this.v={kind:"b",value:4};return this.v.value;}return "other";}}"#,
        "function f(x:{n?:number}):number{if(x.n!=null){x={};return x.n;}return 0;}",
        r#"function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){xs[i]=4;return xs[i];}return "other";}"#,
        r#"function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){xs=[4];return xs[i];}return "other";}"#,
        r#"function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){i=1;return xs[i];}return "other";}"#,
        r#"function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){i++;return xs[i];}return "other";}"#,
        "function f(x:{n?:number},alias:{n?:number}):number{if(x.n!=null){alias.n=null;return x.n;}return 0;}",
    ] {
        assert!(!codes(source).is_empty(), "{source}");
    }
}
#[test]
fn loop_entries_kill_access_path_dependencies() {
    for source in [
        r#"function f(xs:(string|number)[],i:number):string{if(typeof xs[i]=="string"){while(i<2){const value:string=xs[i];i++;}}return "other";}"#,
        "function noop(){}class C{x:number?=1;read():number{if(this.x!=null){while(true){const value:number=this.x;noop();}}return 0;}}",
        "function f(x:{n?:number}):number{if(x.n!=null){while(true){const value:number=x.n;x.n=null;}}return 0;}",
    ] {
        assert!(!codes(source).is_empty(), "{source}");
    }
}

#[test]
fn project_import_no_call() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/project-import-no-call/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/project-import-no-call/state.selu"),
            ),
        ],
        true,
    );
}

#[test]
fn project_reexport_global() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-global/main.selu"),
            ),
            (
                "middle.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-global/middle.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-global/state.selu"),
            ),
        ],
        true,
    );
}

#[test]
fn project_reexport_object() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-object/main.selu"),
            ),
            (
                "middle.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-object/middle.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-object/state.selu"),
            ),
        ],
        false,
    );
}

#[test]
fn project_reexport_snapshot() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-snapshot/main.selu"),
            ),
            (
                "middle.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-snapshot/middle.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/project-reexport-snapshot/state.selu"),
            ),
        ],
        true,
    );
}

#[test]
fn sc_recheck_import_mutable() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/sc-recheck-import-mutable/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/sc-recheck-import-mutable/state.selu"),
            ),
        ],
        false,
    );
}

#[test]
fn sc_recheck_import_union() {
    project(
        &[
            (
                "main.selu",
                include_str!("fixtures/sc-recheck2/sc-recheck-import-union/main.selu"),
            ),
            (
                "state.selu",
                include_str!("fixtures/sc-recheck2/sc-recheck-import-union/state.selu"),
            ),
        ],
        false,
    );
}

#[test]
fn null_guard_does_not_turn_open_inference_into_an_empty_type() {
    assert_eq!(
        run("function f(value){if(value==null){return;}print(`${value}`);}f(3);"),
        vec!["3"]
    );
}

#[test]
fn primitive_equality_guards_preserve_template_operands() {
    assert_eq!(
        run(r#"function f(value:string){if(value=="ok"){print(`${value}`);}}f("ok");"#),
        vec!["ok"]
    );
}
