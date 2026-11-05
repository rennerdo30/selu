//! Client availability and scoped capability facts.
use crate::{Ctx, FactKey, Narrowings, Type, err};
use selu_diagnostics::Span;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind};

#[derive(Clone)]
pub struct Availability {
    targets: std::sync::Arc<Vec<Manifest>>,
}
impl Availability {
    pub(crate) fn manifests(&self) -> &[Manifest] {
        &self.targets
    }
    pub fn new(targets: &[Manifest]) -> Self {
        Self {
            targets: std::sync::Arc::new(targets.to_vec()),
        }
    }
}
pub fn namespace(m: &Manifest, n: &str) -> bool {
    m.functions
        .iter()
        .any(|f| f.name.starts_with(&format!("{n}.")))
        || n == "Enum" && !m.enums.is_empty()
}
fn enum_exists(m: &Manifest, n: &str) -> bool {
    m.enums.iter().any(|e| {
        let base = format!("Enum.{}", e.name);
        n == base
            || n.strip_prefix(&format!("{base}."))
                .is_some_and(|v| e.values.contains_key(v))
    })
}
pub fn exists(m: &Manifest, n: &str) -> bool {
    m.function(n).is_some() || m.event(n).is_some() || m.has_global(n) || enum_exists(m, n)
}
pub fn with_fallbacks(primary: &Manifest, targets: &[Manifest]) -> Manifest {
    use std::collections::HashSet;
    let mut m = primary.clone();
    let mut functions: HashSet<_> = m
        .functions
        .iter()
        .filter(|f| f.available)
        .map(|f| f.name.clone())
        .collect();
    let mut events: HashSet<_> = m.events.iter().map(|f| f.name.clone()).collect();
    let mut globals: HashSet<_> = m.globals.iter().map(|f| f.name.clone()).collect();
    for t in targets {
        if std::ptr::eq(primary, t) {
            continue;
        }
        for (into, from) in [
            (&mut m.tables, &t.tables),
            (&mut m.widgets, &t.widgets),
            (&mut m.predicates, &t.predicates),
        ] {
            let mut names: HashSet<_> = into.iter().map(|d| d.name.clone()).collect();
            for d in from {
                if names.insert(d.name.clone()) {
                    into.push(d.clone());
                }
            }
        }
        for f in &t.functions {
            if f.available && functions.insert(f.name.clone()) {
                m.functions.retain(|x| x.name != f.name);
                m.functions.push(f.clone());
            }
        }
        for e in &t.enums {
            if let Some(existing) = m.enums.iter_mut().find(|x| x.name == e.name) {
                for (key, value) in &e.values {
                    existing
                        .values
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            } else {
                m.enums.push(e.clone());
            }
        }
        for e in &t.events {
            if events.insert(e.name.clone()) {
                m.events.push(e.clone());
            }
        }
        for g in &t.globals {
            if globals.insert(g.name.clone()) {
                m.globals.push(g.clone());
            }
        }
    }
    crate::restrictions::strictest_policies(&mut m, targets);
    m
}

impl Ctx<'_> {
    pub(crate) fn target_is_active(&self, m: &Manifest) -> bool {
        self.scopes.iter().all(|s| {
            s.narrowed.keys().all(|k| {
                let FactKey::Capability(k) = k else {
                    return true;
                };
                if let Some(x) = k.strip_prefix("@has:") {
                    exists(m, x)
                } else if let Some(x) = k.strip_prefix("@lacks:") {
                    !exists(m, x)
                } else {
                    true
                }
            })
        })
    }
    fn availability_error(&mut self, n: &str, span: &Span) -> bool {
        let missing: Vec<_> = self
            .availability
            .targets
            .iter()
            .filter(|m| self.target_is_active(m))
            .filter(|m| !exists(m, n))
            .map(Manifest::target_string)
            .collect();
        if missing.is_empty() {
            return false;
        }
        self.out.push(
            err(
                "SELU0500",
                format!("`{n}` is unavailable on {}", missing.join(", ")),
                span,
            )
            .with_help(format!("guard it with `has({n})`")),
        );
        true
    }
    pub(crate) fn target_event(&mut self, n: &str, span: &Span) {
        if self
            .availability
            .targets
            .iter()
            .any(|m| m.event(n).is_some())
        {
            self.availability_error(n, span);
        }
    }
    pub(crate) fn target_member(&mut self, t: &Type, _field: &str, object: &Expr) -> Option<Type> {
        if *t == Type::Error {
            return Some(Type::Error);
        }
        let root = object.dotted_name()?;
        if self.lookup(root.split('.').next()?).is_some() {
            return None;
        }
        if self.availability.targets.iter().any(|m| exists(m, &root))
            && self.availability_error(&root, &object.span)
        {
            return Some(Type::Error);
        }
        if self.availability.targets.iter().any(|m| {
            m.enums.iter().any(|e| {
                root.strip_prefix(&format!("Enum.{}.", e.name))
                    .is_some_and(|v| e.values.contains_key(v))
            })
        }) {
            return Some(Type::Number);
        }
        None
    }
    pub(crate) fn target_receiver(&mut self, e: &Expr) -> Option<Type> {
        let n = e.dotted_name()?;
        if self.lookup(n.split('.').next()?).is_some() {
            return None;
        }
        if self.availability.targets.iter().any(|m| namespace(m, &n)) {
            self.types.set(e.id, Type::Untyped);
            return Some(Type::Untyped);
        }
        None
    }
    pub(crate) fn target_read(&mut self, n: &str, span: &Span) -> Option<Type> {
        if self.lookup(n).is_some() {
            return None;
        }
        if self.availability.targets.iter().any(|m| namespace(m, n)) {
            self.out.push(
                err("SELU0502", format!("namespace `{n}` is not a value"), span)
                    .with_help(format!("call `{n}.Bar(...)` directly")),
            );
            return Some(Type::Error);
        }
        if !self.availability.targets.iter().any(|m| exists(m, n)) {
            return None;
        }
        if self.scopes.iter().any(|s| {
            s.narrowed.keys().any(
                |k| matches!(k, FactKey::Capability(k) if k.starts_with(&format!("@has:{n}."))),
            )
        }) {
            return None;
        }
        self.availability_error(n, span).then_some(Type::Error)
    }
    pub(crate) fn target_call(&mut self, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let n = callee.dotted_name()?;
        if self.lookup(n.split('.').next()?).is_some()
            || !self.availability.targets.iter().any(|m| exists(m, &n))
        {
            return None;
        }
        if !self.availability_error(&n, &callee.span) {
            if let ExprKind::Member { object, .. } = &callee.kind {
                self.target_receiver(object);
            }
            return None;
        }
        for a in args {
            self.value(a);
        }
        Some(Type::Error)
    }
    pub(crate) fn has_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        if callee.dotted_name().as_deref() != Some("has") || self.lookup("has").is_some() {
            return None;
        }
        let valid = args.len() == 1
            && args[0].dotted_name().is_some_and(|n| {
                self.lookup(n.split('.').next().unwrap()).is_none()
                    && self.availability.targets.iter().any(|m| exists(m, &n))
            });
        if !valid {
            self.out.push(
                err(
                    "SELU0501",
                    "has needs one manifest function, namespace, global or event",
                    &e.span,
                )
                .with_help("use `has(C_Foo.Bar)` with an API known on at least one target"),
            );
            return Some(Type::Error);
        }
        Some(Type::Boolean)
    }
    pub(crate) fn has_narrowings(&self, e: &Expr) -> Option<(Narrowings, Narrowings)> {
        let ExprKind::Call { callee, args } = &e.kind else {
            return None;
        };
        if callee.dotted_name().as_deref() != Some("has")
            || self.lookup("has").is_some()
            || args.len() != 1
        {
            return None;
        }
        let n = args[0].dotted_name()?;
        Some((
            vec![(FactKey::Capability(format!("@has:{n}")), Type::Untyped)],
            vec![(FactKey::Capability(format!("@lacks:{n}")), Type::Untyped)],
        ))
    }
}
