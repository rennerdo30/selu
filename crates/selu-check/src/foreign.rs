//! Declared other addons (owner-approved design, 2026-11-13; level 2 of the
//! checked boundary, docs/lang/addons.md).
//!
//! `addons/<Name>.selu` declares the part of another addon's public Lua API a
//! project uses:
//!
//! ```selu
//! addon "QuestieDB" {
//!     global LibQuestieDB: { RequireContract: (contract: number) => boolean };
//! }
//! ```
//!
//! and code imports it with `import { LibQuestieDB } from addon "QuestieDB"`.
//! Every value crossing in is checked at runtime against its declaration
//! (the global when read, each field read, each call result and each
//! argument the other addon passes to a Selu callback): a wrong shape gives
//! `null` and one warning, never an unchecked value. Calls into the other
//! addon run protected: an error there gives `null` and one warning, never a
//! Lua error in Selu code. Its values cannot be assigned to Selu types: a
//! table from another addon stays a foreign value whose reads stay checked.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind};

/// What a runtime check of one crossing value accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignShape {
    Unknown,
    Number,
    String,
    Boolean,
    /// A string or number literal type, compared exactly.
    StringLiteral(String),
    NumberLiteral(String),
    Table,
    Function,
    /// A Lua table read as an array: every element checked, then copied
    /// into a fresh Selu table (the other addon can never change it after
    /// the check, and Selu code never writes into the other addon's table).
    Array(Box<ForeignCheck>),
    /// A Lua table read as a map: every key and value checked (raw `next`),
    /// then copied like an array.
    Map(Box<ForeignCheck>, Box<ForeignCheck>),
    Union(Vec<ForeignShape>),
}

/// A crossing value's check: its shape, and whether `nil` is declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignCheck {
    pub shape: ForeignShape,
    pub optional: bool,
}

impl ForeignCheck {
    /// The descriptor the runtime parses (and caches): `?` marks a declared
    /// `nil`, then `u n s b t f`, `a<element>`, `m<key><value>`, `(a|b)`,
    /// `'text'` or `#5`.
    pub fn descriptor(&self) -> String {
        fn shape(s: &ForeignShape, out: &mut String) {
            match s {
                ForeignShape::Unknown => out.push('u'),
                ForeignShape::Number => out.push('n'),
                ForeignShape::String => out.push('s'),
                ForeignShape::Boolean => out.push('b'),
                ForeignShape::Table => out.push('t'),
                ForeignShape::Function => out.push('f'),
                ForeignShape::StringLiteral(text) => {
                    out.push('\'');
                    out.push_str(&text.replace('\\', "\\\\").replace('\'', "\\'"));
                    out.push('\'');
                }
                ForeignShape::NumberLiteral(n) => {
                    out.push('#');
                    out.push_str(n);
                    out.push(';');
                }
                ForeignShape::Array(e) => {
                    out.push('a');
                    out.push_str(&e.descriptor());
                }
                ForeignShape::Map(k, v) => {
                    out.push('m');
                    out.push_str(&k.descriptor());
                    out.push_str(&v.descriptor());
                }
                ForeignShape::Union(alts) => {
                    out.push('(');
                    for (i, a) in alts.iter().enumerate() {
                        if i > 0 {
                            out.push('|');
                        }
                        shape(a, out);
                    }
                    out.push(')');
                }
            }
        }
        let mut out = String::new();
        if self.optional {
            out.push('?');
        }
        shape(&self.shape, &mut out);
        out
    }
}

/// A declared global: read live, while its addon is loaded, and checked.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignGlobal {
    pub addon: String,
    pub global: String,
    pub check: ForeignCheck,
    /// `C_AddOns.IsAddOnLoaded` or `IsAddOnLoaded`.
    pub loaded: String,
    /// A Selu addon's published value (a `.selu-api` import): the API
    /// fingerprint the consumer was built against. A matching provider's
    /// values pass directly; a different or missing one is checked per value.
    pub api: Option<String>,
}

/// What `export addon { ... }` publishes: the `.selu-api` body and its
/// fingerprint, and (local, published) names for the lowering.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeluApi {
    pub body: String,
    pub fingerprint: String,
    pub names: Vec<(String, String)>,
}

/// A checked field read of a foreign table.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignRead {
    /// "QuestieDB LibQuestieDB.Quest" for the warning.
    pub path: String,
    pub check: ForeignCheck,
}

/// A protected call into another addon, with its result checked.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignCall {
    pub path: String,
    /// `object.field(...)`: the function is read from the object in the
    /// same protected step; `None` calls the callee value itself.
    pub field: Option<String>,
    pub result: ForeignCheck,
    /// Arguments that are Selu callbacks the other addon calls: the checks
    /// of the arguments it passes, by argument index.
    pub callbacks: Vec<(usize, Vec<ForeignCheck>)>,
}

const PREFIX: &str = "@foreign:";

/// Whether `t` is a value of another addon (a foreign table or function).
pub fn is_foreign(t: &Type) -> bool {
    matches!(t, Type::Named(_, s) if s.key.starts_with(PREFIX))
}

fn stable_hash(text: &str) -> u64 {
    // FNV-1a: the key must not change between runs or hosts.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The foreign view of a declared type: tables and functions become foreign
/// values (their reads and calls stay checked); arrays, unions and optionals
/// keep their structure around foreign elements.
pub(crate) fn foreign(t: &Type, addon: &str) -> Type {
    match t {
        Type::Named(_, s) if s.key.starts_with(PREFIX) => t.clone(),
        Type::Named(_, s) => match s.get() {
            Some(inner) => foreign(&inner, addon),
            None => t.clone(),
        },
        Type::Object(_) | Type::Function { .. } => {
            let key = format!("{PREFIX}{addon}:{:016x}", stable_hash(&format!("{t:?}")));
            let shape = crate::types::NamedShape::new(key);
            shape.set(t.clone());
            let label = match t {
                Type::Function { .. } => format!("{addon} function"),
                _ => format!("{addon} table"),
            };
            Type::Named(label, shape)
        }
        Type::Optional(inner) => Type::optional(foreign(inner, addon)),
        Type::Array(inner) => Type::Array(Box::new(foreign(inner, addon))),
        Type::Map(k, v) => Type::Map(k.clone(), Box::new(foreign(v, addon))),
        Type::Union(ts) => Type::union(ts.iter().map(|t| foreign(t, addon)).collect()),
        _ => t.clone(),
    }
}

/// The addon a foreign value belongs to.
fn addon_of(t: &Type) -> Option<String> {
    match t {
        Type::Named(_, s) => s
            .key
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.rsplit_once(':'))
            .map(|(addon, _)| addon.to_owned()),
        _ => None,
    }
}

/// The runtime check of a declared type, or the reason it cannot cross.
pub(crate) fn check_of(t: &Type) -> Result<ForeignCheck, String> {
    fn shape(t: &Type, optional: &mut bool) -> Result<ForeignShape, String> {
        Ok(match crate::shape_of(t) {
            Type::Unknown => {
                *optional = true;
                ForeignShape::Unknown
            }
            Type::Number => ForeignShape::Number,
            Type::String => ForeignShape::String,
            Type::Boolean => ForeignShape::Boolean,
            Type::StringLiteral(s) => ForeignShape::StringLiteral(s),
            Type::NumberLiteral(n) => ForeignShape::NumberLiteral(n),
            Type::BooleanLiteral(_) => ForeignShape::Boolean,
            Type::Object(_) => ForeignShape::Table,
            Type::Function { .. } => ForeignShape::Function,
            Type::Null => {
                *optional = true;
                ForeignShape::Unknown
            }
            Type::Optional(inner) => {
                *optional = true;
                shape(&inner, optional)?
            }
            Type::Array(e) => ForeignShape::Array(Box::new(check_of(&e)?)),
            Type::Map(k, v) => {
                let key = check_of(&k)?;
                let keyed = |s: &ForeignShape| {
                    matches!(
                        s,
                        ForeignShape::Number
                            | ForeignShape::String
                            | ForeignShape::StringLiteral(_)
                            | ForeignShape::NumberLiteral(_)
                    )
                };
                let fits = match &key.shape {
                    ForeignShape::Union(alts) => alts.iter().all(keyed),
                    s => keyed(s),
                };
                if !fits || key.optional {
                    return Err(format!(
                        "`{}` cannot cross from another addon: a map's keys are numbers or strings",
                        Type::Map(k.clone(), v.clone())
                    ));
                }
                ForeignShape::Map(Box::new(key), Box::new(check_of(&v)?))
            }
            Type::Union(ts) => {
                let mut alts = Vec::new();
                for t in ts {
                    if matches!(t, Type::Null) {
                        *optional = true;
                        continue;
                    }
                    let s = shape(&t, optional)?;
                    if !alts.contains(&s) {
                        alts.push(s);
                    }
                }
                if alts.len() == 1 {
                    alts.remove(0)
                } else {
                    ForeignShape::Union(alts)
                }
            }
            other => {
                return Err(format!(
                    "`{}` cannot cross from another addon: declare numbers, strings, booleans, literals, arrays, maps, `{{ ... }}` tables, functions or `unknown`",
                    other
                ));
            }
        })
    }
    let mut optional = false;
    let shape = shape(t, &mut optional)?;
    Ok(ForeignCheck { shape, optional })
}

/// Validates a whole declared type, nested fields and function signatures
/// included, so every value that can cross later has a runtime check.
fn validate(t: &Type, out: &mut Vec<String>, depth: usize) {
    if depth > 16 {
        return;
    }
    if let Err(e) = check_of(t) {
        out.push(e);
        return;
    }
    match crate::shape_of(t) {
        Type::Object(fields) => {
            for f in fields.values() {
                validate(f, out, depth + 1);
            }
        }
        Type::Function {
            params, ret, rest, ..
        } => {
            for p in &params {
                validate(p, out, depth + 1);
            }
            if let Some(r) = rest {
                validate(&r, out, depth + 1);
            }
            if !matches!(*ret, Type::Void) {
                validate(&ret, out, depth + 1);
            }
        }
        Type::Optional(inner) | Type::Array(inner) => validate(&inner, out, depth + 1),
        Type::Map(k, v) => {
            validate(&k, out, depth + 1);
            validate(&v, out, depth + 1);
        }
        Type::Union(ts) => {
            for t in ts {
                validate(&t, out, depth + 1);
            }
        }
        _ => {}
    }
}

impl Ctx<'_> {
    /// `addon "Name" { ... }`: only in its own declaration file, which holds
    /// nothing else; every global becomes a live, checked export.
    pub(crate) fn addon_decl(&mut self, s: &Stmt) {
        let StmtKind::AddonDecl {
            name,
            name_span,
            fingerprint,
            globals,
            types,
        } = &s.kind
        else {
            return;
        };
        let expected = s
            .span
            .file
            .strip_prefix(selu_syntax::ADDON_IMPORT_PREFIX)
            .and_then(|f| f.strip_suffix(".selu"))
            .map(str::to_owned);
        match expected {
            None => {
                self.out.push(
                    err(
                        "SELU0710",
                        "an `addon \"Name\" { ... }` declaration belongs in addons/<Name>.selu",
                        &s.span,
                    )
                    .with_help("move it to the project's addons folder and import it with `import { X } from addon \"Name\"`"),
                );
                return;
            }
            Some(file) if file != *name => {
                self.out.push(err(
                    "SELU0710",
                    format!("addons/{file}.selu declares the addon `{file}`, not `{name}`"),
                    name_span,
                ));
                return;
            }
            _ => {}
        }
        if name.starts_with("Blizzard_") {
            self.out.push(err(
                "SELU0702",
                format!("`{name}` is part of the game; its values are typed game data, not another addon"),
                name_span,
            ));
            return;
        }
        let Some(loaded) = self.addon_load_check(&s.span) else {
            return;
        };
        // The block's own types, hoisted like top-level aliases.
        self.predeclare_aliases(types);
        for t in types {
            self.stmt(t);
        }
        // Each declared type is importable as its foreign view, so Selu code
        // can name it (`import { Quest } from addon "Name"`) without ever
        // turning the other addon's values into a Selu type of that shape.
        for t in types {
            let StmtKind::TypeDecl(d) = &t.kind else {
                continue;
            };
            // A global of the same name keeps the name for its value.
            if !d.params.is_empty() || globals.iter().any(|(g, _, _)| *g == d.name) {
                continue;
            }
            let named = selu_syntax::TypeExpr {
                kind: selu_syntax::TypeExprKind::Named {
                    name: d.name.clone(),
                    args: vec![],
                },
                span: d.name_span.clone(),
            };
            let declared = self.alias_annotation(&named);
            if format!("{declared:?}").contains("Error") || check_of(&declared).is_err() {
                continue;
            }
            let e = self.module.exports.entry(d.name.clone()).or_default();
            e.has_value = false;
            e.foreign_type = Some(foreign(&declared, name));
        }
        for (global, span, ty) in globals {
            // A Selu addon's published values are no globals.
            if fingerprint.is_none() && self.game_knows(global) {
                self.out.push(
                    err(
                        "SELU0702",
                        format!("`{global}` belongs to the game; the boundary is only for other addons"),
                        span,
                    )
                    .with_help("use it directly: game values are typed"),
                );
                continue;
            }
            let Some(declared) = self.annotation(&Some(ty.clone())) else {
                continue;
            };
            if format!("{declared:?}").contains("Error") {
                // Already reported (an unknown type name).
                continue;
            }
            let mut problems = Vec::new();
            validate(&declared, &mut problems, 0);
            if let Some(problem) = problems.first() {
                self.out.push(err("SELU0711", problem.clone(), span));
                continue;
            }
            let check = check_of(&declared).expect("validated");
            self.types.mark_foreign_global(ForeignGlobal {
                addon: name.clone(),
                global: global.clone(),
                check,
                loaded: loaded.clone(),
                api: fingerprint.clone(),
            });
            let e = self.module.exports.entry(global.clone()).or_default();
            e.has_value = true;
            // A value that is absent or of the wrong shape reads as null.
            e.ty = Type::optional(foreign(&declared, name));
            e.mutable = true;
        }
    }

    /// A declared addon's module may hold only its declaration.
    pub(crate) fn addon_module_rules(&mut self, stmts: &[Stmt]) {
        let Some(first) = stmts.first() else {
            return;
        };
        if !first.span.file.starts_with(selu_syntax::ADDON_IMPORT_PREFIX) {
            return;
        }
        let decls = stmts
            .iter()
            .filter(|s| matches!(s.kind, StmtKind::AddonDecl { .. }))
            .count();
        for s in stmts {
            if !matches!(s.kind, StmtKind::AddonDecl { .. }) || decls > 1 {
                self.out.push(
                    err(
                        "SELU0710",
                        "a declared addon's file holds exactly one `addon \"Name\" { ... }` declaration",
                        &s.span,
                    )
                    .with_help("put Selu code in src/; this file only describes the other addon's API"),
                );
                return;
            }
        }
    }

    /// `object.field` on a foreign table: a checked read.
    pub(crate) fn foreign_member(&mut self, t: &Type, name: &str, e: &Expr) -> Option<Type> {
        if !is_foreign(t) {
            return None;
        }
        let addon = addon_of(t).unwrap_or_default();
        let Type::Object(fields) = crate::shape_of(t) else {
            self.out.push(err(
                "SELU0712",
                format!("a function of {addon} has no fields"),
                &e.span,
            ));
            return Some(Type::Error);
        };
        let Some(field) = fields.get(name) else {
            self.out.push(
                err(
                    "SELU0712",
                    format!("`{name}` is not declared for this {addon} table"),
                    &e.span,
                )
                .with_help(format!(
                    "declare it in addons/{addon}.selu; declared fields: {}",
                    fields.keys().cloned().collect::<Vec<_>>().join(", ")
                )),
            );
            return Some(Type::Error);
        };
        let check = check_of(field).expect("validated declaration");
        self.types.mark_foreign_read(
            e.id,
            ForeignRead {
                path: format!("{addon} {}", path_text(e)),
                check,
            },
        );
        Some(Type::optional(foreign(field, &addon)))
    }

    /// `object.field(...)` on a foreign table: the field is read and called
    /// in one protected step, its result checked.
    pub(crate) fn foreign_member_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        args: &[Expr],
        receiver: &Type,
        name: &str,
    ) -> Option<Type> {
        // The receiver as recorded, before shapes are unwrapped.
        let recorded = match &callee.kind {
            ExprKind::Member { object, .. } => self.types.get(object.id).cloned(),
            _ => None,
        };
        let receiver = match recorded {
            Some(t) if is_foreign(&t) => t,
            _ if is_foreign(receiver) => receiver.clone(),
            _ => return None,
        };
        let receiver = &receiver;
        let addon = addon_of(receiver).unwrap_or_default();
        let Type::Object(fields) = crate::shape_of(receiver) else {
            self.out.push(err(
                "SELU0712",
                format!("a function of {addon} has no fields"),
                &callee.span,
            ));
            return Some(Type::Error);
        };
        let Some(field) = fields.get(name) else {
            self.out.push(
                err(
                    "SELU0712",
                    format!("`{name}` is not declared for this {addon} table"),
                    &callee.span,
                )
                .with_help(format!("declare it in addons/{addon}.selu")),
            );
            for a in args {
                self.value(a);
            }
            return Some(Type::Error);
        };
        let fn_type = foreign(&crate::shape_of(field), &addon);
        self.types.set(callee.id, fn_type.clone());
        self.checked_foreign_call(e, callee, args, fn_type, Some(name.to_owned()))
    }

    /// A call whose callee value is a foreign function (a variable or an
    /// already narrowed field).
    pub(crate) fn foreign_direct_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        args: &[Expr],
        callee_type: &Type,
    ) -> Option<Type> {
        if !is_foreign(callee_type) {
            return None;
        }
        self.checked_foreign_call(e, callee, args, callee_type.clone(), None)
    }

    fn checked_foreign_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        args: &[Expr],
        fn_type: Type,
        field: Option<String>,
    ) -> Option<Type> {
        let addon = addon_of(&fn_type).unwrap_or_default();
        let path = format!("{addon} {}", path_text(callee));
        let fn_shape = crate::shape_of(&fn_type);
        let (params, ret, required, rest) = match &fn_shape {
            Type::Function {
                params,
                ret,
                required,
                rest,
                ..
            } => (params.clone(), (**ret).clone(), *required, rest.clone()),
            Type::Optional(_) | Type::Union(_) => {
                self.out.push(
                    err("SELU0331", format!("this {addon} value may be null here, so calling it could fail"), &callee.span)
                        .with_help("check it first: `if (f != null) { ... }`"),
                );
                return Some(Type::Error);
            }
            _ => {
                self.out.push(err(
                    "SELU0712",
                    format!("this {addon} value is not declared as a function"),
                    &callee.span,
                ));
                return Some(Type::Error);
            }
        };
        if args.len() < required || (rest.is_none() && args.len() > params.len()) {
            self.out.push(err(
                "SELU0335",
                format!(
                    "{path} takes {} argument(s) as declared, but this call passes {}",
                    if rest.is_some() { format!("{required} or more") } else { params.len().to_string() },
                    args.len()
                ),
                &e.span,
            ));
        }
        let mut callbacks = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let want = params.get(i).cloned().or_else(|| rest.as_deref().cloned());
            let Some(want) = want else {
                self.value(arg);
                continue;
            };
            // A Selu function the other addon calls: each argument it passes
            // is checked and null when it does not match, so the callback
            // receives optional (and foreign) parameters.
            let want = match crate::shape_of(&want) {
                Type::Function {
                    multiple_results,
                    unspecified_callback,
                    api,
                    api_semantics,
                    params,
                    ret,
                    required,
                    rest,
                } => Type::Function {
                    multiple_results,
                    unspecified_callback,
                    api,
                    api_semantics,
                    params: params.iter().map(|p| Type::optional(foreign(p, &addon))).collect(),
                    ret,
                    required,
                    rest,
                },
                _ => want,
            };
            // A foreign value passed back has the foreign view of the type.
            let expected = foreign(&want, &addon);
            let actual = self.value_expected(arg, Some(&want));
            if !actual.assignable_to(&want) && !actual.assignable_to(&expected) {
                self.out.push(err(
                    "SELU0335",
                    format!(
                        "argument {} of {path} must be a `{}`, but this is a `{}`",
                        i + 1,
                        want,
                        actual
                    ),
                    &arg.span,
                ));
            }
            // Its arguments cross in (checked against the declaration).
            if let Some(Type::Function { params: cb, .. }) = params.get(i).or(rest.as_deref()).map(crate::shape_of) {
                let checks = cb
                    .iter()
                    .map(|p| check_of(p).expect("validated declaration"))
                    .collect();
                callbacks.push((i, checks));
            }
        }
        let result = if matches!(ret, Type::Void) {
            Type::Void
        } else {
            Type::optional(foreign(&ret, &addon))
        };
        let check = if matches!(ret, Type::Void) {
            ForeignCheck {
                shape: ForeignShape::Unknown,
                optional: true,
            }
        } else {
            check_of(&ret).expect("validated declaration")
        };
        self.types.mark_foreign_call(
            e.id,
            ForeignCall {
                path,
                field,
                result: check,
                callbacks,
            },
        );
        Some(result)
    }

    /// The load check every selected target has.
    pub(crate) fn addon_load_check(&mut self, span: &selu_diagnostics::Span) -> Option<String> {
        let manifests = self.availability.manifests();
        if manifests
            .iter()
            .all(|m| m.function("C_AddOns.IsAddOnLoaded").is_some())
        {
            return Some("C_AddOns.IsAddOnLoaded".into());
        }
        if manifests.iter().all(|m| m.function("IsAddOnLoaded").is_some()) {
            return Some("IsAddOnLoaded".into());
        }
        self.out.push(err(
            "SELU0703",
            "no addon load check exists on every selected target",
            span,
        ));
        None
    }
}

impl Ctx<'_> {
    /// `export addon { a, b as c };`: the values this addon publishes to
    /// other Selu addons, with their types written out for `.selu-api`.
    pub(crate) fn export_addon(&mut self, s: &Stmt) {
        let StmtKind::ExportAddon { names } = &s.kind else {
            return;
        };
        if self.scopes.len() != 1 {
            self.out.push(err(
                "SELU0714",
                "`export addon` belongs at the top level of a module",
                &s.span,
            ));
            return;
        }
        if self.types.selu_api().is_some() {
            self.out.push(err(
                "SELU0714",
                "an addon publishes one `export addon { ... }` list",
                &s.span,
            ));
            return;
        }
        let mut api = SeluApi::default();
        let mut body = String::new();
        let mut published = std::collections::HashSet::new();
        for (local, name, span) in names {
            let Some(binding) = self.lookup(local).map(|(_, b)| b.clone()) else {
                self.out.push(err("SELU0310", format!("`{local}` is not declared"), span));
                continue;
            };
            if !matches!(
                binding.kind,
                crate::Kind::Const | crate::Kind::Function | crate::Kind::Import
            ) {
                self.out.push(
                    err(
                        "SELU0714",
                        format!("`{local}` can change; an addon publishes constants and functions"),
                        span,
                    )
                    .with_help("publish a function that returns the current value"),
                );
                continue;
            }
            if !published.insert(name.clone()) {
                self.out.push(err("SELU0714", format!("`{name}` is published twice"), span));
                continue;
            }
            let ty = self.type_of(local).unwrap_or(Type::Error);
            let mut problems = Vec::new();
            validate(&ty, &mut problems, 0);
            let text = match (problems.first().cloned(), type_text(&ty, 0)) {
                (None, Ok(text)) => text,
                (Some(problem), _) | (None, Err(problem)) => {
                    self.out.push(
                        err("SELU0711", format!("`{local}` cannot be published: {problem}"), span)
                            .with_help("publish plain data and functions; classes and game objects stay inside the addon"),
                    );
                    continue;
                }
            };
            body.push_str(&format!("    global {name}: {text};\n"));
            api.names.push((local.clone(), name.clone()));
        }
        api.fingerprint = format!("{:016x}", stable_hash(&body));
        api.body = body;
        self.types.set_selu_api(api);
    }
}

/// A type as Selu source, for a `.selu-api` file: aliases are written out
/// structurally, so the file needs no other declarations.
pub(crate) fn type_text(t: &Type, depth: usize) -> Result<String, String> {
    if depth > 24 {
        return Err("the type is too deeply nested".into());
    }
    let wrapped = |t: &Type| -> Result<String, String> {
        let text = type_text(t, depth + 1)?;
        Ok(match crate::shape_of(t) {
            Type::Function { .. } | Type::Union(_) => format!("({text})"),
            _ => text,
        })
    };
    Ok(match crate::shape_of(t) {
        Type::Number => "number".into(),
        Type::String => "string".into(),
        Type::Boolean => "boolean".into(),
        Type::Unknown => "unknown".into(),
        Type::Null => "null".into(),
        Type::Void => "void".into(),
        Type::StringLiteral(s) => serde_json::to_string(&s).unwrap_or_default(),
        Type::NumberLiteral(n) => n,
        Type::BooleanLiteral(b) => b.to_string(),
        Type::Optional(inner) => format!("{}?", wrapped(&inner)?),
        Type::Array(inner) => format!("{}[]", wrapped(&inner)?),
        Type::Map(k, v) => format!(
            "Map<{}, {}>",
            type_text(&k, depth + 1)?,
            type_text(&v, depth + 1)?
        ),
        Type::Union(ts) => ts
            .iter()
            .map(wrapped)
            .collect::<Result<Vec<_>, _>>()?
            .join(" | "),
        Type::Object(fields) => {
            let mut parts = Vec::new();
            for (k, v) in &fields {
                match crate::shape_of(v) {
                    Type::Optional(inner) => {
                        parts.push(format!("{k}?: {}", type_text(&inner, depth + 1)?))
                    }
                    _ => parts.push(format!("{k}: {}", type_text(v, depth + 1)?)),
                }
            }
            format!("{{ {} }}", parts.join("; "))
        }
        Type::Function {
            params,
            ret,
            required,
            rest,
            ..
        } => {
            let mut parts = Vec::new();
            for (i, p) in params.iter().enumerate() {
                let optional = if i >= required { "?" } else { "" };
                parts.push(format!("p{}{optional}: {}", i + 1, type_text(p, depth + 1)?));
            }
            if let Some(r) = rest {
                parts.push(format!("...rest: {}[]", wrapped(&r)?));
            }
            format!("({}) => {}", parts.join(", "), type_text(&ret, depth + 1)?)
        }
        other => return Err(format!("`{other}` is not plain data")),
    })
}

/// `a.b.c` as written, for warnings.
fn path_text(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Member { object, name } => format!("{}.{name}", path_text(object)),
        ExprKind::Call { callee, .. } => format!("{}()", path_text(callee)),
        _ => "value".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_are_compact_and_escaped() {
        let check = ForeignCheck {
            shape: ForeignShape::Array(Box::new(ForeignCheck {
                shape: ForeignShape::Union(vec![
                    ForeignShape::StringLiteral("it's".into()),
                    ForeignShape::NumberLiteral("5".into()),
                ]),
                optional: false,
            })),
            optional: true,
        };
        assert_eq!(check.descriptor(), "?a('it\\'s'|#5;)");
    }
}
