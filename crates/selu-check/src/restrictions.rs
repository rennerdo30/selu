//! Restriction policy from manifest data; broad access flags are not combat flags.
use selu_manifest::{Declaration, Manifest};

pub(crate) fn secret_results(m: &Manifest, d: &Declaration) -> Vec<bool> {
    let full = d
        .namespace
        .as_ref()
        .map(|n| format!("{n}.{}", d.name))
        .unwrap_or_else(|| d.name.clone());
    let entry = m.function(&full);
    let per_result = d.returns.iter().any(|r| {
        ["SecretValue", "ConditionalSecret"]
            .iter()
            .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true))
    });
    let all = !per_result
        && (d.attributes.get("SecretReturns").and_then(|v| v.as_bool()) == Some(true)
            || entry.is_some_and(|f| {
                (f.returns_secret && !per_result)
                    || f.restrictions
                        .iter()
                        .any(|r| r.starts_with("SecretWhen") || r.starts_with("SecretIn"))
            })
            || d.attributes
                .keys()
                .any(|r| r.starts_with("SecretWhen") || r.starts_with("SecretIn")));
    d.returns
        .iter()
        .map(|r| {
            r.attributes.get("NeverSecret").and_then(|v| v.as_bool()) != Some(true)
                && (all
                    || ["SecretValue", "ConditionalSecret"]
                        .iter()
                        .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true)))
        })
        .collect()
}

/// Every context may execute during combat, including file load on demand.
pub(crate) fn check_uses(
    program: &selu_syntax::Program,
    types: &crate::TypeTable,
    out: &mut Vec<selu_diagnostics::Diagnostic>,
) {
    use selu_syntax::{ExprKind, StmtKind, visit};
    fn walk(
        c: visit::Child<'_>,
        allowed: Option<selu_syntax::ExprId>,
        types: &crate::TypeTable,
        out: &mut Vec<selu_diagnostics::Diagnostic>,
    ) {
        match c {
            visit::Child::Stmt(s) => {
                let allowed = if let StmtKind::Expr(e) = &s.kind {
                    Some(e.id)
                } else {
                    None
                };
                visit::child_stmts_and_exprs(s, &mut |c| walk(c, allowed, types, out));
            }
            visit::Child::Expr(e) => {
                if allowed != Some(e.id)
                    && let ExprKind::Call { callee, .. } = &e.kind
                    && matches!(types.get(callee.id), Some(crate::Type::Function { api_semantics, .. }) if api_semantics.protected)
                {
                    out.push(
                        crate::err(
                            "SELU0600",
                            "a protected call whose result is used cannot be deferred",
                            &e.span,
                        )
                        .with_help("call it as a statement, then read state after combat"),
                    );
                }
                visit::child_exprs_and_stmts(e, &mut |c| walk(c, None, types, out));
            }
        }
    }
    for s in &program.stmts {
        walk(visit::Child::Stmt(s), None, types, out);
    }
}
