//! Restriction policy from manifest data; broad access flags are not combat flags.
use selu_manifest::{Declaration, Manifest};

pub(crate) fn secret_results(m: &Manifest, d: &Declaration) -> Vec<bool> {
    let full = d
        .namespace
        .as_ref()
        .map(|n| format!("{n}.{}", d.name))
        .unwrap_or_else(|| d.name.clone());
    raw_secret_results(d, m.function(&full).is_some_and(|f| f.returns_secret))
}
fn raw_secret_results(d: &Declaration, returns_secret: bool) -> Vec<bool> {
    let per_result = d.returns.iter().any(|r| {
        ["SecretValue", "ConditionalSecret"]
            .iter()
            .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true))
    });
    let all = !per_result
        && (d.attributes.get("SecretReturns").and_then(|v| v.as_bool()) == Some(true)
            || returns_secret
            || d.attributes
                .keys()
                .any(|r| r.starts_with("SecretWhen") || r.starts_with("SecretIn")));
    d.returns
        .iter()
        .enumerate()
        .map(|(index, r)| {
            d.attributes
                .get(&format!("SeluSecretResult{index}"))
                .and_then(|v| v.as_bool())
                == Some(true)
                || r.attributes.get("NeverSecret").and_then(|v| v.as_bool()) != Some(true)
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

/// Merge restriction policies without changing any target's API signatures.
pub(crate) fn strictest_policies(manifest: &mut Manifest, targets: &[Manifest]) {
    if targets.len() < 2 {
        return;
    }
    use std::collections::HashMap;
    let mut policies: HashMap<String, (Vec<bool>, bool)> = HashMap::new();
    fn record(
        policies: &mut HashMap<String, (Vec<bool>, bool)>,
        key: String,
        d: &Declaration,
        secret: bool,
    ) {
        let mask = raw_secret_results(d, secret);
        let entry = policies.entry(key).or_default();
        entry.0.resize(entry.0.len().max(mask.len()), false);
        for (into, value) in entry.0.iter_mut().zip(mask) {
            *into |= value;
        }
        entry.1 |= d
            .attributes
            .get("IsProtectedFunction")
            .and_then(|v| v.as_bool())
            == Some(true);
    }
    for target in targets {
        for f in &target.functions {
            if f.available
                && let Some(d) = &f.signature
            {
                record(&mut policies, f.name.clone(), d, f.returns_secret);
            }
        }
        for w in &target.widgets {
            for d in &w.methods {
                record(&mut policies, format!("@{}.{}", w.name, d.name), d, false);
            }
        }
    }
    fn apply(d: &mut Declaration, policy: Option<&(Vec<bool>, bool)>) {
        if let Some((mask, protected)) = policy {
            for (i, secret) in mask.iter().enumerate() {
                if *secret {
                    d.attributes
                        .insert(format!("SeluSecretResult{i}"), true.into());
                }
            }
            if *protected {
                d.attributes
                    .insert("IsProtectedFunction".into(), true.into());
            }
        }
    }
    for f in &mut manifest.functions {
        if let Some(d) = &mut f.signature {
            apply(d, policies.get(&f.name));
        }
    }
    for w in &mut manifest.widgets {
        for d in &mut w.methods {
            apply(d, policies.get(&format!("@{}.{}", w.name, d.name)));
        }
    }
}
