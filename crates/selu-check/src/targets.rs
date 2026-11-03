//! Client availability and scoped capability facts.
use crate::{Ctx, Narrowings, Type, err};
use selu_diagnostics::Span;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind};

pub struct Availability {
    targets: Vec<Manifest>,
}
impl Availability {
    pub fn new(targets: &[Manifest]) -> Self {
        Self {
            targets: targets.to_vec(),
        }
    }
}
pub fn exists(m: &Manifest, n: &str) -> bool {
    m.function(n).is_some() || m.event(n).is_some() || m.has_global(n)
}
pub fn with_fallbacks(primary: &Manifest, targets: &[Manifest]) -> Manifest {
    let mut m = primary.clone();
    for t in targets {
        for f in &t.functions {
            if !m.functions.iter().any(|x| x.name == f.name) {
                m.functions.push(f.clone());
            }
        }
        for e in &t.events {
            if m.event(&e.name).is_none() {
                m.events.push(e.clone());
            }
        }
        for g in &t.globals {
            if !m.globals.iter().any(|x| x.name == g.name) {
                m.globals.push(g.clone());
            }
        }
    }
    m
}
impl Ctx<'_> {
    fn availability_error(&mut self, n: &str, span: &Span) -> bool {
        let missing: Vec<_> = self
            .availability
            .targets
            .iter()
            .filter(|m| {
                self.scopes.iter().all(|s| {
                    s.narrowed.keys().all(|k| {
                        if let Some(x) = k.strip_prefix("@has:") {
                            exists(m, x)
                        } else if let Some(x) = k.strip_prefix("@lacks:") {
                            !exists(m, x)
                        } else {
                            true
                        }
                    })
                })
            })
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
    pub(crate) fn target_member(&mut self, t: &Type, field: &str, object: &Expr) -> Option<Type> {
        if *t == Type::Error {
            return Some(Type::Error);
        }
        let root = object.dotted_name()?;
        if self.lookup(root.split('.').next()?).is_some() {
            return None;
        }
        self.target_read(&format!("{root}.{field}"), &object.span)
    }
    pub(crate) fn target_read(&mut self, n: &str, span: &Span) -> Option<Type> {
        if self.lookup(n).is_some() || !self.availability.targets.iter().any(|m| exists(m, n)) {
            return None;
        }
        if self.scopes.iter().any(|s| {
            s.narrowed
                .keys()
                .any(|k| k.starts_with(&format!("@has:{n}.")))
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
            vec![(format!("@has:{n}"), Type::Untyped)],
            vec![(format!("@lacks:{n}"), Type::Untyped)],
        ))
    }
}
