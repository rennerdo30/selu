mod common;
use common::{codes, run};

#[test]
fn literals_indices_templates_and_optional_false() {
    assert_eq!(
        run(r#"
        const xs = [10, 20];
        xs[0] = 30;
        const o = { enabled: false };
        print(xs[0], xs[1], o?.enabled);
        print(`100% ${xs[0]} ${true}`);
        const m = new Map<string, number>();
        const s = new Set<number>();
        const w = new WeakMap<{ id: number }, string>();
    "#),
        ["30\t20\tfalse", "100% 30 true"]
    );
}

#[test]
fn rejects_holes_and_bad_indices() {
    assert!(codes("const xs = [null];").contains(&"SELU0350".into()));
    assert!(codes("const xs = [1]; print(xs[\"x\"]);").contains(&"SELU0352".into()));
    assert!(codes("let n: number? = null; print(`${n}`);").contains(&"SELU0353".into()));
}

#[test]
fn collection_methods() {
    assert_eq!(
        run(r#"
        const xs: number[] = [];
        xs.push(3); xs.push(1); xs.insert(1, 2);
        print(xs.length, xs.indexOf(2), xs.includes(9));
        print(xs.removeAt(0), xs.pop());
        xs.push(4); xs.sort();
        const copy = xs.slice(0, 2);
        print(copy.join("|"));
        const m = new Map<string, boolean>();
        m.set("a", false); print(m.has("a"), m.get("a"), m.count());
        print(m.delete("a"), m.delete("a")); m.clear(); print(m.count());
        const s = new Set<number>(); s.add(3); s.add(3); print(s.count(), s.has(3));
        print(s.delete(3)); s.clear(); print(s.count());
    "#),
        [
            "3\t1\tfalse",
            "3\t1",
            "2|4",
            "true\tfalse\t1",
            "true\tfalse",
            "0",
            "1\ttrue",
            "true",
            "0"
        ]
    );
}

#[test]
fn strings_and_math() {
    assert_eq!(
        run(r#"
        const s = " abc ";
        print(s.length, s.trim(), s.upper(), s.lower(), s.slice(1, 4));
        print(s.startsWith(" "), s.endsWith(" "), s.includes("b"));
        const parts = "a..b..".split(".."); print(parts.join("|"));
        const chars = "ab".split(""); print(chars.join("-"));
        print(Math.floor(1.9), Math.ceil(1.1), Math.round(-1.5), Math.abs(-2));
        print(Math.min(1, 2), Math.max(1, 2), Math.clamp(9, 1, 3));
    "#),
        [
            "5\tabc\t ABC \t abc \tabc",
            "true\ttrue\ttrue",
            "a|b|",
            "a-b",
            "1\t2\t-1\t2",
            "1\t2\t3"
        ]
    );
}

#[test]
fn iteration_and_objects() {
    assert_eq!(
        run(r#"
        const xs = [2, 3]; for (const x of xs) { print(x); }
        const m = new Map<string, number>(); m.set("k", 5);
        for (const [k, v] of m) { print(k, v); }
        const s = new Set<number>(); s.add(7); for (const x of s) { print(x); }
        const o = { n: 1 }; o.n = 2; print(o.n);
    "#),
        ["2", "3", "k\t5", "7", "2"]
    );
    assert!(codes("const o = { n: 1 }; o.n = false;").contains(&"SELU0355".into()));
    assert!(codes("const xs = []; ").contains(&"SELU0351".into()));
    assert!(codes("const xs = [1]; xs[0] = null;").contains(&"SELU0355".into()));
}

#[test]
fn slices_and_evaluation_order() {
    assert_eq!(
        run(r#"
        const xs = [1, 2, 3]; const tail = xs.slice(-2, 99); print(tail.join("/"));
        const s = "abc"; print(s.slice(-2, 99)); print(s.endsWith(""));
        let calls = 0;
        function value(): number { calls = calls + 1; return calls; }
        const a = [value(), value()]; print(a[0], a[1], calls);
    "#),
        ["2/3", "bc", "true", "1\t2\t2"]
    );
}

#[test]
fn push_and_index_do_not_allocate_garbage() {
    let lua =
        common::compile("function exercise(xs: number[], n: number) { xs.push(n); xs[0] = n; }");
    assert!(lua.contains("#"));
    assert!(!lua.contains("table.insert"));
    let harness = format!(
        r#"{lua}
        local xs = {{0}}
        for i = 1, 1024 do exercise(xs, i) end
        for i = 1025, 2, -1 do xs[i] = nil end
        collectgarbage("collect")
        collectgarbage("stop")
        local before = collectgarbage("count")
        for i = 1, 1024 do exercise(xs, i) end
        for i = 1025, 2, -1 do xs[i] = nil end
        local after = collectgarbage("count")
        assert(after == before, "push/index allocated garbage")
        assert(xs[1] == 1024)
    "#
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("allocation.lua".into(), harness)])
        .unwrap();
}

#[test]
fn weak_maps_optional_fields_and_errors() {
    assert_eq!(
        run(r#"
        const key = { id: 1 }; const w = new WeakMap<{ id: number }, boolean>();
        w.set(key, false); print(w.get(key), w.has(key), w.count()); w.clear(); print(w.has(key));
        const o: { enabled?: boolean } = {}; print(o.enabled, o?.enabled);
        const bs = [true, false]; print(bs.join("/"));
        const m = new Map<string, number>(); m.set("a", 1); m.clear(); print(m.count());
    "#),
        ["false\ttrue\t1", "false", "nil\tnil", "true/false", "0"]
    );
    for (src, code) in [
        ("const a = [1, false];", "SELU0351"),
        ("const a = new Map<string?, number>();", "SELU0350"),
        ("const a = [1]; a.length = 0;", "SELU0355"),
        ("const a = [1]; a.push(false);", "SELU0355"),
        ("const a = [1]; a.push();", "SELU0356"),
    ] {
        assert!(codes(src).contains(&code.into()), "{src}");
    }
}

#[test]
fn review_receiver_order() {
    assert_eq!(
        run(
            "let xs = [1]; const old = xs; function value(): number? { xs = [9]; return 2; } xs.push(value() ?? 0); print(old.join(), xs.join());"
        ),
        ["1,2\t9"]
    );
    assert_eq!(
        run("let xs = [1]; function i(): number? { xs = [9]; return 0; } print(xs[i() ?? 0]);"),
        ["1"]
    );
}
#[test]
fn review_nullable_arrays() {
    assert!(
        codes("const xs: number?[] = []; xs.push(null); print(xs.length);")
            .contains(&"SELU0350".into())
    );
    for src in [
        "function f(xs: number?[]) {}",
        "function f(): number?[] { return [1]; }",
    ] {
        assert!(codes(src).contains(&"SELU0350".into()));
    }
}
#[test]
fn review_optional_length() {
    assert_eq!(
        run("const xs = [1, 2]; const s = \"abc\"; print(xs?.length, s?.length);"),
        ["2\t3"]
    );
}
#[test]
fn review_object_dispatch() {
    let lua = common::compile(
        "function f(x: string): string { return x; } const o = { slice: f }; print(o.slice(\"hi\"));",
    );
    assert!(!lua.contains("string.sub"), "{lua}");
    assert!(!lua.contains("math.floor"), "{lua}");
}
#[test]
fn review_shrinking_iteration() {
    assert_eq!(
        run("const xs = [1, 2, 3]; for (const x of xs) { print(x + 1); xs.pop(); }"),
        ["2", "3"]
    );
}
#[test]
fn review_callbacks() {
    assert_eq!(
        run(r#"
        const xs = [1, 2, 3];
        function twice(x: number): number { return x * 2; }
        const ys = xs.map(twice); print(ys.join());
        function positive(x: number): boolean { return x > 1; }
        const zs = xs.filter(positive); print(zs.join());
        print(xs.find(positive), xs.some(positive), xs.every(positive));
        function add(a: number, x: number): number { return a + x; }
        print(xs.reduce(add, 0)); print(xs.reduce(add));
        function show(x: number) { print(x); } xs.forEach(show);
        function compare(a: number, b: number): number { return b - a; }
        xs.sort(compare); print(xs.join());
    "#),
        [
            "2,4,6",
            "2,3",
            "2\ttrue\tfalse",
            "6",
            "6",
            "1",
            "2",
            "3",
            "3,2,1"
        ]
    );
}
#[test]
fn review_check_once() {
    assert_eq!(codes("const xs = [1]; xs.slice().slice(0);"), ["SELU0356"]);
}
#[test]
fn review_empty_removal() {
    assert_eq!(
        run("const xs: number[] = []; print(xs.pop()); print(xs.removeAt(0));"),
        ["nil", "nil"]
    );
}
#[test]
fn review_readable_methods() {
    let lua = common::compile("const xs = [1, 2]; xs.push(3); for (const x of xs) { print(x); }");
    assert!(lua.contains("local xs = { 1, 2 }"), "{lua}");
    assert!(lua.contains("xs[#xs + 1] = 3"), "{lua}");
    assert!(!lua.contains("local _ = nil"), "{lua}");
}
#[test]
fn review_large_literal_index() {
    let lua = common::compile("const xs = [1]; print(xs[9007199254740992]);");
    assert!(lua.contains("9007199254740992 + 1"), "{lua}");
}
#[test]
fn review_field_diagnostic() {
    let src = "const object = { good: 1 }; print(object.bad);";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    let d = ds.iter().find(|d| d.code == "SELU0330").unwrap();
    let sp = d.span.as_ref().unwrap();
    assert!(src[sp.start..sp.end].contains("bad"));
    assert!(d.help.is_some());
}

#[test]
fn review_store_receiver_without_pre() {
    assert_eq!(
        run(
            "let xs = [1]; const old = xs; function value(): number { xs = [9]; return 2; } xs.push(value()); print(old.join(), xs.join());"
        ),
        ["1,2\t9"]
    );
}

#[test]
fn review_callback_types() {
    assert_eq!(
        codes("const xs = [1]; function wrong(x: string): number { return 1; } xs.map(wrong);"),
        ["SELU0355"]
    );
    assert_eq!(
        codes("const xs = [1]; function wrong(x: number): number { return x; } xs.filter(wrong);"),
        ["SELU0355"]
    );
    assert_eq!(
        codes("const xs = [1]; function wrong(x: number): number? { return null; } xs.map(wrong);"),
        ["SELU0350"]
    );
}

#[test]
fn review_receiver_before_stabilized_argument() {
    assert_eq!(
        run(
            "let s = \"abc\"; function suffix(): string { s = \"z\"; return \"bc\"; } print(s.endsWith(suffix()));"
        ),
        ["true"]
    );
}

macro_rules! transfer_test {
    ($name:ident, $setup:expr, $binding:expr, $transfer:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(run(&format!("{} let seen = 0; let visits = 0; for ({}) {{ seen++; {} visits++; }} print(seen, visits);", $setup, $binding, $transfer)), [$expected]);
        }
    };
}
transfer_test!(
    for_of_array_continue,
    "const xs = [1, 2, 3, 4];",
    "const x of xs",
    "if (seen == 1) { continue; }",
    "4\t3"
);
transfer_test!(
    for_of_array_break,
    "const xs = [1, 2, 3, 4];",
    "const x of xs",
    "if (seen == 2) { break; }",
    "2\t1"
);
transfer_test!(
    for_of_array_both,
    "const xs = [1, 2, 3, 4];",
    "const x of xs",
    "if (seen == 1) { continue; } if (seen == 3) { break; }",
    "3\t1"
);
transfer_test!(
    for_of_map_continue,
    "const xs = new Map<number, number>(); xs.set(1, 1); xs.set(2, 2); xs.set(3, 3); xs.set(4, 4);",
    "const [k, v] of xs",
    "if (seen == 1) { continue; }",
    "4\t3"
);
transfer_test!(
    for_of_map_break,
    "const xs = new Map<number, number>(); xs.set(1, 1); xs.set(2, 2); xs.set(3, 3); xs.set(4, 4);",
    "const [k, v] of xs",
    "if (seen == 2) { break; }",
    "2\t1"
);
transfer_test!(
    for_of_map_both,
    "const xs = new Map<number, number>(); xs.set(1, 1); xs.set(2, 2); xs.set(3, 3); xs.set(4, 4);",
    "const [k, v] of xs",
    "if (seen == 1) { continue; } if (seen == 3) { break; }",
    "3\t1"
);
transfer_test!(
    for_of_set_continue,
    "const xs = new Set<number>(); xs.add(1); xs.add(2); xs.add(3); xs.add(4);",
    "const x of xs",
    "if (seen == 1) { continue; }",
    "4\t3"
);
transfer_test!(
    for_of_set_break,
    "const xs = new Set<number>(); xs.add(1); xs.add(2); xs.add(3); xs.add(4);",
    "const x of xs",
    "if (seen == 2) { break; }",
    "2\t1"
);
transfer_test!(
    for_of_set_both,
    "const xs = new Set<number>(); xs.add(1); xs.add(2); xs.add(3); xs.add(4);",
    "const x of xs",
    "if (seen == 1) { continue; } if (seen == 3) { break; }",
    "3\t1"
);
#[test]
fn for_of_closures_capture_each_turn() {
    assert_eq!(
        run(r#"
        function initial(): number { return 0; }
        const fs = [initial]; fs.pop();
        const xs = [2, 4, 6];
        for (const x of xs) { function capture(): number { return x; } fs.push(capture); }
        for (const f of fs) { print(f()); }
        function empty(): string { return ""; }
        const gs = [empty]; gs.pop();
        const m = new Map<string, number>(); m.set("a", 1); m.set("b", 2);
        for (const [k, v] of m) { function capture(): string { return `${k}:${v}`; } gs.push(capture); }
        const values: string[] = []; for (const g of gs) { values.push(g()); }
        values.sort(); print(values.join());
    "#),
        ["2", "4", "6", "a:1,b:2"]
    );
}

#[test]
fn callback_arrows_get_context() {
    assert_eq!(
        run(r#"
        const xs = [1, 2, 3]; let sum = 0;
        xs.forEach(x => { sum += x; }); print(sum);
        const ys = xs.map(x => x * 2); print(ys.join());
        const zs = xs.filter(x => x > 1); print(zs.join());
        print(xs.find(x => x > 1), xs.some(x => x > 1), xs.every(x => x > 1));
        print(xs.reduce((a, x) => a + x, 0));
        xs.sort((a, b) => b - a); print(xs.join());
        const scale = 3; const captured = xs.map(x => x * scale); print(captured.join());
    "#),
        ["6", "2,4,6", "2,3", "2\ttrue\tfalse", "6", "3,2,1", "9,6,3"]
    );
}

#[test]
fn contextual_initializer_and_array_destructuring() {
    assert_eq!(
        run(
            "const xs: number[] = []; const f: (x: number) => number = x => x + x; xs.push(f(3)); print(xs[0]); const [a, , c] = [1, 2, 3]; print(a, c);"
        ),
        ["6", "1\t3"]
    );
}

#[test]
fn object_slice_is_an_ordinary_function_call() {
    assert_eq!(
        run(
            "function f(x: string): string { return x; } const o = { slice: f }; print(o.slice(\"hi\"));"
        ),
        ["hi"]
    );
}

macro_rules! arrow_context_test {
    ($name:ident, $source:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(run(concat!("const xs = [1, 2, 3]; ", $source)), $expected);
        }
    };
}
arrow_context_test!(
    arrow_for_each_context,
    "xs.forEach(x => { print(x + x); });",
    ["2", "4", "6"]
);
arrow_context_test!(
    arrow_map_context,
    "const ys = xs.map(x => x + x); print(ys.join());",
    ["2,4,6"]
);
arrow_context_test!(
    arrow_filter_context,
    "const ys = xs.filter(x => x + x > 2); print(ys.join());",
    ["2,3"]
);
arrow_context_test!(arrow_find_context, "print(xs.find(x => x + x > 2));", ["2"]);
arrow_context_test!(
    arrow_some_context,
    "print(xs.some(x => x + x > 2));",
    ["true"]
);
arrow_context_test!(
    arrow_every_context,
    "print(xs.every(x => x + x > 2));",
    ["false"]
);
arrow_context_test!(
    arrow_reduce_context,
    "print(xs.reduce((a, x) => a + x, 0));",
    ["6"]
);
arrow_context_test!(
    arrow_sort_context,
    "xs.sort((a, b) => b + b - a - a); print(xs.join());",
    ["3,2,1"]
);

#[test]
fn callback_minimum_arity_and_rest() {
    assert_eq!(
        run(r#"
        const xs = [3, 1, 2];
        function optional(x: number, i: number = 0, a: number[] = [4], unused: number = 9): number { return x + i; }
        const ys = xs.map(optional); print(ys.join());
        function compare(a: number, ...bs: number[]): number { return a - bs[0]; }
        xs.sort(compare); print(xs.join());
    "#),
        ["3,2,4", "1,2,3"]
    );
    assert_eq!(
        codes(
            "const xs = [1]; function wrong(a: number, ...bs: string[]): number { return a; } xs.sort(wrong);"
        ),
        ["SELU0355"]
    );
    assert_eq!(
        codes(
            "const xs = [1]; function wrong(a: number, b: number, c: number[], d: number): number { return a; } xs.map(wrong);"
        ),
        ["SELU0355"]
    );
}
#[test]
fn sb_class_arrays_clear_preserves_aliases() {
    assert_eq!(
        run(r#"
        class Samples { samples: number[] = []; static shared: number[] = []; }
        const a = new Samples(); const b = new Samples();
        a.samples.push(4); const alias = a.samples;
        a.samples.clear(); alias.push(7);
        print(a.samples.length, a.samples[0], b.samples.length);
        Samples.shared.push(9); Samples.shared.clear(); print(Samples.shared.length);
        let calls = 0;
        function receiver(): number[] { calls += 1; return alias; }
        receiver().clear(); print(calls, alias.length);
        alias.clear(); alias.push(8); print(alias[0]);
    "#),
        ["1\t7\t0", "0", "1\t0", "8"]
    );
}
#[test]
fn sb_keyed_locale_catalog_is_checked() {
    assert_eq!(
        run(r#"
        type Sentences = { enabled: string, sold: string };
        const catalog: { enUS: Sentences, deDE: Sentences } = {
            enUS: { enabled: "Aura flash enabled.", sold: "Sold your junk." },
            deDE: { enabled: "Aurablitz aktiviert.", sold: "Dein Plunder wurde verkauft." }
        };
        function L(german: boolean): Sentences {
            if (german) { return catalog.deDE; }
            return catalog.enUS;
        }
        print(L(false).enabled, L(true).sold);
    "#),
        ["Aura flash enabled.\tDein Plunder wurde verkauft."]
    );
    assert!(!codes("type Sentences = { enabled: string }; const c: { enUS: Sentences, deDE: Sentences } = { enUS: { enabled: \"Enabled.\" } };").is_empty());
    assert!(!codes("type Sentences = { enabled: string, sold: string }; const c: { enUS: Sentences, deDE: Sentences } = { enUS: { enabled: \"On.\", sold: \"Sold.\" }, deDE: { enabled: \"An.\" } };").is_empty());
    assert!(!codes("const c = { enabled: \"Enabled.\" }; print(c.missing);").is_empty());
}
#[test]
fn sb_native_error_boundary_normalizes_and_chains() {
    let lua = common::compile("onError((e: Error) => { print(e.message); });");
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("seterrorhandler(function(value, extra) print(type(value), extra) end)")
        .unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    sim.eval("geterrorhandler()('early load failure', 7); geterrorhandler()(setmetatable({}, {__tostring=function() error('must not stringify') end}), 8)").unwrap();
    assert_eq!(
        sim.prints(),
        [
            "early load failure",
            "string\t7",
            "<table error>",
            "table\t8"
        ]
    );
    sim.set_secret_global("hidden").unwrap();
    sim.eval("geterrorhandler()(hidden, 9)").unwrap();
    assert_eq!(&sim.prints()[4..], ["<secret error>", "userdata\t9"]);
    assert!(codes("onError((e: string) => {});").contains(&"SELU0543".into()));
}

#[test]
fn sb_error_chaining_preserves_zero_arguments_and_survives_sink_failure() {
    let lua = common::compile("onError((e: Error) => { throw new Error(e.message); });");
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("seterrorhandler(function(...) print(select('#', ...)) end)")
        .unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    sim.eval("geterrorhandler()(); geterrorhandler()('failure', nil, 3)")
        .unwrap();
    assert_eq!(sim.prints(), ["0", "3"]);
}

#[test]
fn sb_record_samples_and_typed_catalog_formatting() {
    assert_eq!(
        run(r#"
        type Sample = { xp: number };
        class Buffer { samples: Sample[] = []; }
        const buffer = new Buffer(); buffer.samples.push({ xp: 9 });
        print(buffer.samples[0].xp); buffer.samples.clear();
        type Sentences = { sold: (count: number) => string };
        const catalog: { enUS: Sentences, deDE: Sentences } = {
            enUS: { sold: (count: number) => `Sold ${count} items. 100% complete.` },
            deDE: { sold: (count: number) => `${count} Gegenstände verkauft. 100% abgeschlossen.` }
        };
        print(catalog.deDE.sold(3));
    "#),
        ["9", "3 Gegenstände verkauft. 100% abgeschlossen."]
    );
    assert!(!codes("const xs = [1]; xs.clear(2);").is_empty());
    assert!(!codes("class Buffer { samples: number[] = []; } const b = new Buffer(); b.samples.push(\"bad\");").is_empty());
    assert!(
        !codes("const c = { sold: (count: number) => `${count} items.` }; c.sold(\"three\");")
            .is_empty()
    );
}

#[test]
fn sb_existing_error_identity_survives_native_boundary() {
    let mut lua = common::compile(
        "let seen: Error? = null; onError((e: Error) => { if (seen != null) { print(seen == e); } seen = e; });",
    );
    lua.push_str("\nlocal original = __selu_new_error('owned'); geterrorhandler()(original); geterrorhandler()(original)");
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("seterrorhandler(function() end)").unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["true"]);
}

#[test]
fn review_on_error_uses_production_error_range() {
    assert!(codes("onError((e: string) => {});").contains(&"SELU0543".into()));
    assert!(codes("onError();").contains(&"SELU0543".into()));
}

#[test]
fn review_real_callbacks_reach_typed_error_sink_once() {
    for setup in [
        "C_Timer.After(.1, function() error('timer failure') end)",
        "local f=CreateFrame('Frame'); f:SetScript('OnUpdate', function(self) self:SetScript('OnUpdate', nil); error('update failure') end)",
        "local f=CreateFrame('Frame'); f:RegisterEvent('PLAYER_LOGIN'); f:SetScript('OnEvent', function() error('event failure') end)",
        "local f=CreateFrame('Frame'); f:Hide(); f:SetScript('OnShow', function() error('show failure') end); C_Timer.After(.1, function() f:Show() end)",
    ] {
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("seterrorhandler(function() print('previous') end)")
            .unwrap();
        sim.load_files(&[(
            "t.lua".into(),
            common::compile("onError(e => { print(\"captured\"); });"),
        )])
        .unwrap();
        sim.eval(setup).unwrap();
        let _ = sim.fire("PLAYER_LOGIN", vec![]);
        sim.advance_time(0.1);
        assert_eq!(sim.prints(), ["captured", "previous"], "{setup}");
    }
}
