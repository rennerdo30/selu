mod common;
use common::*;
#[test]
fn and_null() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-and-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn and_typeof() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-and-typeof.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn api_callback() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-api-callback.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn callback_and_null() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-callback-and-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn callback() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-callback.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn captured_guard_no_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-captured-guard-no-call.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn catch_join() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-catch-join.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn class_discriminant() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-class-discriminant.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn class_field_alias() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-class-field-alias.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn class_field() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-class-field.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn class_object_alias() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-class-object-alias.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn constructor_capture() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-constructor-capture.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn field_initializer() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-field-initializer.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn for_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-for-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn guard_exit() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-guard-exit.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn local_closure() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-local-closure.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn loop_assign() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-loop-assign.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn loop_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-loop-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn method_and_null() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-method-and-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn method_capture() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-method-capture.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn narrow_object_alias() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-narrow-object-alias.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn nested_closure() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-nested-closure.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn null_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-null-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn or_null() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-or-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn ordinary_guard() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-ordinary-guard.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn ordinary_typeof() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-ordinary-typeof.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn param_closure() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-param-closure.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn shadow_inner_binding() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-shadow-inner-binding.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn shadow_parameter() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-shadow-parameter.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn try_and_null() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-try-and-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn try_before_throw() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-try-before-throw.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn try_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-try-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn try_direct() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-try-direct.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn try_guard_exit() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-try-guard-exit.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn unrelated_captured_call() {
    let source = include_str!("fixtures/sc-recheck/sc-recheck-unrelated-captured-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn sc_recheck_import_mutable() {
    assert_project_rejected(
        include_str!("fixtures/sc-recheck/sc-recheck-import-mutable/main.selu"),
        include_str!("fixtures/sc-recheck/sc-recheck-import-mutable/state.selu"),
    );
}
#[test]
fn sc_recheck_import_union() {
    assert_project_rejected(
        include_str!("fixtures/sc-recheck/sc-recheck-import-union/main.selu"),
        include_str!("fixtures/sc-recheck/sc-recheck-import-union/state.selu"),
    );
}

fn assert_project_rejected(main: &str, state: &str) {
    assert_project(main, state, true);
}
fn assert_project(main: &str, state: &str, rejected: bool) {
    use common::BoundedCommand;
    use std::{path::Path, process::Command};
    let root = std::env::temp_dir().join(format!(
        "selu-flow-recheck-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname=\"FlowRecheck\"\ntargets=[\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.selu"), main).unwrap();
    std::fs::write(root.join("src/state.selu"), state).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(["build", root.to_str().unwrap(), "--manifest-dir"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .bounded_output()
        .unwrap();
    assert_eq!(
        out.status.success(),
        !rejected,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    if rejected {
        assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0337"));
    }
}
#[test]
fn stable_const_guard_survives_calls() {
    assert_eq!(
        run(
            "const x:number?=1;function f():number{if(x!=null){print(x);return x;}return 0;}print(f());"
        ),
        vec!["1", "1"]
    );
}
#[test]
fn stable_local_and_shadowed_binding_run() {
    assert_eq!(
        run(include_str!(
            "fixtures/sc-recheck/sc-recheck-shadow-inner-binding.selu"
        )),
        vec!["2", "2"]
    );
    assert_eq!(
        run(include_str!(
            "fixtures/sc-recheck/sc-recheck-ordinary-typeof.selu"
        )),
        vec!["2", "OK"]
    );
}

#[test]
fn immutable_import_guard_survives_calls() {
    assert_project(
        "import {x} from './state';function f():number{if(x!=null){print(x);return x;}return 0;}print(f());",
        "export const x:number?=1;",
        false,
    );
}
#[test]
fn call_then_guard_reestablishes_external_fact() {
    assert!(codes("let x:number?=1;function clear():boolean{x=null;return true;}function f():number{if(clear()&&x!=null){return x;}return 0;}").is_empty());
}
#[test]
fn branch_outputs_intersect_after_direct_writes() {
    assert_eq!(
        run("function f(x:number?):number{if(x==null){x=0;}return x;}print(f(null),f(2));"),
        vec!["0\t2"]
    );
}
#[test]
fn loop_const_guard_survives_calls() {
    assert!(codes("const x:number?=1;function f():number{if(x!=null){for(let i=0;i<2;i++){print(x);const y:number=x;}return x;}return 0;}").is_empty());
}
#[test]
fn counted_loop_update_does_not_reuse_fallthrough_facts() {
    assert!(!codes("function use(x:number){}function f(x:number?,flag:boolean){for(;x!=null;use(x)){if(flag){x=null;continue;}}}").is_empty());
}
#[test]
fn nested_writable_view_is_invariant() {
    assert!(!codes("const a={value:'s'};const b:{nested:{value:string|number}}={nested:a};b.nested.value=4;").is_empty());
}
#[test]
fn class_discriminant_positive_arm_is_retained() {
    assert_eq!(
        run(
            r#"class A{kind:"a"="a";value:string="ok";}class B{kind:"b"="b";value:number=1;}function f(x:A|B):string{if(x.kind=="a"){return x.value;}return "b";}print(f(new A()));"#
        ),
        vec!["ok"]
    );
}

#[test]
fn mutable_field_writes_include_compound_assignments() {
    assert!(!codes("function f(xs:number[]){xs.length++;}").is_empty());
    assert!(!codes("type V={value:string}|{value:number};function f(x:V){x.value+=1;}").is_empty());
}
#[test]
fn loop_field_writes_do_not_kill_unassigned_scalar_facts_without_calls() {
    assert!(codes("let x:number?=1;function clear(){x=null;}function f():number{if(x!=null){const box={n:0};while(box.n<2){box.n++;const y:number=x;}return x;}return 0;}").is_empty());
}
#[test]
fn optional_view_cannot_recover_erased_writable_fields() {
    assert!(!codes(r#"type V={kind:"a";value:string}|{kind:"b";value:number};function f(x:V):string{const erased:{}=x;const view:{kind?:string;value?:string|number}=erased;if(x.kind=="a"){view.value=4;return x.value;}return "other";}"#).is_empty());
}
#[test]
fn mutable_class_field_overrides_are_invariant() {
    assert!(!codes("class Base{value:string|number='s';change(){this.value=4;}}class Child extends Base{value:string='s';}const x=new Child();x.change();function read():string{return x.value;}print(read());").is_empty());
}
#[test]
fn delayed_field_initializer_does_not_execute_at_class_definition() {
    assert!(codes("let x:number?=1;function clear():number{x=null;return 0;}function f():number{if(x!=null){class C{value=clear();}return x;}return 0;}").is_empty());
}
#[test]
fn creating_a_closure_does_not_execute_its_writes() {
    assert!(codes("function f(x:number?):number{if(x!=null){const clear=()=>{x=null;};return x;}return 0;}").is_empty());
}

#[test]
fn branch_assignments_merge_into_one_fact() {
    // Each path assigned a different type: after the merge either may hold.
    let src = "const c = GetTime() > 1; let k: number | string | null = null; if (c) { k = 3; } else { k = \"x\"; } const j: number | string = k; let p: number | string | null = null; if (p == null) { p = 3; } else if (typeof p == \"string\") { p = \"x\"; } const q: number | string = p;";
    assert!(codes(src).is_empty(), "{:?}", codes(src));
    assert_eq!(
        codes(
            "const c = GetTime() > 1; let k: number | string | null = null; if (c) { k = 3; } else { k = \"x\"; } const bad: number = k;"
        ),
        ["SELU0336"]
    );
}
