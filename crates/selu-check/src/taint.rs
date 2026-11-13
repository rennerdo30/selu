//! Conservative source-origin proof, following lower::purity: writes and duplicate
//! bindings invalidate proof. Types alone never prove ownership.
use crate::{Type, TypeTable};
use selu_diagnostics::Diagnostic;
#[cfg(test)]
use selu_diagnostics::Severity;
use selu_syntax::{
    Expr, ExprKind, Program, StmtKind,
    visit::{self, Child},
};
use std::collections::{HashMap, HashSet};

/// All products have secure UI. No product default or fallback is used.
/// Incidents supplied by the owner in SL-34, observed on Forever.
pub const DENIED_APIS: &[(&str, &str, &str)] = &[
    (
        "C_Bank.FetchPurchasedBankTabData",
        "cached table refilled under caller taint",
        "C_Bank.FetchNumPurchasedBankTabs",
    ),
    (
        "C_Bank.FetchNextPurchasableBankTabData",
        "cached table refilled under caller taint",
        "C_Bank.FetchNumPurchasedBankTabs",
    ),
    (
        "C_Bank.FetchPurchasedBankTabIDs",
        "cached table refilled under caller taint",
        "C_Bank.FetchNumPurchasedBankTabs",
    ),
    (
        "C_Bank.PurchaseBankTab",
        "protected bank purchase",
        "secure click proxy for Blizzard's purchase button",
    ),
    (
        "PlayerSpellsUtil.OpenToClassTalentsTab",
        "tainted opening blocked MainActionBar:SetPointBase in combat",
        "secure click proxy for the micro button",
    ),
    (
        "PlayerSpellsUtil.OpenToSpellBookTab",
        "tainted opening blocked MainActionBar:SetPointBase in combat",
        "secure click proxy for the micro button",
    ),
    (
        "PlayerSpellsUtil.OpenToSpellBookTabAtSpell",
        "tainted opening blocked MainActionBar:SetPointBase in combat",
        "secure click proxy for the micro button",
    ),
    (
        "TogglePlayerSpellsFrame",
        "tainted opening blocked MainActionBar:SetPointBase in combat",
        "secure click proxy for the micro button",
    ),
    (
        "ToggleSpellBook",
        "tainted opening blocked MainActionBar:SetPointBase in combat",
        "secure click proxy for the micro button",
    ),
    (
        "ToggleWorldMap",
        "opens the map under caller taint",
        "C_Map.OpenWorldMap(mapID)",
    ),
    (
        "OpenWorldMap",
        "opens the map under caller taint",
        "C_Map.OpenWorldMap(mapID)",
    ),
];
/// CallbackRegistry TriggerEvent uses secureexecuterange (client CallbackRegistry.lua:200-204).
/// Only the named EventRegistry registry is proven here; a method name alone is insufficient.
const SAFE_REGISTRIES: &[&str] = &[
    "CallbackRegistryMixin.RegisterCallback",
    "EventRegistry.RegisterCallback",
    "EventRegistry.RegisterFrameEventAndCallback",
];

#[derive(Default)]
struct Origins {
    values: HashMap<String, Expr>,
    counts: HashMap<String, usize>,
    writes: HashSet<String>,
    fields: HashMap<String, Expr>,
    duplicate_fields: HashSet<String>,
    constructor_stores: HashSet<selu_syntax::ExprId>,
    resolved_values: HashMap<crate::access_paths::AccessPath, Vec<Expr>>,
    resolved_writes: HashSet<crate::access_paths::AccessPath>,
}
fn walk<'a>(c: Child<'a>, f: &mut impl FnMut(Child<'a>)) {
    match c {
        Child::Stmt(s) => visit::child_stmts_and_exprs(s, f),
        Child::Expr(e) => visit::child_exprs_and_stmts(e, f),
    }
}
impl Origins {
    fn resolve(&mut self, c: Child<'_>, types: &TypeTable) {
        if let Child::Expr(e) = &c {
            match &e.kind {
                ExprKind::Assign { target, value } => {
                    if let Some(path) = types.origin_path(target) {
                        if path.steps.is_empty() {
                            self.resolved_writes.insert(path);
                        } else {
                            self.resolved_values
                                .entry(path)
                                .or_default()
                                .push(*value.clone());
                        }
                    }
                }
                ExprKind::CompoundAssign { target, .. } | ExprKind::Update { target, .. } => {
                    if let Some(path) = types.origin_path(target) {
                        self.resolved_writes.insert(path);
                    }
                }
                _ => {}
            }
        }
        walk(c, &mut |c| self.resolve(c, types));
    }
    fn owned_checked(&self, e: &Expr, types: &TypeTable, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        if let ExprKind::Call { callee, .. } = &e.kind
            && matches!(types.get(callee.id), Some(Type::Function { api_semantics, .. }) if api_semantics.frame_factory || api_semantics.plain_result)
        {
            return true;
        }
        if let Some(path) = types.origin_path(e) {
            if self.resolved_writes.contains(&path) {
                return false;
            }
            return self.resolved_values.get(&path).is_some_and(|values| {
                values.iter().any(|v| !matches!(v.kind, ExprKind::Null))
                    && values.iter().all(|v| {
                        matches!(v.kind, ExprKind::Null) || self.owned_checked(v, types, depth + 1)
                    })
            });
        }
        // Unchecked unit probes retain the spelling-based purity proof. Checked
        // bindings always use the checker's existing declaration/access identities.
        self.owned(e, depth)
    }
    fn api_name(&self, e: &Expr, depth: usize) -> Option<String> {
        if depth > 32 {
            return None;
        }
        match &e.kind {
            ExprKind::Ident(n) if self.counts.get(n) == Some(&1) && !self.writes.contains(n) => {
                self.api_name(self.values.get(n)?, depth + 1)
            }
            ExprKind::Ident(n) if !self.counts.contains_key(n) && !self.writes.contains(n) => {
                Some(n.clone())
            }
            ExprKind::Member { object, name } => {
                Some(format!("{}.{}", self.api_name(object, depth + 1)?, name))
            }
            _ => None,
        }
    }

    fn pattern(&mut self, p: &selu_syntax::Pattern) {
        match p {
            selu_syntax::Pattern::Array(items) => {
                for (n, _) in items.iter().flatten() {
                    self.writes.insert(n.clone());
                }
            }
            selu_syntax::Pattern::Object(items) => {
                for (_, n, _, _) in items {
                    self.writes.insert(n.clone());
                }
            }
        }
    }

    fn collect(&mut self, c: Child<'_>) {
        match &c {
            Child::Stmt(s) => match &s.kind {
                StmtKind::VarDecl { name, init, .. } => {
                    *self.counts.entry(name.clone()).or_default() += 1;
                    if let Some(e) = init {
                        self.values.insert(name.clone(), *e.clone());
                    }
                }
                StmtKind::FuncDecl { params, .. } | StmtKind::OnHandler { params, .. } => {
                    for p in params {
                        self.writes.insert(p.name.clone());
                    }
                }
                StmtKind::Destructure { pattern, .. } => self.pattern(pattern),
                StmtKind::ForOf { binding, .. } => match binding {
                    selu_syntax::ForBinding::Name(n, _) => {
                        self.writes.insert(n.clone());
                    }
                    selu_syntax::ForBinding::Pattern(p) => self.pattern(p),
                },
                StmtKind::Try {
                    catch: Some((Some(p), _)),
                    ..
                } => {
                    self.writes.insert(p.name.clone());
                }
                StmtKind::Migrate { param, context, .. } => {
                    self.writes.insert(param.name.clone());
                    if let Some(p) = context {
                        self.writes.insert(p.name.clone());
                    }
                }
                StmtKind::Import { names, default, .. } => {
                    for (_, n, _, _) in names {
                        self.writes.insert(n.clone());
                    }
                    if let Some((n, _)) = default {
                        self.writes.insert(n.clone());
                    }
                }
                StmtKind::Class(c) => {
                    self.writes.insert(c.name.clone());
                    for m in &c.members {
                        if let selu_syntax::ClassMemberKind::Constructor { body, .. } = &m.kind {
                            for s in body {
                                if let StmtKind::Expr(e) = &s.kind
                                    && let ExprKind::Assign { target, value } = &e.kind
                                    && let ExprKind::Member { object, name } = &target.kind
                                    && matches!(object.kind, ExprKind::This)
                                {
                                    if self.fields.insert(name.clone(), *value.clone()).is_some() {
                                        self.duplicate_fields.insert(name.clone());
                                    }
                                    self.constructor_stores.insert(e.id);
                                }
                            }
                        }
                        match &m.kind {
                            selu_syntax::ClassMemberKind::Field {
                                name,
                                init: Some(e),
                                ..
                            } => {
                                if self.fields.insert(name.clone(), *e.clone()).is_some() {
                                    self.duplicate_fields.insert(name.clone());
                                }
                            }
                            selu_syntax::ClassMemberKind::Method { params, .. }
                            | selu_syntax::ClassMemberKind::Constructor { params, .. } => {
                                for p in params {
                                    self.writes.insert(p.name.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Child::Expr(e) => match &e.kind {
                ExprKind::Function { params, .. } => {
                    for p in params {
                        self.writes.insert(p.name.clone());
                    }
                }
                ExprKind::Assign { target, .. }
                | ExprKind::CompoundAssign { target, .. }
                | ExprKind::Update { target, .. } => {
                    if let Some(n) = target.dotted_name() {
                        self.writes.insert(n);
                    }
                    if matches!(target.kind, ExprKind::Index { .. }) {
                        self.duplicate_fields.insert("*".into());
                    }
                    if let ExprKind::Member { name, .. } = &target.kind
                        && !self.constructor_stores.contains(&e.id)
                    {
                        self.duplicate_fields.insert(name.clone());
                    }
                }
                _ => {}
            },
        }
        if let Child::Stmt(s) = &c
            && let StmtKind::FuncDecl { name, .. } | StmtKind::Global { name, .. } = &s.kind
        {
            self.writes.insert(name.clone());
        }
        walk(c, &mut |c| self.collect(c));
    }
    /// Proven to come from the game: a manifest global that no Selu binding
    /// shadows, a game API's result that is not a factory, or a field, index
    /// or method result of a proven game value. Factory returns, parameters
    /// and imports are unknown, not foreign.
    fn proven_foreign(&self, e: &Expr, types: &TypeTable, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        // Checked bindings and class fields: every store must be a game value.
        if let Some(path) = types.origin_path(e) {
            if self.resolved_writes.contains(&path) {
                return false;
            }
            if let Some(values) = self.resolved_values.get(&path) {
                return values.iter().any(|v| !matches!(v.kind, ExprKind::Null))
                    && values.iter().all(|v| {
                        matches!(v.kind, ExprKind::Null) || self.proven_foreign(v, types, depth + 1)
                    });
            }
        }
        match &e.kind {
            ExprKind::Ident(n) => {
                if self.counts.get(n) == Some(&1) && !self.writes.contains(n) {
                    return self
                        .values
                        .get(n)
                        .is_some_and(|v| self.proven_foreign(v, types, depth + 1));
                }
                !self.counts.contains_key(n) && !self.writes.contains(n)
            }
            // A class field whose only store is a proven game value.
            ExprKind::Member { object, name }
                if matches!(object.kind, ExprKind::This)
                    && !self.duplicate_fields.contains(name)
                    && !self.duplicate_fields.contains("*") =>
            {
                self.fields
                    .get(name)
                    .is_some_and(|v| self.proven_foreign(v, types, depth + 1))
            }
            ExprKind::Member { object, .. } | ExprKind::Index { object, .. } => {
                self.proven_foreign(object, types, depth + 1)
            }
            ExprKind::Call { callee, .. } => {
                let api = matches!(
                    types.get(callee.id),
                    Some(Type::Function { api, api_semantics, .. })
                        if api.0.is_some() && !api_semantics.frame_factory
                );
                match &callee.kind {
                    // A method on a game object returns more of the game.
                    ExprKind::Member { object, .. } => {
                        api && self.proven_foreign(object, types, depth + 1)
                    }
                    // A game function returning a game object (GetMouseFocus, ...).
                    _ => api && self.api_name(callee, 0).is_some(),
                }
            }
            _ => false,
        }
    }
    fn owned(&self, e: &Expr, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        match &e.kind {
            ExprKind::Ident(n) if self.counts.get(n) == Some(&1) && !self.writes.contains(n) => {
                self.values.get(n).is_some_and(|v| self.owned(v, depth + 1))
            }
            ExprKind::Call { callee, .. } => {
                callee.dotted_name().as_deref() == Some("CreateFrame")
                    && !self.counts.contains_key("CreateFrame")
                    && !self.writes.contains("CreateFrame")
            }
            ExprKind::Member { object, name }
                if !self.duplicate_fields.contains(name)
                    && !self.duplicate_fields.contains("*") =>
            {
                if matches!(object.kind, ExprKind::This) {
                    self.fields
                        .get(name)
                        .is_some_and(|v| self.owned(v, depth + 1))
                } else if let ExprKind::Ident(n) = &object.kind {
                    if self.counts.get(n) != Some(&1) || self.writes.contains(n) {
                        return false;
                    }
                    match self.values.get(n).map(|v| &v.kind) {
                        Some(ExprKind::Object(fields)) => fields
                            .iter()
                            .find(|f| f.key == *name)
                            .is_some_and(|f| self.owned(&f.value, depth + 1)),
                        Some(ExprKind::New { .. }) => self
                            .fields
                            .get(name)
                            .is_some_and(|v| self.owned(v, depth + 1)),
                        _ => false,
                    }
                } else {
                    false
                }
            }
            ExprKind::Object(_) | ExprKind::Array(_) | ExprKind::New { .. } => true,
            _ => false,
        }
    }
}
/// Only manifest game types form a foreign boundary. A Selu object that
/// contains game values is not itself a game object; inspect only union arms,
/// optional wrappers and aliases, never fields or collection element types.
pub(crate) fn is_game_type(t: &Type) -> bool {
    fn visit(t: &Type, seen: &mut HashSet<String>) -> bool {
        match t {
            Type::Widget(..) => true,
            Type::ApiOpaque(_, members) => members.is_empty(),
            Type::Optional(t) | Type::Secret(t) => visit(t, seen),
            Type::Union(ts) => ts.iter().any(|t| visit(t, seen)),
            Type::Named(_, shape) if seen.insert(shape.key.clone()) => {
                shape.key.starts_with("@manifest:")
                    || shape.key.starts_with("@selu_closed:")
                    || shape.get().is_some_and(|t| visit(&t, seen))
            }
            _ => false,
        }
    }
    visit(t, &mut HashSet::new())
}
fn safe_registry(callee: &Expr, types: &TypeTable) -> bool {
    // A Lua table operation (`table.sort`) runs its callback at once, inside
    // the addon's own call; nothing is installed anywhere.
    matches!(types.get(callee.id), Some(Type::Function { api, api_semantics, .. }) if api_semantics.table_operation.is_some() || api_semantics.pure_callback || api.0.as_ref().is_some_and(|s| s.name.starts_with("C_Timer.") || SAFE_REGISTRIES.contains(&s.name.as_str())))
}
/// A game receiver this check reports. By default only values proven to come
/// from the game; `strict` also reports values of unknown origin (anything
/// not proven owned), such as frames returned by the addon's own factories.
fn foreign(e: &Expr, o: &Origins, types: &TypeTable, strict: bool) -> bool {
    !matches!(e.kind, ExprKind::This)
        && types.is_game_receiver(e.id)
        && !o.owned_checked(e, types, 0)
        && (strict || o.proven_foreign(e, types, 0))
}
/// Run on checked syntax; project entries augment the shipped list.
pub fn check(
    program: &Program,
    types: &TypeTable,
    strict: bool,
    deny: &[(String, String)],
) -> Vec<Diagnostic> {
    let mut origins = Origins::default();
    for s in &program.stmts {
        origins.collect(Child::Stmt(s));
    }
    for (path, e) in &types.origin_initializers {
        origins
            .resolved_values
            .entry(path.clone())
            .or_default()
            .push(e.clone());
    }
    for s in &program.stmts {
        origins.resolve(Child::Stmt(s), types);
    }
    let mut out = Vec::new();
    fn scan(
        c: Child<'_>,
        o: &Origins,
        types: &TypeTable,
        strict: bool,
        deny: &[(String, String)],
        out: &mut Vec<Diagnostic>,
    ) {
        if let Child::Expr(e) = &c {
            let mut emit = |code: &str, message: String, help: &str| {
                let d = if strict || code == "SELU0606" || code == "SELU0607" {
                    Diagnostic::error(code, message)
                } else {
                    Diagnostic::warning(code, message)
                };
                out.push(d.with_span(e.span.clone()).with_help(help));
            };
            match &e.kind {
                ExprKind::Call { callee, args, .. } => {
                    let n = callee.dotted_name().unwrap_or_default();
                    if n == "allowTaint" && types.is_builtin_call(callee.id) {
                        if !matches!(args.first().map(|e| &e.kind), Some(ExprKind::Str(s)) if !s.trim().is_empty())
                        {
                            emit(
                                "SELU0607",
                                "Taint suppression requires a non-empty literal reason".into(),
                                "allowTaint(\"reason\", expression)",
                            );
                        } else if args.len() == 2 {
                            // Suppress only the wrapped occurrence, not nested calls.
                            walk(Child::Expr(&args[1]), &mut |c| {
                                scan(c, o, types, strict, deny, out)
                            });
                            return;
                        }
                    }
                    let api_name = o.api_name(callee, 0).unwrap_or_default();
                    if let Some((_, reason, alternative)) =
                        DENIED_APIS.iter().find(|(api, _, _)| *api == api_name)
                    {
                        emit("SELU0606", format!("{n}: {reason}"), alternative);
                    }
                    if let Some((_, reason)) = deny.iter().find(|(api, _)| *api == api_name) {
                        emit(
                            "SELU0606",
                            format!("{n}: {reason}"),
                            "use a safe alternative or allowTaint with a written reason",
                        );
                    }
                    if types.is_builtin_call(callee.id)
                        && ((n == "hook"
                            && (args.len() == 2
                                || types.is_mixin_hook(callee.id)
                                || args.first().is_some_and(|e| foreign(e, o, types, strict))))
                            || (n == "hookScript"
                                && args.first().is_some_and(|e| foreign(e, o, types, strict))
                                && matches!(args.get(1).map(|e| &e.kind), Some(ExprKind::Str(s)) if matches!(s.as_str(), "OnShow"|"OnHide"|"OnEnable"|"OnDisable"))))
                    {
                        emit(
                            "SELU0604",
                            "Hook runs inside a foreign call and can taint secure UI".into(),
                            "poll IsShown from an owned ticker or frame",
                        );
                    }
                    if let ExprKind::Member { object, .. } = &callee.kind
                        && foreign(object, o, types, strict)
                        && !safe_registry(callee, types)
                        && args.iter().any(|a| {
                            matches!(
                                types.get(a.id).map(crate::shape_of),
                                Some(Type::Function { .. })
                            ) || matches!(a.kind, ExprKind::Function { .. })
                        })
                    {
                        emit(
                            "SELU0605",
                            "Callback installed into a foreign object may taint its caller".into(),
                            "use an owned frame or a documented safe callback registry",
                        );
                    }
                }
                ExprKind::Assign { target, value } => {
                    let allowed_value = matches!(&value.kind, ExprKind::Call { callee, args } if callee.dotted_name().as_deref() == Some("allowTaint") && types.is_builtin_call(callee.id) && args.len() == 2 && matches!(&args[0].kind, ExprKind::Str(s) if !s.trim().is_empty()));
                    if let ExprKind::Member { object, .. } | ExprKind::Index { object, .. } =
                        &target.kind
                        && foreign(object, o, types, strict)
                        && !allowed_value
                        && (matches!(value.kind, ExprKind::Function { .. })
                            || matches!(
                                types.get(value.id).map(crate::shape_of),
                                Some(Type::Function { .. })
                            ))
                    {
                        emit(
                            "SELU0605",
                            "Function stored in a foreign object may taint its caller".into(),
                            "keep callbacks on owned objects",
                        );
                    }
                }
                _ => {}
            }
        }
        walk(c, &mut |c| scan(c, o, types, strict, deny, out));
    }
    for s in &program.stmts {
        scan(Child::Stmt(s), &origins, types, strict, deny, &mut out);
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn lint(src: &str, strict: bool, deny: &[(String, String)]) -> Vec<Diagnostic> {
        let program = selu_syntax::parse("taint.selu", src).unwrap();
        let mut types = TypeTable::default();
        fn annotate(c: Child<'_>, types: &mut TypeTable) {
            if let Child::Expr(e) = &c {
                if matches!(&e.kind, ExprKind::Ident(n) if matches!(n.as_str(), "foreign"|"UIParent"|"frame"|"f"))
                {
                    types.set(e.id, Type::Widget("Frame".into(), vec![]));
                }
                if let ExprKind::Call { callee, .. } = &e.kind
                    && matches!(
                        callee.dotted_name().as_deref(),
                        Some("hook" | "hookScript" | "allowTaint")
                    )
                {
                    types.mark_builtin_call(callee.id);
                }
            }
            walk(c, &mut |c| annotate(c, types));
        }
        for s in &program.stmts {
            annotate(Child::Stmt(s), &mut types);
        }
        check(&program, &types, strict, deny)
    }
    fn checked_source(src: &str) -> Vec<Diagnostic> {
        let m = selu_manifest::load_manifest(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
        )
        .unwrap();
        crate::check(&selu_syntax::parse("regression.selu", src).unwrap(), &m)
    }
    #[test]
    fn selu_constructor_callback_fields_are_not_foreign() {
        let ds = checked_source(
            r#"class Hud {
            clock: () => number; text: () => string;
            constructor(clock: () => number, text: () => string) { this.clock = clock; this.text = text; }
        }"#,
        );
        assert!(ds.is_empty(), "{ds:?}");
    }
    #[test]
    fn selu_interface_callbacks_are_not_foreign() {
        let ds = checked_source(
            r#"interface Scheduler { after: (seconds: number, callback: () => void) => void; }
        class Hud { scheduler: Scheduler;
            constructor(scheduler: Scheduler) { this.scheduler = scheduler; }
            run() { this.scheduler.after(0, () => {}); }
        }"#,
        );
        assert!(ds.is_empty(), "{ds:?}");
    }
    #[test]
    fn selu_array_sort_callback_is_not_foreign() {
        let ds = checked_source("function rank(files: number[]) { files.sort((a, b) => b - a); }");
        assert!(ds.is_empty(), "{ds:?}");
    }
    #[test]
    fn shadowed_hook_names_are_not_builtins() {
        let ds = checked_source(
            r#"function run(hook: ((state: number) => boolean)?) { return hook != null ? hook(1) : false; }
            { const hookScript = (state: number) => state; hookScript(1); }
            hook("GetTime", () => {});
            { const hook = (value: number) => value; hook(1); }
        "#,
        );
        assert_eq!(ds.len(), 1, "{ds:?}");
        assert_eq!(ds[0].code, "SELU0604");
    }
    #[test]
    fn manifest_tables_and_namespaces_remain_foreign() {
        let mut m = selu_manifest::load_manifest(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
        )
        .unwrap();
        m.tables.push(
            serde_json::from_value(
                serde_json::json!({"name":"TaintFixtureCallback","type":"CallbackType"}),
            )
            .unwrap(),
        );
        let callback = serde_json::json!({"name":"callback","type":"TaintFixtureCallback"});
        m.tables.push(serde_json::from_value(serde_json::json!({"name":"TaintFixtureTable", "type":"Structure", "fields":[{"name":"install","type":"Function","arguments":[callback.clone()]}]})).unwrap());
        m.globals.push(serde_json::from_value(serde_json::json!({"name":"TaintFixtureGlobal", "type":"TaintFixtureTable", "evidence":"verified"})).unwrap());
        for name in ["C_TaintFixture.Install", "C_Timer.After"] {
            m.functions.push(serde_json::from_value(serde_json::json!({"name":name,"params":["callback"],"evidence":"verified","signature":{"name":name,"arguments":[callback.clone()]}})).unwrap());
        }
        m.functions.push(serde_json::from_value(serde_json::json!({"name":"TaintFixtureGetTable","params":[],"evidence":"verified","signature":{"name":"TaintFixtureGetTable","returns":[{"name":"result","type":"TaintFixtureTable"}]}})).unwrap());
        let src = r#"TaintFixtureGlobal.install(() => {}); const t = TaintFixtureGetTable(); t.install(() => {}); C_TaintFixture.Install(() => {}); C_Timer.After(() => {});"#;
        let p = selu_syntax::parse("manifest-taint.selu", src).unwrap();
        let ds = crate::check(&p, &m);
        assert_eq!(ds.len(), 3, "{ds:?}");
        assert!(ds.iter().all(|d| d.code == "SELU0605"), "{ds:?}");
    }
    #[test]
    fn ownership_uses_binding_and_class_identities() {
        let m = selu_manifest::store::load_build(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
            "wow@12.1.5.70391",
        )
        .unwrap();
        let src = r#"
            function unrelated(frame: Frame) { frame.GetName(); }
            class Owned {
                frame: Frame;
                constructor() { const frame = CreateFrame("Frame"); this.frame = frame; frame.SetScript("OnShow", () => {}); }
                install() { this.frame.SetScript("OnHide", () => {}); }
            }
            class Foreign {
                frame: Frame;
                constructor() { this.frame = UIParent; }
                install() { this.frame.SetScript("OnShow", () => {}); }
            }
        "#;
        let p = selu_syntax::parse("identity.selu", src).unwrap();
        let ds = crate::check(&p, &m);
        assert_eq!(ds.len(), 1, "{ds:?}");
        assert_eq!(ds[0].code, "SELU0605");
        assert!(src[ds[0].span.as_ref().unwrap().start..].starts_with("this.frame.SetScript"));
    }
    #[test]
    fn only_manifest_arms_of_unions_are_game_types() {
        let widget = Type::Widget("Frame".into(), vec![]);
        let object = Type::Object(Default::default());
        assert!(is_game_type(&Type::Union(vec![
            object.clone(),
            widget.clone()
        ])));
        for t in [
            object,
            Type::Array(Box::new(widget.clone())),
            Type::Map(Box::new(Type::Number), Box::new(widget.clone())),
            Type::Set(Box::new(widget)),
            Type::Unknown,
            Type::Untyped,
        ] {
            assert!(!is_game_type(&t), "{t:?}");
        }
    }
    #[test]
    fn foreign_and_owned_hooks() {
        let d = lint(
            r#"const f = CreateFrame("Frame"); const child = CreateFrame("Frame", null, f); hookScript(child, "OnShow", () => {}); hookScript(UIParent, "OnShow", () => {}); hook(UIParent, "Show", () => {}); hook("GetTime", () => {});"#,
            false,
            &[],
        );
        assert_eq!(d.iter().filter(|d| d.code == "SELU0604").count(), 3);
        assert!(d.iter().all(|d| d.severity == Severity::Warning));
    }
    #[test]
    fn callbacks_and_registries() {
        let d = lint(
            r#"foreign.SetCalculator(() => {}); foreign.callback = () => {}; C_Timer.After(1, () => {}); EventRegistry.RegisterCallback("X", () => {}); const own = {}; own.callback = () => {};"#,
            false,
            &[],
        );
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|d| d.code == "SELU0605"));
    }
    #[test]
    fn suppression_and_deny() {
        assert_eq!(
            lint(
                "const bank = C_Bank; const fetch = bank.FetchPurchasedBankTabData; fetch();",
                false,
                &[]
            )[0]
            .code,
            "SELU0606"
        );
        assert!(
            lint(
                r#"allowTaint("reviewed caller", C_Bank.FetchPurchasedBankTabData());"#,
                false,
                &[]
            )
            .is_empty()
        );
        let d = lint(
            r#"allowTaint("  ", C_Bank.FetchPurchasedBankTabData());"#,
            false,
            &[],
        );
        assert!(d.iter().any(|d| d.code == "SELU0607"));
        assert!(d.iter().any(|d| d.code == "SELU0606"));
        assert_eq!(
            lint(
                "Custom.API();",
                false,
                &[("Custom.API".into(), "local incident".into())]
            )[0]
            .code,
            "SELU0606"
        );
    }
    #[test]
    fn strict_and_invalidated_proof() {
        let d = lint(
            r#"let f = CreateFrame("Frame"); f = UIParent; hookScript(f, "OnHide", () => {});"#,
            true,
            &[],
        );
        assert_eq!(d[0].severity, Severity::Error);
    }
    #[test]
    fn parameters_and_suppression_boundaries() {
        let d = lint(
            r#"function inspect(frame: Frame) { hookScript(frame, "OnShow", () => {}); } inspect(UIParent); allowTaint("outer reviewed", foreign.Install(() => { C_Bank.PurchaseBankTab(); })); foreign.callback = allowTaint("reviewed store", () => {});"#,
            false,
            &[],
        );
        // A parameter is of unknown origin: only strict mode reports it.
        assert_eq!(d.iter().filter(|d| d.code == "SELU0604").count(), 0);
        assert_eq!(d.iter().filter(|d| d.code == "SELU0606").count(), 1);
        assert!(!d.iter().any(|d| d.code == "SELU0605"));
        let strict = lint(
            r#"function inspect(frame: Frame) { hookScript(frame, "OnShow", () => {}); }"#,
            true,
            &[],
        );
        assert_eq!(strict.iter().filter(|d| d.code == "SELU0604").count(), 1);
    }
    #[test]
    fn unknown_origins_warn_only_in_strict_mode() {
        // A frame returned by the addon's own factory is unknown, not foreign.
        let src = r#"function make(): Frame { return CreateFrame("Frame"); } const f = make(); f.SetScript("OnShow", () => {}); hookScript(f, "OnShow", () => {});"#;
        assert!(
            lint(src, false, &[]).is_empty(),
            "{:?}",
            lint(src, false, &[])
        );
        let strict = lint(src, true, &[]);
        assert!(strict.iter().any(|d| d.code == "SELU0605"), "{strict:?}");
        assert!(strict.iter().any(|d| d.code == "SELU0604"), "{strict:?}");
        // Proven game values still warn by default.
        let d = lint(
            r#"const frame = UIParent; frame.SetScript("OnShow", () => {}); hookScript(UIParent, "OnShow", () => {});"#,
            false,
            &[],
        );
        assert!(d.iter().any(|d| d.code == "SELU0605"), "{d:?}");
        assert!(d.iter().any(|d| d.code == "SELU0604"), "{d:?}");
    }
    #[test]
    fn module_values_and_class_fields() {
        assert!(lint(r#"class View { frame: Frame; constructor() { this.frame = CreateFrame("Frame"); } show() { hookScript(this.frame, "OnShow", () => {}); } }"#, false, &[]).is_empty());
        assert!(lint(r#"const own = { frame: CreateFrame("Frame") }; hookScript(own.frame, "OnShow", () => {}); class View { frame = CreateFrame("Frame"); show() { hookScript(this.frame, "OnShow", () => {}); } }"#, false, &[]).is_empty());
    }
}
