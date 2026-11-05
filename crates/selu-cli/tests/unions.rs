mod common;
use common::*;
#[test]
fn scalar_union_guards_and_negative_members() {
    assert_eq!(
        run(r#"type V = string | number;
        function f(x: V): string { if (typeof x == "string") { return x; } return "number"; }
        print(f("ok"), f(3));"#),
        vec!["ok\tnumber"]
    );
    assert!(!codes("type V = string | number; const x: V = true;").is_empty());
    assert!(!codes(r#"function f(x: string | number): number { if(typeof x == "string") { return x; } return x; }"#).is_empty());
}
#[test]
fn discriminants_and_literal_defaults() {
    assert_eq!(
        run(
            r#"type V = { kind: "a"; value: string } | { kind: "b"; value: number };
        function f(x: V): string { if (x.kind == "a") { return x.value; } return "b"; }
        const x: V = {kind: "a", value: "ok"}; print(f(x));"#
        ),
        vec!["ok"]
    );
    assert!(!codes(r#"const x: "a" | "b" = "c";"#).is_empty());
}
#[test]
fn independent_saved_schema_preserves_partial_defaults_and_exact_union() {
    let src = r#"type Position = {p?: string; rp?: string; x?: number; y?: number};
        type Schema = {fallbackPos: Position; value: string | number};
        saved global DB using dbVersion: Schema = { fallbackPos: {}, value: 0 };
        on PLAYER_LOGIN() { print(DB.status); }"#;
    for (value, status) in [("'text'", "ready"), ("17", "ready"), ("false", "corrupt")] {
        let sim = common::SavedVm::new();
        sim.eval("__selu_saved_names.DB=true").unwrap();
        sim.eval(&format!(
            "DB={{dbVersion=1,fallbackPos={{p='BOTTOM',x=0}},value={value}}}; root=DB"
        ))
        .unwrap();
        sim.load_addon("Addon", &[("main.lua".into(), common::compile(src))])
            .unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert!(sim.prints().iter().any(|s| s == status));
        sim.eval("assert(DB==root and DB.fallbackPos.p=='BOTTOM' and DB.fallbackPos.x==0 and DB.fallbackPos.rp==nil and DB.fallbackPos.y==nil)").unwrap();
    }
}

#[test]
fn wave_zero_probes() {
    for source in [
        include_str!("fixtures/sc/literal-empty.selu"),
        include_str!("fixtures/sc/typed-empty.selu"),
        include_str!("fixtures/sc/scalar-union.selu"),
    ] {
        assert!(codes(source).is_empty(), "{:?}", codes(source));
        common::compile(source);
    }
}

#[test]
fn unions_in_containers_returns_and_null_checks() {
    assert_eq!(
        run(r#"
        type Box<T> = {value: T};
        const xs: (string | number)[] = [1, "x"];
        const box: Box<string | number> = {value: "ok"};
        function literal(): "yes" | "no" { return "yes"; }
        function f(x: string | null): string { if(x != null) { return x; } return "nil"; }
        print(xs[1], f(null), literal());
    "#),
        vec!["x\tnil\tyes"]
    );
    assert!(!codes(r#"type V = {kind: "a"; value: string} | {kind:"b"; value:number}; function f(x:V):number { if(x.kind=="a"){return x.value;}return x.value;}"#).is_empty());
}

#[test]
fn literal_members_keep_primitive_operations() {
    assert_eq!(
        run("function f(x: -1 | 2): number {return x+1;} print(f(-1));"),
        vec!["0"]
    );
    assert_eq!(
        run("function f(x: 1 | 2): number {return x+1;} print(f(1));"),
        vec!["2"]
    );
}

#[test]
fn string_literal_union_concatenation_is_lua_concatenation() {
    assert_eq!(
        run(r#"function f(x: "a" | "b", y: "c"): string {return x+y;} print(f("a","c"));"#),
        vec!["ac"]
    );
}

#[test]
fn optional_record_defaults_fill_only_literal_keys() {
    let src = r#"saved global DB using v: {pos?: {p?:string; x?:number; y?:number}} = {pos:{x:0}}; on PLAYER_LOGIN(){print(DB.status);}"#;
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.DB=true; DB={v=1,pos={p='BOTTOM'}}; root=DB")
        .unwrap();
    sim.load_addon("Addon", &[("main.lua".into(), common::compile(src))])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert!(sim.prints().iter().any(|s| s == "ready"));
    sim.eval("assert(DB==root and DB.pos.p=='BOTTOM' and DB.pos.x==0 and DB.pos.y==nil)")
        .unwrap();
}
