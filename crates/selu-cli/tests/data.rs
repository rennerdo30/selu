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
        (
            "const a = [1]; for (const x of a) { continue; }",
            "SELU0390",
        ),
        ("const a = [1]; a.map(x => x);", "SELU0390"),
    ] {
        assert!(codes(src).contains(&code.into()), "{src}");
    }
}
