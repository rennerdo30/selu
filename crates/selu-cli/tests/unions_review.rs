mod common;
use common::*;
#[test]
fn alias_direct() {
    let source = include_str!("fixtures/sc-review/sc-probe-alias-direct.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn alias_mutation() {
    let source = include_str!("fixtures/sc-review/sc-probe-alias-mutation.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn all_member_assignment() {
    let source = include_str!("fixtures/sc-review/sc-probe-all-member-assignment.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn array_context() {
    let source = include_str!("fixtures/sc-review/sc-probe-array-context.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn assign_literal() {
    let source = include_str!("fixtures/sc-review/sc-probe-assign-literal.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn closure_const() {
    let source = include_str!("fixtures/sc-review/sc-probe-closure-const.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn closure() {
    let source = include_str!("fixtures/sc-review/sc-probe-closure.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn early_assign() {
    let source = include_str!("fixtures/sc-review/sc-probe-early-assign.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn eval_order() {
    let source = include_str!("fixtures/sc-review/sc-probe-eval-order.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn false_nil() {
    let source = include_str!("fixtures/sc-review/sc-probe-false-nil.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn format() {
    let source = include_str!("fixtures/sc-review/sc-probe-format.selu");
    let formatted = selu_fmt::format(source, &selu_fmt::Options::default()).unwrap();
    assert_eq!(
        selu_fmt::format(&formatted, &selu_fmt::Options::default()).unwrap(),
        formatted
    );
    assert!(formatted.contains("/* keep */"));
}
#[test]
fn function_common_call() {
    let source = include_str!("fixtures/sc-review/sc-probe-function-common-call.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn function_union() {
    let source = include_str!("fixtures/sc-review/sc-probe-function-union.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn generic() {
    let source = include_str!("fixtures/sc-review/sc-probe-generic.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn literals_order() {
    let source = include_str!("fixtures/sc-review/sc-probe-literals-order.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn loop_closure() {
    let source = include_str!("fixtures/sc-review/sc-probe-loop-closure.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn loop_object() {
    let source = include_str!("fixtures/sc-review/sc-probe-loop-object.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn nested_unions() {
    let source = include_str!("fixtures/sc-review/sc-probe-nested-unions.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn nested_unsound() {
    let source = include_str!("fixtures/sc-review/sc-probe-nested-unsound.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn number_literals_order() {
    let source = include_str!("fixtures/sc-review/sc-probe-number-literals-order.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn object_literal_assign() {
    let source = include_str!("fixtures/sc-review/sc-probe-object-literal-assign.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn object_union() {
    let source = include_str!("fixtures/sc-review/sc-probe-object-union.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn optional_arm() {
    let source = include_str!("fixtures/sc-review/sc-probe-optional-arm.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn optional_assign() {
    let source = include_str!("fixtures/sc-review/sc-probe-optional-assign.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn optional_guard() {
    let source = include_str!("fixtures/sc-review/sc-probe-optional-guard.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn optional_nested_null() {
    let source = include_str!("fixtures/sc-review/sc-probe-optional-nested-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn optional_null_arm() {
    let source = include_str!("fixtures/sc-review/sc-probe-optional-null-arm.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn parenthesized_function() {
    let source = include_str!("fixtures/sc-review/sc-probe-parenthesized-function.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn primitive_literal_narrow() {
    let source = include_str!("fixtures/sc-review/sc-probe-primitive-literal-narrow.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn return_literal_branches() {
    let source = include_str!("fixtures/sc-review/sc-probe-return-literal-branches.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn saved_literal() {
    let source = include_str!("fixtures/sc-review/sc-probe-saved-literal.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn saved_matrix() {
    let source = include_str!("fixtures/sc-review/sc-probe-saved-matrix.selu");
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn typeof_null() {
    let source = include_str!("fixtures/sc-review/sc-probe-typeof-null.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn typeof_optional() {
    let source = include_str!("fixtures/sc-review/sc-probe-typeof-optional.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn union_nil_guard() {
    let source = include_str!("fixtures/sc-review/sc-probe-union-nil-guard.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}
#[test]
fn union_nonmatching_call() {
    let source = include_str!("fixtures/sc-review/sc-probe-union-nonmatching-call.selu");
    assert!(!codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn optional_captured_mutation_invalidates_null_guard() {
    assert!(!codes("let x:number?=1; function clear(){x=null;} function f():number{if(x!=null){clear();return x;}return 0;}").is_empty());
}
#[test]
fn nested_optional_table_guards() {
    assert!(codes(r#"type A=Map<string,number>|Set<string>; function f(x:(A|number)?):number {if(typeof x=="table"){return 1;}if(x==null){return 0;}return x;}"#).is_empty());
}
#[test]
fn narrowed_arm_field_write_is_allowed() {
    assert!(codes(r#"type V={kind:"a";value:string}|{kind:"b";value:number}; function f(x:V){if(x.kind=="b"){x.value=4;}}"#).is_empty());
}
#[test]
fn transactional_object_context_second_arm_is_not_an_escape() {
    assert!(
        !codes("type V={items:string[]}|{items:number[]};const x:V={items:[true]};").is_empty()
    );
}
#[test]
fn history_distinguishes_array_of_union_from_union_with_array() {
    use common::BoundedCommand;
    use std::{path::Path, process::Command};
    let root = std::env::temp_dir().join(format!("selu-sc-review-history-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname=\"History\"\ntargets=[\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    let build = || {
        Command::new(env!("CARGO_BIN_EXE_selu"))
            .args(["build", root.to_str().unwrap(), "--manifest-dir"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
            .bounded_output()
            .unwrap()
    };
    std::fs::write(
        root.join("src/main.selu"),
        "saved prefs:{x:(string|number)[]}={x:[1]};",
    )
    .unwrap();
    assert!(build().status.success());
    let original = std::fs::read(root.join("selu.saved.toml")).unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        "saved prefs:{x:string|number[]}={x:[1]};",
    )
    .unwrap();
    let output = build();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SELU0468"));
    assert_eq!(
        std::fs::read(root.join("selu.saved.toml")).unwrap(),
        original
    );
}

#[test]
#[ignore = "known limit: missing-return exhaustiveness is deferred"]
fn deferred_missing_return_exhaustiveness() {
    assert!(
        !codes(include_str!(
            "fixtures/sc-review/sc-probe-exhaustiveness.selu"
        ))
        .is_empty()
    );
}
#[test]
fn captured_forward_closure_invalidates_null_guard() {
    assert!(!codes("let x:number?=1; function f():number {if(x!=null){later();return x;}return 0;} function later(){x=null;}").is_empty());
}
#[test]
fn uncaptured_mutable_guard_survives_calls() {
    assert!(
        codes("function f(x:number?):number {if(x!=null){print(x);return x;}return 0;}").is_empty()
    );
}
#[test]
fn closure_parameter_shadowing_does_not_invalidate_outer_binding() {
    assert!(codes("let x:number?=1; function change(x:number?){x=null;} function f():number {if(x!=null){change(2);return x;}return 0;}").is_empty());
}
#[test]
fn union_function_call_unions_return_values_at_runtime() {
    assert_eq!(
        run(
            r#"type F=((x:string)=>number)|((x:string)=>string);function f(g:F):string|number{return g("ok");}print(f((x:string):number=>1),f((x:string):string=>x));"#
        ),
        vec!["1\tok"]
    );
}

#[test]
fn all_table_categories_survive_nested_partition() {
    assert!(codes(r#"class C {x=1;} type T={x:number}|number[]|[number,number]|Map<string,number>|Set<string>|WeakMap<{x:number},number>|C;function f(x:(T|string)?):string{if(typeof x=="table"){return "table";}if(x==null){return "nil";}return x;}"#).is_empty());
}
#[test]
fn failed_callable_context_cannot_fall_back_to_unchecked_parameters() {
    assert!(!codes(r#"type F=((x:string)=>number)|((x:number)=>string);const f:F=x=>{return x.noSuchField;};"#).is_empty());
}

#[test]
fn positive_probe_values_run_with_exact_results() {
    for (source, expected) in [
        (
            include_str!("fixtures/sc-review/sc-probe-eval-order.selu"),
            "boolean\t1\tnil",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-false-nil.selu"),
            "false\tnil\ts",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-assign-literal.selu"),
            "b",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-object-literal-assign.selu"),
            "2",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-nested-unions.selu"),
            "s",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-optional-assign.selu"),
            "nil",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-optional-null-arm.selu"),
            "4",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-primitive-literal-narrow.selu"),
            "a",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-literals-order.selu"),
            "b",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-number-literals-order.selu"),
            "2",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-parenthesized-function.selu"),
            "1",
        ),
        (
            include_str!("fixtures/sc-review/sc-probe-return-literal-branches.selu"),
            "b",
        ),
    ] {
        assert_eq!(run(source), vec![expected]);
    }
}

#[test]
fn optional_and_equivalent_union_assign_both_directions() {
    assert!(codes("function f(x:number|null):number?{return x;} function g(x:number?):number|null{return x;}").is_empty());
}
#[test]
fn optional_unknown_keeps_primitive_type_guards() {
    assert!(
        codes(r#"function f(x:unknown?):string{if(typeof x=="string"){return x;}return "other";}"#)
            .is_empty()
    );
}

#[test]
fn checked_union_reference_is_assignable_to_complete_union() {
    assert!(codes("type V=string|number; function f(x:V):V{const y:V=x;return y;}").is_empty());
}
#[test]
fn constructor_calls_invalidate_captured_facts() {
    assert!(!codes(r#"let x:number?=1;class C{constructor(){x=null;}}function f():number{if(x!=null){const c=new C();return x;}return 0;}"#).is_empty());
}

#[test]
fn literal_guard_preserves_string_operations() {
    assert_eq!(
        run(
            r#"function f(x:string|number):string{if(x=="a"){print(x.length);return x.upper();}return "other";}print(f("a"));"#
        ),
        vec!["1", "A"]
    );
}
#[test]
fn union_receivers_cannot_write_readonly_members() {
    assert!(!codes("function f(x:string[]|number[]){x.length=1;}").is_empty());
}

#[test]
fn union_array_lengths_use_sequence_lowering() {
    assert_eq!(
        run("function f(x:string[]|number[]):number{return x.length;}print(f([1]));"),
        vec!["1"]
    );
}
