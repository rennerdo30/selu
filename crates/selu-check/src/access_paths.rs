//! Access paths use declaration identities, never variable spellings.
use crate::{BindingId, Ctx, FactKey, Type};
use selu_syntax::{Expr, ExprKind};
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Root {
    Binding(BindingId),
    Receiver(String, bool),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Step {
    Field(String),
    IndexBinding(BindingId),
    IndexNumber(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AccessPath {
    pub root: Root,
    pub steps: Vec<Step>,
}
impl AccessPath {
    pub fn depends_on(&self, id: BindingId) -> bool {
        matches!(self.root,Root::Binding(root) if root==id)
            || self
                .steps
                .iter()
                .any(|s| matches!(s,Step::IndexBinding(index) if *index==id))
    }
}
impl Ctx<'_> {
    pub(crate) fn access_path(&self, e: &Expr) -> Option<AccessPath> {
        match &e.kind {
            ExprKind::Ident(n) => Some(AccessPath {
                root: Root::Binding(self.lookup(n)?.1.id),
                steps: vec![],
            }),
            ExprKind::This => {
                let (key, static_) = self.classes.receiver.as_ref()?;
                Some(AccessPath {
                    root: Root::Receiver(key.clone(), *static_),
                    steps: vec![],
                })
            }
            ExprKind::Member { object, name } | ExprKind::OptionalMember { object, name } => {
                let mut p = self.access_path(object)?;
                p.steps.push(Step::Field(name.clone()));
                Some(p)
            }
            ExprKind::Index { object, index } => {
                let mut p = self.access_path(object)?;
                let step = match &index.kind {
                    ExprKind::Ident(n) => Step::IndexBinding(self.lookup(n)?.1.id),
                    ExprKind::Number(n) => Step::IndexNumber(n.parse::<f64>().ok()?.to_string()),
                    ExprKind::Unary { op, expr } if op == "-" => {
                        let ExprKind::Number(n) = &expr.kind else {
                            return None;
                        };
                        let n = -n.parse::<f64>().ok()?;
                        Step::IndexNumber(if n == 0.0 { "0".into() } else { n.to_string() })
                    }
                    _ => return None,
                };
                p.steps.push(step);
                Some(p)
            }
            _ => None,
        }
    }
    pub(crate) fn access_key(&self, e: &Expr) -> Option<FactKey> {
        let p = self.access_path(e)?;
        if p.steps.is_empty()
            && let Root::Binding(id) = p.root
        {
            Some(FactKey::Binding(id))
        } else {
            Some(FactKey::Path(p))
        }
    }
    pub(crate) fn discriminant_key(&self, e: &Expr) -> Option<FactKey> {
        match self.access_key(e)? {
            FactKey::Binding(id) => Some(FactKey::Field(id)),
            key => Some(key),
        }
    }
    pub(crate) fn refined_access(&self, e: &Expr) -> Option<Type> {
        let key = self.access_key(e)?;
        if !matches!(key, FactKey::Path(_)) {
            return None;
        }
        self.scopes.iter().rev().find_map(|s| {
            s.narrowed
                .get(&key)
                .filter(|n| n.fn_depth == self.fn_depth)
                .map(|n| n.ty.clone())
        })
    }
    pub(crate) fn checked_access_type(&self, e: &Expr) -> Option<Type> {
        if let ExprKind::Ident(n) = &e.kind {
            return self.type_of(n);
        }
        if let Some(t) = self.refined_access(e) {
            return Some(t);
        }
        let t = self.types.get(e.id)?.clone();
        // An indexed lookup can be absent even though nullable *elements* are
        // forbidden. Include that possibility when partitioning a guard.
        Some(if matches!(e.kind, ExprKind::Index { .. }) {
            Type::optional(t)
        } else {
            t
        })
    }
    pub(crate) fn invalidate_path_writes(&mut self) {
        // Without alias analysis, a field/index store can affect another root's
        // path. Conservatively discard all field-dependent facts.
        for scope in &mut self.scopes {
            scope.narrowed.retain(|_, n| !n.field);
        }
    }
}
