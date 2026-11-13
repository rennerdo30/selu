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
        local("wow", f(name("__selu_ns"), "__selu_wow")),
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
    vec![
        assign(rt(), LExpr::Table(vec![])),
        assign(f(name("__selu_ns"), "__selu_saved"), rt()),
        assign(f(rt(), "copy"), fun(&["value"], copy_body)),
        assign(
            f(rt(), "fill"),
            fun(&["data", "defaults", "objects"], fill_body),
        ),
        assign(f(rt(), "warn"), fun(&["message"], warn_body)),
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
fn objects(defaults: &selu_syntax::Expr) -> LExpr {
    let fields = if let selu_syntax::ExprKind::Object(fs) = &defaults.kind {
        fs.iter()
            .filter(|f| matches!(f.value.kind, selu_syntax::ExprKind::Object(_)))
            .map(|f| LField::Named(f.key.clone(), objects(&f.value)))
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
                version_key,
                legacy,
                ..
            } => {
                if !forward {
                    out.push(s(LStmtKind::Local(vec![saved.clone()], vec![])));
                }
                {
                    out.push(assign(name(saved), helper("pending", vec![])));
                    // `defaults()` evaluates the declared defaults again, so
                    // every call returns fresh tables.
                    let mut fresh = vec![];
                    let value = self.expr(defaults, &mut fresh);
                    fresh.push(s(LStmtKind::Return(vec![value])));
                    out.push(assign(f(name(saved), "defaults"), fun(&[], fresh)));
                }
                let key = declaration_global(&self.module.addon_name, saved, *adopted);
                let entry = ix(f(rt(), "entries"), st(&key));
                out.push(assign(entry.clone(), LExpr::Table(vec![])));
                {
                    out.push(assign(
                        ix(f(rt(), "readers"), st(&key)),
                        LExpr::Table(vec![]),
                    ));
                    out.push(assign(
                        ix(f(rt(), "contexts"), st(&key)),
                        LExpr::Table(vec![]),
                    ));
                }
                let mut body = vec![];
                let defaults_value = self.expr(defaults, &mut body);
                {
                    let loader = if version_key.is_some() {
                        "adopt"
                    } else {
                        "ordinary"
                    };
                    let version_key = version_key.as_deref().unwrap_or("__selu_version");
                    let legacy_value = legacy
                        .as_ref()
                        .map_or(LExpr::Nil, |_| call(&format!("legacy_{saved}"), vec![]));
                    let publish = LExpr::Function(LFunction {
                        params: vec!["__selu_saved_result".into(), "__selu_saved_retire".into()],
                        vararg: false,
                        body: {
                            let mut b = vec![];
                            b.push(iff(
                                bin(BinOp::Ne, f(g("result"), "data"), LExpr::Nil),
                                vec![invoke(call(
                                    &format!("publish_{saved}"),
                                    vec![f(g("result"), "data")],
                                ))],
                            ));
                            if legacy.is_some() {
                                b.push(iff(
                                    g("retire"),
                                    vec![invoke(call(&format!("retire_{saved}"), vec![]))],
                                ));
                            }
                            b.push(invoke(helper(
                                "publishAdoption",
                                vec![name(saved), g("result")],
                            )));
                            b
                        },
                    });
                    // Direct, compiler-owned global references; no dynamic global lookup.
                    // Getters resolve outside the shadowing source binding, but read only
                    // inside this loader, after the client's SavedVariables restoration.
                    body.push(invoke(helper(
                        loader,
                        vec![
                            st(&key),
                            call(&format!("root_{saved}"), vec![]),
                            legacy_value,
                            st(version_key),
                            n(*version),
                            defaults_value,
                            schema(
                                self.types
                                    .saved_schemas
                                    .get(&stmt.span.start)
                                    .unwrap_or_else(|| {
                                        self.types.get(defaults.id).expect("checked defaults")
                                    }),
                            ),
                            objects(defaults),
                            entry,
                            ix(f(rt(), "readers"), st(&key)),
                            ix(f(rt(), "contexts"), st(&key)),
                            publish,
                        ],
                    )));
                }
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
                context,
                body,
                ..
            } => {
                // The matching declaration controls whether this key adopts a legacy global.
                let key = self
                    .saved_globals
                    .get(saved)
                    .cloned()
                    .expect("saved declaration checked");
                let mut params = vec![param.clone()];
                params.extend(context.iter().map(|p| p.as_ref().clone()));
                let func = self.function(&params, body);
                if let Some(ty) = self.types.saved_contexts.get(&stmt.span.start) {
                    out.push(assign(
                        ix(ix(f(rt(), "contexts"), st(&key)), n(*from)),
                        schema(ty),
                    ));
                }
                let reader = self.types.saved_readers.get(&stmt.span.start);
                if param.ty.is_some() {
                    out.push(assign(
                        ix(ix(f(rt(), "readers"), st(&key)), n(*from)),
                        schema(reader.expect("checked reader")),
                    ));
                }
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
        "next",
        "ipairs",
        "CreateFrame",
        "rawget",
        "pcall",
        "print",
        "tostring",
    ] {
        out.push(local(global, name(global)));
    }
    out
}
pub(crate) fn setup(program: &Program) -> LBlock {
    if !has_saved(program) {
        return vec![];
    }
    vec![
        local("runtime", f(name("__selu_ns"), "__selu_saved")),
        iff(eq(rt(), LExpr::Nil), runtime()),
    ]
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

fn schema(ty: &selu_check::Type) -> LExpr {
    use selu_check::Type;
    match selu_check::shape_of(ty) {
        Type::Number => st("number"),
        Type::String => st("string"),
        Type::Boolean => st("boolean"),
        Type::Null => st("nil"),
        Type::StringLiteral(v) => LExpr::Table(vec![LField::Named("literal".into(), st(&v))]),
        Type::NumberLiteral(v) => {
            LExpr::Table(vec![LField::Named("literal".into(), LExpr::Number(v))])
        }
        Type::BooleanLiteral(v) => {
            LExpr::Table(vec![LField::Named("literal".into(), LExpr::Bool(v))])
        }
        Type::Optional(t) => LExpr::Table(vec![LField::Named("optional".into(), schema(&t))]),
        Type::Union(ts) => LExpr::Table(vec![LField::Named(
            "union".into(),
            LExpr::Table(ts.iter().map(|t| LField::Pos(schema(t))).collect()),
        )]),
        Type::SavedRepair(t) => LExpr::Table(vec![LField::Named("repair".into(), schema(&t))]),
        Type::Array(t) => LExpr::Table(vec![LField::Named("array".into(), schema(&t))]),
        Type::Map(k, v) => LExpr::Table(vec![
            LField::Named("key".into(), schema(&k)),
            LField::Named("value".into(), schema(&v)),
        ]),
        Type::Object(fs) => LExpr::Table(vec![LField::Named(
            "fields".into(),
            LExpr::Table(
                fs.iter()
                    .map(|(k, v)| LField::Keyed(st(k), schema(v)))
                    .collect(),
            ),
        )]),
        _ => st("invalid"),
    }
}

pub(crate) fn adoption_captures(program: &Program, addon: &str) -> LBlock {
    let mut out = if has_saved(program) {
        vec![
            local("rootRawget", name("rawget")),
            local("rootEnvironment", name("_G")),
        ]
    } else {
        vec![]
    };
    for stmt in &program.stmts {
        if let StmtKind::Saved {
            name: saved,
            adopted,
            version_key,
            legacy,
            ..
        } = &inner(stmt).kind
        {
            out.push(local(
                &format!("root_{saved}"),
                fun(
                    &[],
                    vec![ret(if version_key.is_some() {
                        name(&declaration_global(addon, saved, *adopted))
                    } else {
                        call(
                            "rootRawget",
                            vec![
                                g("rootEnvironment"),
                                st(&declaration_global(addon, saved, *adopted)),
                            ],
                        )
                    })],
                ),
            ));
            if let Some(legacy) = legacy {
                out.push(local(
                    &format!("legacy_{saved}"),
                    fun(&[], vec![ret(name(legacy))]),
                ));
                out.push(local(
                    &format!("retire_{saved}"),
                    fun(&[], vec![assign(name(legacy), LExpr::Nil)]),
                ));
            }

            out.push(local(
                &format!("publish_{saved}"),
                fun(
                    &["value"],
                    vec![assign(
                        name(&declaration_global(addon, saved, *adopted)),
                        g("value"),
                    )],
                ),
            ));
        }
    }
    out
}

pub(crate) fn has_adoption(program: &Program) -> bool {
    has_saved(program)
}
pub(crate) fn adoption_setup(program: &Program) -> LBlock {
    if !has_adoption(program) {
        return vec![];
    }
    let mut body = vec![];
    for global in [
        "type", "pairs", "next", "ipairs", "pcall", "tostring", "tonumber",
    ] {
        body.push(local(global, name(global)));
    }
    body.push(crate::raw(include_str!("saved_adoption.lua")));
    body.push(assign(f(rt(), "contexts"), LExpr::Table(vec![])));
    body.push(assign(f(rt(), "readers"), LExpr::Table(vec![])));
    vec![iff(eq(f(rt(), "adopt"), LExpr::Nil), body)]
}
