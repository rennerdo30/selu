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
}
fn walk<'a>(c: Child<'a>, f: &mut impl FnMut(Child<'a>)) {
    match c {
        Child::Stmt(s) => visit::child_stmts_and_exprs(s, f),
        Child::Expr(e) => visit::child_exprs_and_stmts(e, f),
    }
}
impl Origins {
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
                    if n == "allowTaint" && !o.counts.contains_key(&n) && !o.writes.contains(&n) {
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
                    if (n == "hook"
                        && (args.len() == 2 || args.first().is_some_and(|e| !o.owned(e, 0))))
                        || (n == "hookScript"
                            && args.first().is_some_and(|e| !o.owned(e, 0))
                            && matches!(args.get(1).map(|e| &e.kind), Some(ExprKind::Str(s)) if matches!(s.as_str(), "OnShow"|"OnHide"|"OnEnable"|"OnDisable")))
                    {
                        emit(
                            "SELU0604",
                            "Hook runs inside a foreign call and can taint secure UI".into(),
                            "poll IsShown from an owned ticker or frame",
                        );
                    }
                    if let ExprKind::Member { object, .. } = &callee.kind
                        && !o.owned(object, 0)
                        && !n.starts_with("C_Timer.")
                        && !SAFE_REGISTRIES.contains(&n.as_str())
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
                    let allowed_value = matches!(&value.kind, ExprKind::Call { callee, args } if callee.dotted_name().as_deref() == Some("allowTaint") && !o.counts.contains_key("allowTaint") && !o.writes.contains("allowTaint") && args.len() == 2 && matches!(&args[0].kind, ExprKind::Str(s) if !s.trim().is_empty()));
                    if let ExprKind::Member { object, .. } | ExprKind::Index { object, .. } =
                        &target.kind
                        && !o.owned(object, 0)
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
        check(
            &selu_syntax::parse("taint.selu", src).unwrap(),
            &TypeTable::default(),
            strict,
            deny,
        )
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
        assert_eq!(d.iter().filter(|d| d.code == "SELU0604").count(), 1);
        assert_eq!(d.iter().filter(|d| d.code == "SELU0606").count(), 1);
        assert!(!d.iter().any(|d| d.code == "SELU0605"));
    }
    #[test]
    fn module_values_and_class_fields() {
        assert!(lint(r#"class View { frame: Frame; constructor() { this.frame = CreateFrame("Frame"); } show() { hookScript(this.frame, "OnShow", () => {}); } }"#, false, &[]).is_empty());
        assert!(lint(r#"const own = { frame: CreateFrame("Frame") }; hookScript(own.frame, "OnShow", () => {}); class View { frame = CreateFrame("Frame"); show() { hookScript(this.frame, "OnShow", () => {}); } }"#, false, &[]).is_empty());
    }
}
