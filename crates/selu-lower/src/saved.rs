//! Cold-path SavedVariables initialization. Generated bindings are always reserved.
use crate::{
    Lowerer,
    lua::{self, *},
};
use selu_syntax::{Program, Stmt, StmtKind};
fn s(kind: LStmtKind) -> LStmt {
    LStmt { kind, span: None }
}
fn g(v: &str) -> LExpr {
    name(&format!("__selu_saved_{v}"))
}
fn st(v: &str) -> LExpr {
    LExpr::Str(v.into())
}
fn n(v: u32) -> LExpr {
    LExpr::Number(v.to_string())
}
fn f(v: LExpr, k: &str) -> LExpr {
    LExpr::Field(Box::new(v), k.into())
}
fn ix(v: LExpr, k: LExpr) -> LExpr {
    LExpr::Index(Box::new(v), Box::new(k))
}
fn call(v: &str, args: Vec<LExpr>) -> LExpr {
    lua::call(g(v), args)
}
fn invoke(v: LExpr) -> LStmt {
    s(LStmtKind::Call(v))
}
fn assign(a: LExpr, b: LExpr) -> LStmt {
    s(LStmtKind::Assign(vec![a], vec![b]))
}
fn local(v: &str, value: LExpr) -> LStmt {
    s(LStmtKind::Local(
        vec![format!("__selu_saved_{v}")],
        vec![value],
    ))
}
fn fun(params: &[&str], body: LBlock) -> LExpr {
    LExpr::Function(LFunction {
        params: params.iter().map(|v| format!("__selu_saved_{v}")).collect(),
        vararg: false,
        body,
    })
}
fn iff(cond: LExpr, body: LBlock) -> LStmt {
    s(LStmtKind::If(vec![(cond, body)], None))
}
fn eq(a: LExpr, b: LExpr) -> LExpr {
    bin(BinOp::Eq, a, b)
}
fn ret(v: LExpr) -> LStmt {
    s(LStmtKind::Return(vec![v]))
}
fn rt() -> LExpr {
    g("runtime")
}
fn method(v: LExpr, k: &str, args: Vec<LExpr>) -> LStmt {
    invoke(LExpr::Method(Box::new(v), k.into(), args))
}
fn helper(k: &str, args: Vec<LExpr>) -> LExpr {
    lua::call(f(rt(), k), args)
}
fn warning(message: &str) -> LStmt {
    invoke(helper(
        "warn",
        vec![bin(BinOp::Concat, g("key"), st(&format!(": {message}")))],
    ))
}
fn each(iter: LExpr, body: LBlock) -> LStmt {
    s(LStmtKind::GenFor {
        names: vec!["__selu_saved_key".into(), "__selu_saved_value".into()],
        exprs: vec![iter],
        body,
    })
}
fn runtime() -> LBlock {
    let copy_body = vec![
        iff(
            bin(BinOp::Ne, call("type", vec![g("value")]), st("table")),
            vec![ret(g("value"))],
        ),
        local("result", LExpr::Table(vec![])),
        each(
            call("pairs", vec![g("value")]),
            vec![assign(
                ix(g("result"), g("key")),
                helper("copy", vec![g("value")]),
            )],
        ),
        ret(g("result")),
    ];
    let fill_body = vec![each(
        call("pairs", vec![g("defaults")]),
        vec![s(LStmtKind::If(
            vec![
                (
                    eq(ix(g("data"), g("key")), LExpr::Nil),
                    vec![assign(
                        ix(g("data"), g("key")),
                        helper("copy", vec![g("value")]),
                    )],
                ),
                (
                    bin(
                        BinOp::And,
                        bin(BinOp::Ne, ix(g("objects"), g("key")), LExpr::Nil),
                        eq(call("type", vec![ix(g("data"), g("key"))]), st("table")),
                    ),
                    vec![invoke(helper(
                        "fill",
                        vec![
                            ix(g("data"), g("key")),
                            g("value"),
                            ix(g("objects"), g("key")),
                        ],
                    ))],
                ),
            ],
            None,
        ))],
    )];
    let warn_body = vec![
        local("wow", ix(name("__selu_ns"), LExpr::Bool(false))),
        s(LStmtKind::If(
            vec![(
                bin(BinOp::Ne, g("wow"), LExpr::Nil),
                vec![iff(
                    lua::call(f(g("wow"), "enabled"), vec![st("warn"), g("message")]),
                    vec![invoke(lua::call(
                        f(g("wow"), "write"),
                        vec![st("warn"), g("message"), g("message")],
                    ))],
                )],
            )],
            Some(vec![invoke(call("print", vec![g("message")]))]),
        )),
    ];
    let data = g("data");
    let version = g("version");
    let ver = f(data.clone(), "__selu_version");
    let invalid = bin(
        BinOp::Or,
        bin(BinOp::Ne, call("type", vec![version.clone()]), st("number")),
        bin(
            BinOp::Or,
            bin(BinOp::Lt, version.clone(), n(1)),
            bin(BinOp::Ne, bin(BinOp::Mod, version.clone(), n(1)), n(0)),
        ),
    );
    let load_body = vec![
        local("data", call("rawget", vec![g("_G"), g("key")])),
        iff(
            eq(data.clone(), LExpr::Nil),
            vec![
                assign(data.clone(), LExpr::Table(vec![])),
                assign(ver.clone(), g("current")),
            ],
        ),
        iff(
            bin(BinOp::Ne, call("type", vec![data.clone()]), st("table")),
            vec![warning("saved root is not a table"), ret(data.clone())],
        ),
        local(
            "version",
            call("rawget", vec![data.clone(), st("__selu_version")]),
        ),
        iff(
            eq(version.clone(), LExpr::Nil),
            vec![assign(version.clone(), n(1))],
        ),
        iff(
            invalid,
            vec![warning("invalid saved version"), ret(data.clone())],
        ),
        iff(
            bin(BinOp::Gt, version.clone(), g("current")),
            vec![
                warning("saved data is newer than this addon"),
                ret(data.clone()),
            ],
        ),
        s(LStmtKind::While(
            bin(BinOp::Lt, version.clone(), g("current")),
            vec![
                local("migration", ix(g("migrations"), version.clone())),
                iff(
                    eq(g("migration"), LExpr::Nil),
                    vec![warning("missing saved migration"), ret(data.clone())],
                ),
                local("candidate", helper("copy", vec![data.clone()])),
                s(LStmtKind::Local(
                    vec!["__selu_saved_ok".into(), "__selu_saved_message".into()],
                    vec![call("pcall", vec![g("migration"), g("candidate")])],
                )),
                iff(
                    LExpr::Unary(UnOp::Not, Box::new(g("ok"))),
                    vec![
                        invoke(helper(
                            "warn",
                            vec![bin(
                                BinOp::Concat,
                                g("key"),
                                bin(
                                    BinOp::Concat,
                                    st(": migration failed: "),
                                    call("tostring", vec![g("message")]),
                                ),
                            )],
                        )),
                        ret(data.clone()),
                    ],
                ),
                assign(version.clone(), bin(BinOp::Add, version.clone(), n(1))),
                assign(data.clone(), g("candidate")),
                assign(ver.clone(), version.clone()),
                assign(ix(g("_G"), g("key")), data.clone()),
            ],
        )),
        invoke(helper(
            "fill",
            vec![data.clone(), g("defaults"), g("objects")],
        )),
        assign(ver, g("current")),
        assign(ix(g("_G"), g("key")), data.clone()),
        ret(data),
    ];
    vec![
        assign(rt(), LExpr::Table(vec![])),
        assign(f(name("__selu_ns"), "__selu_saved"), rt()),
        assign(f(rt(), "copy"), fun(&["value"], copy_body)),
        assign(
            f(rt(), "fill"),
            fun(&["data", "defaults", "objects"], fill_body),
        ),
        assign(f(rt(), "warn"), fun(&["message"], warn_body)),
        assign(
            f(rt(), "load"),
            fun(
                &["key", "current", "defaults", "objects", "migrations"],
                load_body,
            ),
        ),
        assign(f(rt(), "entries"), LExpr::Table(vec![])),
        assign(f(rt(), "loaders"), LExpr::Table(vec![])),
        local("frame", call("CreateFrame", vec![st("Frame")])),
        method(g("frame"), "RegisterEvent", vec![st("ADDON_LOADED")]),
        method(
            g("frame"),
            "SetScript",
            vec![
                st("OnEvent"),
                fun(
                    &["self", "event", "addon"],
                    vec![
                        iff(
                            bin(BinOp::Ne, g("addon"), name("__selu_addon")),
                            vec![s(LStmtKind::Return(vec![]))],
                        ),
                        each(
                            call("ipairs", vec![f(rt(), "loaders")]),
                            vec![
                                s(LStmtKind::Local(
                                    vec!["__selu_saved_ok".into(), "__selu_saved_message".into()],
                                    vec![call("pcall", vec![g("value")])],
                                )),
                                iff(
                                    LExpr::Unary(UnOp::Not, Box::new(g("ok"))),
                                    vec![invoke(helper(
                                        "warn",
                                        vec![call("tostring", vec![g("message")])],
                                    ))],
                                ),
                            ],
                        ),
                        assign(f(rt(), "entries"), LExpr::Nil),
                        assign(f(rt(), "loaders"), LExpr::Nil),
                        method(g("self"), "UnregisterEvent", vec![st("ADDON_LOADED")]),
                        method(g("self"), "SetScript", vec![st("OnEvent"), LExpr::Nil]),
                        assign(f(name("__selu_ns"), "__selu_saved"), LExpr::Nil),
                    ],
                ),
            ],
        ),
    ]
}
pub fn global_name(addon: &str, _module: &str, saved: &str) -> String {
    format!("{addon}_{saved}")
}
pub fn declaration_global(addon: &str, saved: &str, adopted: bool) -> String {
    if adopted {
        saved.into()
    } else {
        global_name(addon, "", saved)
    }
}
pub(crate) fn has_saved(p: &Program) -> bool {
    p.stmts
        .iter()
        .any(|s| matches!(inner(s).kind, StmtKind::Saved { .. }))
}
fn inner(s: &Stmt) -> &Stmt {
    if let StmtKind::Export(s) = &s.kind {
        s
    } else {
        s
    }
}
fn objects(ty: Option<&selu_check::Type>) -> LExpr {
    let fields = if let Some(selu_check::Type::Object(fs)) = ty {
        fs.iter()
            .filter(|(_, t)| matches!(t, selu_check::Type::Object(_)))
            .map(|(k, t)| LField::Named(k.clone(), objects(Some(t))))
            .collect()
    } else {
        vec![]
    };
    LExpr::Table(fields)
}
impl Lowerer<'_> {
    pub(crate) fn saved_stmt(&mut self, stmt: &Stmt, forward: bool, out: &mut LBlock) {
        match &stmt.kind {
            StmtKind::Saved {
                name: saved,
                version,
                defaults,
                adopted,
                ..
            } => {
                if !forward {
                    out.push(s(LStmtKind::Local(vec![saved.clone()], vec![])));
                }
                let key = declaration_global(&self.module.addon_name, saved, *adopted);
                let entry = ix(f(rt(), "entries"), st(&key));
                out.push(assign(entry.clone(), LExpr::Table(vec![])));
                let mut body = vec![];
                let defaults_value = self.expr(defaults, &mut body);
                body.push(assign(
                    name(saved),
                    helper(
                        "load",
                        vec![
                            st(&key),
                            n(*version),
                            defaults_value,
                            objects(self.types.get(defaults.id)),
                            entry,
                        ],
                    ),
                ));
                let loaders = f(rt(), "loaders");
                out.push(assign(
                    ix(
                        loaders.clone(),
                        bin(BinOp::Add, LExpr::Unary(UnOp::Len, Box::new(loaders)), n(1)),
                    ),
                    fun(&[], body),
                ));
            }
            StmtKind::Migrate {
                name: saved,
                from,
                param,
                body,
                ..
            } => {
                // The matching declaration controls whether this key adopts a legacy global.
                let key = self
                    .saved_globals
                    .get(saved)
                    .cloned()
                    .expect("saved declaration checked");
                let func = self.function(std::slice::from_ref(param), body);
                out.push(assign(
                    ix(ix(f(rt(), "entries"), st(&key)), n(*from)),
                    LExpr::Function(func),
                ));
            }
            _ => unreachable!(),
        }
    }
}
pub(crate) fn captures(program: &Program) -> LBlock {
    if !has_saved(program) {
        return vec![];
    }
    let mut out = vec![];
    for global in [
        "_G",
        "type",
        "pairs",
        "ipairs",
        "CreateFrame",
        "rawget",
        "pcall",
        "print",
        "tostring",
    ] {
        out.push(local(global, name(global)));
    }
    out.push(local("runtime", f(name("__selu_ns"), "__selu_saved")));
    out.push(iff(eq(rt(), LExpr::Nil), runtime()));
    out
}
pub(crate) fn globals(program: &Program, addon: &str) -> std::collections::HashMap<String, String> {
    program
        .stmts
        .iter()
        .filter_map(|s| {
            if let StmtKind::Saved { name, adopted, .. } = &inner(s).kind {
                Some((name.clone(), declaration_global(addon, name, *adopted)))
            } else {
                None
            }
        })
        .collect()
}
