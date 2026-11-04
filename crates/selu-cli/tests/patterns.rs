mod common;
use common::{codes, compile, run};
#[test]
fn methods_and_results() {
    assert_eq!(
        run(r#"
on PLAYER_LOGIN() {
 const s = "|Hitem:123|h[Sword]|h";
 const [id, label] = s.match(luaPattern`%|Hitem:(%d+)%|h%[(.-)%]%|h`);
 print(id ?? "missing", label ?? "missing");
 print(s.search(luaPattern`item:`) ?? -1);
 print(s.match(luaPattern`nothing`) ?? "none");
 const [a,n] = "a%a".replace(luaPattern`a`, "%"); print(a,n);
 const [b,m] = "a%a".replaceAll(luaPattern`a`, "%"); print(b,m);
 const [c,k] = "a1 b2".replaceAll(luaPattern`(%a)(%d)`, (a,b) => a + a + b); print(c,k);
 const [d,j] = "abc".replaceAll(luaPattern`%a`, (a) => null); print(d,j);
 for (const [a,b] of "x1 y2".matchAll(luaPattern`(%a)(%d)`)) { print(a,b); }
 let count = 0; for (const a of "ab".matchAll(luaPattern``)) { count += 1; } print(count);
 print("abc".match(luaPattern`^abc$`) ?? "bad");
}
"#),
        [
            "123\tSword",
            "2",
            "none",
            "%%a\t1",
            "%%%\t2",
            "aa1 bb2\t2",
            "abc\t3",
            "x\t1",
            "y\t2",
            "3",
            "abc"
        ]
    );
}
#[test]
fn positions_and_contexts() {
    assert_eq!(
        run(r#"
function captures(): [number?, string?, number?] { return "abc".match(luaPattern`()(%a+)()`); }
function show(n: number?, s: string) { print(n ?? -1, s); }
on PLAYER_LOGIN() {
 const [a,b,c] = captures(); print(a ?? -1,b ?? "?",c ?? -1);
 const [d,e,f] = "abc".match(luaPattern`()(%a+)()`); print(d ?? -1,e ?? "?",f ?? -1);
 const [g,h] = "abc".match(luaPattern`()x(%a+)`); print(g ?? -1,h ?? "none");
 show("abc".match(luaPattern`()(%a+)()`), "tail");
 for (const [p,v,q] of "a b".matchAll(luaPattern`()(%a)()`)) {print(p,v,q);}
 const x = ["abc".match(luaPattern`(%a)(%a)` ) ?? "?"]; print(x[0]);
}
"#),
        [
            "0\tabc\t3",
            "0\tabc\t3",
            "-1\tnone",
            "0\ttail",
            "0\ta\t1",
            "2\tb\t3",
            "a"
        ]
    );
}
#[test]
fn exact_lua_plain_iterator() {
    let lua = compile("for (const [a,b] of \"x1\".matchAll(luaPattern`(%a)(%d)`)) { print(a,b); }");
    assert_eq!(
        lua,
        "for a, b in string.gmatch(\"x1\", \"(%a)(%d)\") do\n  print(a, b)\nend\n"
    );
}
#[test]
fn invalid_patterns() {
    for pat in [
        "(", ")", "[a", "%", "%b(", "%1", "(%1)", "()%1", "%g", "a|b", "a{2}", r"\d", "(?:x)",
    ] {
        let src = format!("const x = luaPattern`{pat}`;");
        assert!(codes(&src).contains(&"SELU0560".into()), "{pat}");
    }
    assert!(codes("const x = luaPattern`a${1}`;").contains(&"SELU0561".into()));
    assert!(codes("const x = \"a\".match(\"a\");").contains(&"SELU0562".into()));
    assert!(
        codes("for (const a of \"a\".matchAll(luaPattern`^a`)) {}").contains(&"SELU0563".into())
    );
    assert!(
        codes("const x = \"a\".replaceAll(luaPattern`(%a)`, (a) => 1);")
            .iter()
            .any(|c| c.starts_with("SELU03") || c == "SELU0564")
    );
    let src = "const x = luaPattern`abc\\d`;";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    let d = ds.iter().find(|d| d.code == "SELU0560").unwrap();
    assert_eq!(d.span.as_ref().unwrap().start, src.find('\\').unwrap());
    assert!(d.help.as_ref().unwrap().contains("%d"));
    assert_eq!(
        selu_syntax::parse("t.selu", r"const x = `\d`; ")
            .unwrap_err()
            .code,
        "SELU0108"
    );
}
#[test]
fn simulator_frontier_probe() {
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("assert(string.match(' hi', '%f[%a]hi') == 'hi')")
        .unwrap();
}

#[test]
fn stored_iterators_callbacks_and_single_capture() {
    assert_eq!(
        run(r#"
function replacement(p: number, s: string): string { return `${p}:${s}`; }
on PLAYER_LOGIN() {
 const [p] = "a".match(luaPattern`()a`); print(p ?? -1);
 const it = "a b".matchAll(luaPattern`()(%a)`);
 for (const [p,s] of it) { print(p,s); }
 const [s,n] = "a b".replaceAll(luaPattern`()(%a)`, replacement); print(s,n);
 const [t,k] = "ab".replaceAll(luaPattern`()(%a)`, (p,s) => `${p}:${s}`); print(t,k);
 const literal: string = "%1";
 const [u,j] = "ab".replaceAll(luaPattern`%a`,literal); print(u,j);
 const [v] = "abc".match(luaPattern`(%a+)`); print(v ?? "?");
}
"#),
        [
            "0",
            "0\ta",
            "2\tb",
            "0:a 2:b\t2",
            "0:a1:b\t2",
            "%1%1\t2",
            "abc"
        ]
    );
}
#[test]
fn capture_limits_and_raw_tagged_templates() {
    assert_eq!(
        selu_syntax::parse("t.selu", r"const x = `\d${1}`;")
            .unwrap_err()
            .code,
        "SELU0108"
    );
    let good = format!("const x = luaPattern`{}`;", "()".repeat(32));
    assert!(codes(&good).is_empty());
    let bad = format!("const x = luaPattern`{}`;", "()".repeat(33));
    assert!(codes(&bad).contains(&"SELU0560".into()));
    assert!(selu_syntax::parse("t.selu", r"const x = `outer ${L`\d`}`;").is_err());
    assert!(selu_syntax::parse("t.selu", r"const x = L`outer ${`\d`}`;").is_err());
    let source = r"const x = luaPattern`\n|`;";
    let p = selu_syntax::parse("t.selu", source).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    assert_eq!(
        ds[0].span.as_ref().unwrap().start,
        source.find('\\').unwrap()
    );
}

#[test]
fn result_projection_in_objects_returns_and_conditions() {
    assert_eq!(
        run(r#"
function first(): number? { return "a".match(luaPattern`()(%a)`); }
function replace(): [string, number] {return "aa".replaceAll(luaPattern`a`, "b");}
on PLAYER_LOGIN() {
 const obj = {value: "ab".replaceAll(luaPattern`a`, "c")}; print(obj.value);
 if ("a".match(luaPattern`(%a)(%a?)`)) {print("yes");}
 const [s,n] = replace(); print(s,n); print(first() ?? -1);
 const [a,b] = "aba".match(luaPattern`(%a)(%a)%1`); print(a ?? "?",b ?? "?");
 print("(nested(x))".match(luaPattern`%b()`) ?? "?");
}
"#),
        ["cb", "yes", "bb\t2", "0", "a\tb", "(nested(x))"]
    );
    let src = "const x = \"a\".match(\"a\");";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    assert!(
        ds.iter()
            .any(|d| d.help.as_ref().is_some_and(|h| h.contains("luaPattern")))
    );
}

#[test]
fn iterator_is_not_a_string_union() {
    assert!(!codes("const value: string = \"a\".matchAll(luaPattern`%a`);").is_empty());
}
#[test]
fn review_user_methods_keep_their_identity() {
    assert_eq!(
        run(r#"
const obj = {match: (p: unknown): [number?, string?] => [10, "ok"], matchAll: (p: unknown) => "ab".matchAll(luaPattern`(%a)`)};
function forward(): [number?, string?] {return obj.match(luaPattern`()(%a)`);}
on PLAYER_LOGIN() {
 const [p,s] = obj.match(luaPattern`()(%a)`); print(p ?? -1,s ?? "?");
 const [q,t] = forward(); print(q ?? -1,t ?? "?");
 for (const x of obj.matchAll(luaPattern`(%a)`)) {print(x);}
}
"#),
        ["10\tok", "10\tok", "a", "b"]
    );
}
#[test]
fn review_scalar_captures_do_not_exhaust_lua_locals() {
    for pattern in ["()".repeat(32), format!("(%a){}", "()".repeat(31))] {
        let mut src = String::from("on PLAYER_LOGIN() {\n");
        for i in 0..7 {
            src.push_str(&format!(
                "const x{i} = \"a\".match(luaPattern`{pattern}`); print(x{i} ?? {});\n",
                if pattern.starts_with("()") {
                    "-1"
                } else {
                    "\"?\""
                }
            ));
        }
        src.push('}');
        assert_eq!(
            run(&src),
            vec![if pattern.starts_with("()") { "0" } else { "a" }; 7]
        );
        let lua = compile(&src);
        assert!(!lua.contains(", __selu_t"));
        assert_eq!(
            lua.matches(" - 1").count(),
            if pattern.starts_with("()") { 7 } else { 0 }
        );
    }
}
#[test]
fn review_rest_callback_positions() {
    assert_eq!(
        run(r#"
on PLAYER_LOGIN() {
 print("ab".replaceAll(luaPattern`()`, (...ps: number[]) => `${ps[0]}`));
 print("ab".replaceAll(luaPattern`()()`, (p, ...ps: number[]) => `${p}:${ps[0]}`));
}
"#),
        ["0a1b2", "0:0a1:1b2:2"]
    );
}
#[test]
fn review_untyped_replacements() {
    assert_eq!(
        run(r#"
function substitute(value): string {return "ab".replaceAll(luaPattern`()(%a)`,value);}
function plain(value): string {return "ab".replaceAll(luaPattern`(%a)`,value);}
on PLAYER_LOGIN() {
 print(substitute("%1"));
 print(substitute((p: number, s: string) => `${p}:${s}`));
 print(plain("%1"));
 print(plain((s: string) => s + s));
}
"#),
        ["%1%1", "0:a1:b", "%1%1", "aabb"]
    );
}
#[test]
fn review_empty_bindings() {
    assert_eq!(
        run(r#"on PLAYER_LOGIN() {
 const [] = "a".match(luaPattern`()a`);
 let n=0; for (const [] of "aa".matchAll(luaPattern`(a)`)) {n+=1;} print(n);
 }"#),
        ["2"]
    );
}
#[test]
fn review_existing_tags_keep_escape_diagnostics() {
    for tag in [
        "",
        "L",
        "log.info",
        "log.debug",
        "log.warn",
        "log.error",
        "log.warnOnce",
    ] {
        for tail in ["", "${1}"] {
            let src = format!("const x = {tag}`hello\\d{tail}`;");
            let d = selu_syntax::parse("t.selu", &src).unwrap_err();
            assert_eq!(d.code, "SELU0108");
            assert_eq!(d.span.as_ref().unwrap().start, src.find('\\').unwrap());
        }
    }
}
#[test]
fn review_regex_sets_have_exact_spans() {
    for class in ["d", "w", "s"] {
        let src = format!("const x = luaPattern`[\\{class}]`; ");
        let p = selu_syntax::parse("t.selu", &src).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        let d = ds
            .iter()
            .find(|d| d.code == "SELU0560")
            .expect("regex class must be rejected");
        assert_eq!(d.span.as_ref().unwrap().start, src.find('\\').unwrap());
        assert!(d.help.as_ref().unwrap().contains(&format!("%{class}")));
    }
}
#[test]
fn review_template_delimiters_and_offsets() {
    assert_eq!(
        run(r#"on PLAYER_LOGIN() {print("a`b".match(luaPattern`a\`b`) ?? "?");}"#),
        ["a`b"]
    );
    assert_eq!(
        run(r#"on PLAYER_LOGIN() {print("${value}".match(luaPattern`\${value}`) ?? "?");}"#),
        ["${value}"]
    );
    let src = r"const x = luaPattern`a\`b[\d]`;";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    assert_eq!(ds[0].span.as_ref().unwrap().start, src.rfind('\\').unwrap());
}
#[test]
fn review_frontiers_are_supported() {
    assert_eq!(
        run(r#"on PLAYER_LOGIN() {
 print(" hi there".match(luaPattern`%f[%a]%a+`) ?? "?");
 print("x".search(luaPattern`%f[%a]`) ?? -1);
 for (const word of "hi there".matchAll(luaPattern`%f[%a]%a+`)) {print(word);}
 print("hi".replaceAll(luaPattern`%f[%a]`, ">"));
 }"#),
        ["hi", "0", "hi", "there", ">hi"]
    );
    for pat in ["%f", "%fa", "%f[", "%f[%g]"] {
        assert!(codes(&format!("const p=luaPattern`{pat}`;")).contains(&"SELU0560".into()));
    }
}
#[test]
fn review_proof_document_encoding() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/proof/junksell.md"
    ))
    .unwrap();
    assert!(!text.contains('\u{fffd}'));
    assert!(text.contains("``const x = \"item:123\".match(luaPattern`item:(%d+)`);``"));
}
