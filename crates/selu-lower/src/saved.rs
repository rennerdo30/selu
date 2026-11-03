//! Saved initialization is cold-path code; migration attempts work on deep copies.
use crate::{
    Lowerer,
    lua::{self, *},
};
use selu_syntax::{Stmt, StmtKind};
fn s(k: LStmtKind) -> LStmt {
    LStmt {
        kind: k,
        span: None,
    }
}
fn st(v: &str) -> LExpr {
    LExpr::Str(v.into())
}
fn n(v: u32) -> LExpr {
    LExpr::Number(v.to_string())
}
fn f(v: LExpr, key: &str) -> LExpr {
    LExpr::Field(Box::new(v), key.into())
}
fn ix(v: LExpr, k: LExpr) -> LExpr {
    LExpr::Index(Box::new(v), Box::new(k))
}
fn call(v: &str, a: Vec<LExpr>) -> LExpr {
    let callee = if matches!(
        v,
        "type" | "pairs" | "ipairs" | "CreateFrame" | "rawget" | "pcall" | "print" | "error"
    ) {
        format!("__selu_saved_{v}")
    } else {
        v.into()
    };
    lua::call(name(&callee), a)
}
fn assign(a: LExpr, b: LExpr) -> LStmt {
    s(LStmtKind::Assign(vec![a], vec![b]))
}
fn local(a: &str, b: LExpr) -> LStmt {
    s(LStmtKind::Local(vec![a.into()], vec![b]))
}
fn fun(ps: &[&str], body: LBlock) -> LExpr {
    LExpr::Function(LFunction {
        params: ps.iter().map(|x| (*x).into()).collect(),
        vararg: false,
        body,
    })
}
fn iff(c: LExpr, body: LBlock) -> LStmt {
    s(LStmtKind::If(vec![(c, body)], None))
}
fn eq(a: LExpr, b: LExpr) -> LExpr {
    bin(BinOp::Eq, a, b)
}
fn rt() -> LExpr {
    ix(name("__selu_saved_ns"), n(2))
}
fn invoke(e: LExpr) -> LStmt {
    s(LStmtKind::Call(e))
}
fn copy(v: LExpr) -> LExpr {
    lua::call(f(rt(), "copy"), vec![v])
}
fn runtime() -> LBlock {
    let mut copy_body = vec![
        iff(
            bin(BinOp::Ne, call("type", vec![name("value")]), st("table")),
            vec![s(LStmtKind::Return(vec![name("value")]))],
        ),
        local("result", LExpr::Table(vec![])),
    ];
    copy_body.push(s(LStmtKind::GenFor {
        names: vec!["key".into(), "value".into()],
        exprs: vec![call("pairs", vec![name("value")])],
        body: vec![assign(ix(name("result"), name("key")), copy(name("value")))],
    }));
    copy_body.push(s(LStmtKind::Return(vec![name("result")])));
    let fill_body = vec![s(LStmtKind::GenFor {
        names: vec!["key".into(), "value".into()],
        exprs: vec![call("pairs", vec![name("defaults")])],
        body: vec![s(LStmtKind::If(
            vec![
                (
                    eq(ix(name("data"), name("key")), LExpr::Nil),
                    vec![assign(ix(name("data"), name("key")), copy(name("value")))],
                ),
                (
                    bin(
                        BinOp::And,
                        eq(call("type", vec![name("value")]), st("table")),
                        eq(
                            call("type", vec![ix(name("data"), name("key"))]),
                            st("table"),
                        ),
                    ),
                    vec![invoke(lua::call(
                        f(rt(), "fill"),
                        vec![ix(name("data"), name("key")), name("value")],
                    ))],
                ),
            ],
            None,
        ))],
    })];
    vec![
        assign(rt(), LExpr::Table(vec![])),
        assign(f(rt(), "copy"), fun(&["value"], copy_body)),
        assign(f(rt(), "fill"), fun(&["data", "defaults"], fill_body)),
        assign(f(rt(), "loaders"), LExpr::Table(vec![])),
        local("__selu_saved_frame", call("CreateFrame", vec![st("Frame")])),
        invoke(LExpr::Method(
            Box::new(name("__selu_saved_frame")),
            "RegisterEvent".into(),
            vec![st("ADDON_LOADED")],
        )),
        invoke(LExpr::Method(
            Box::new(name("__selu_saved_frame")),
            "SetScript".into(),
            vec![
                st("OnEvent"),
                fun(
                    &["self", "event", "addon"],
                    vec![
                        iff(
                            bin(BinOp::Ne, name("addon"), name("__selu_saved_addon")),
                            vec![s(LStmtKind::Return(vec![]))],
                        ),
                        s(LStmtKind::GenFor {
                            names: vec!["_".into(), "load".into()],
                            exprs: vec![call("ipairs", vec![f(rt(), "loaders")])],
                            body: vec![invoke(call("load", vec![]))],
                        }),
                        invoke(LExpr::Method(
                            Box::new(name("self")),
                            "UnregisterEvent".into(),
                            vec![st("ADDON_LOADED")],
                        )),
                    ],
                ),
            ],
        )),
    ]
}
/// Hex-encoding UTF-8 components is injective and avoids Lua keywords/punctuation.
pub fn global_name(addon: &str, module: &str, saved: &str) -> String {
    fn hex(s: &str) -> String {
        s.bytes().map(|b| format!("{b:02x}")).collect()
    }
    format!("Selu_{}_{}_{}", hex(addon), hex(module), hex(saved))
}
impl Lowerer<'_> {
    pub(crate) fn saved_stmt(&mut self, stmt: &Stmt, forward: bool, out: &mut LBlock) {
        match &stmt.kind {
            StmtKind::Saved {
                name: saved,
                version,
                defaults,
                ..
            } => {
                out.push(s(LStmtKind::Local(
                    vec!["__selu_saved_addon".into(), "__selu_saved_ns".into()],
                    vec![LExpr::Vararg],
                )));
                out.push(assign(
                    name("__selu_saved_ns"),
                    bin(BinOp::Or, name("__selu_saved_ns"), LExpr::Table(vec![])),
                ));
                out.push(iff(eq(rt(), LExpr::Nil), runtime()));
                if !forward {
                    out.push(s(LStmtKind::Local(vec![saved.clone()], vec![])));
                }
                let migrations = format!("__selu_migrations_{saved}");
                out.push(local(&migrations, LExpr::Table(vec![])));
                // Build the global key from the actual addon loader name at runtime.
                let suffix = global_name("", &self.module.key, saved)
                    .strip_prefix("Selu__")
                    .unwrap()
                    .to_owned();
                // Use a deterministic hex encoder for the addon name, without relying on string methods.
                let encoder = fun(
                    &["text"],
                    vec![
                        local("result", st("")),
                        s(LStmtKind::NumFor {
                            var: "i".into(),
                            start: n(1),
                            stop: LExpr::Unary(UnOp::Len, Box::new(name("text"))),
                            step: None,
                            body: vec![assign(
                                name("result"),
                                bin(
                                    BinOp::Concat,
                                    name("result"),
                                    lua::call(
                                        name("__selu_saved_format"),
                                        vec![
                                            st("%02x"),
                                            lua::call(
                                                name("__selu_saved_byte"),
                                                vec![name("text"), name("i")],
                                            ),
                                        ],
                                    ),
                                ),
                            )],
                        }),
                        s(LStmtKind::Return(vec![name("result")])),
                    ],
                );
                let key = format!("__selu_saved_key_{saved}");
                out.push(local(
                    &key,
                    bin(
                        BinOp::Concat,
                        st("Selu_"),
                        bin(
                            BinOp::Concat,
                            lua::call(encoder, vec![name("__selu_saved_addon")]),
                            st(&format!("_{suffix}")),
                        ),
                    ),
                ));
                let data = name("data");
                let ver = f(data.clone(), "__selu_version");
                let mut body = vec![
                    local(
                        "data",
                        call("rawget", vec![name("__selu_saved__G"), name(&key)]),
                    ),
                    local("fresh", eq(data.clone(), LExpr::Nil)),
                    iff(
                        name("fresh"),
                        vec![
                            assign(data.clone(), LExpr::Table(vec![])),
                            assign(ver.clone(), n(*version)),
                        ],
                    ),
                    local("version", bin(BinOp::Or, ver.clone(), n(1))),
                    iff(
                        bin(BinOp::Gt, name("version"), n(*version)),
                        vec![
                            invoke(call(
                                "print",
                                vec![st("Selu warning: saved data is newer than this addon")],
                            )),
                            assign(name(saved), data.clone()),
                            s(LStmtKind::Return(vec![])),
                        ],
                    ),
                ];
                body.push(s(LStmtKind::While(
                    bin(BinOp::Lt, name("version"), n(*version)),
                    vec![
                        local("migration", ix(name(&migrations), name("version"))),
                        iff(
                            eq(name("migration"), LExpr::Nil),
                            vec![invoke(call("error", vec![st("missing saved migration")]))],
                        ),
                        local("candidate", copy(data.clone())),
                        s(LStmtKind::Local(
                            vec!["ok".into(), "message".into()],
                            vec![call("pcall", vec![name("migration"), name("candidate")])],
                        )),
                        iff(
                            LExpr::Unary(UnOp::Not, Box::new(name("ok"))),
                            vec![
                                invoke(call(
                                    "print",
                                    vec![st("Selu warning: migration failed"), name("message")],
                                )),
                                assign(name(saved), data.clone()),
                                s(LStmtKind::Return(vec![])),
                            ],
                        ),
                        assign(name("version"), bin(BinOp::Add, name("version"), n(1))),
                        assign(data.clone(), name("candidate")),
                        assign(ver.clone(), name("version")),
                        assign(ix(name("__selu_saved__G"), name(&key)), data.clone()),
                    ],
                )));
                let mut pre = vec![];
                let defaults = self.expr(defaults, &mut pre);
                body.extend(pre);
                body.push(invoke(lua::call(
                    f(rt(), "fill"),
                    vec![data.clone(), defaults],
                )));
                body.push(assign(ver, n(*version)));
                body.push(assign(
                    ix(name("__selu_saved__G"), name(&key)),
                    data.clone(),
                ));
                body.push(assign(name(saved), data));
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
                let func = self.function(std::slice::from_ref(param), body);
                out.push(assign(
                    ix(name(&format!("__selu_migrations_{saved}")), n(*from)),
                    LExpr::Function(func),
                ));
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) fn captures(program: &selu_syntax::Program) -> LBlock {
    let has_saved = program.stmts.iter().any(|s| {
        let s = if let StmtKind::Export(inner) = &s.kind {
            inner.as_ref()
        } else {
            s
        };
        matches!(s.kind, StmtKind::Saved { .. })
    });
    if !has_saved {
        return vec![];
    }
    let mut out = Vec::new();
    for global in [
        "_G",
        "type",
        "pairs",
        "ipairs",
        "CreateFrame",
        "rawget",
        "pcall",
        "print",
        "error",
    ] {
        out.push(local(&format!("__selu_saved_{global}"), name(global)));
    }
    out.push(local("__selu_saved_format", f(name("string"), "format")));
    out.push(local("__selu_saved_byte", f(name("string"), "byte")));
    out
}
