//! Check an assignment location once, before its RHS, then commit the store.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind};

pub(crate) struct PreparedWrite {
    pub contexts: Vec<Type>,
    pub read: Type,
    fallback: Option<Vec<Type>>,
    dependencies: crate::FlowState,
}
impl Ctx<'_> {
    pub(crate) fn prepare_write(&mut self, target: &Expr) -> PreparedWrite {
        let contexts = match &target.kind {
            ExprKind::Ident(name) => {
                self.mark_if_stable(name, target.id);
                self.lookup(name)
                    .map(|(_, b)| vec![b.ty.clone()])
                    .unwrap_or_default()
            }
            ExprKind::Member { object, name } => {
                let receiver = self.value(object);
                let members = receiver.members();
                if members.iter().any(
                    |t| matches!(t.operand_type(), Type::ApiOpaque(n, _) if n == "SeluOpaqueTable"),
                ) {
                    self.out.push(err(
                        "SELU0355",
                        "opaque table fields are read-only",
                        &target.span,
                    ));
                }
                let contexts: Vec<_> = members
                    .iter()
                    .map(|t| self.member(t, name, target))
                    .collect();
                let read = Type::union(contexts.clone());
                self.types
                    .set(target.id, self.refined_access(target).unwrap_or(read));
                if name == "length"
                    && members
                        .iter()
                        .any(|t| matches!(t.operand_type(), Type::Array(_) | Type::String))
                {
                    self.out
                        .push(err("SELU0355", "length is read-only", &target.span));
                    return PreparedWrite {
                        contexts: vec![],
                        read: Type::Number,
                        fallback: None,
                        dependencies: Default::default(),
                    };
                }
                contexts
            }
            ExprKind::Index { object, .. } => {
                let read = self.expr_inner(target);
                let receiver = self.types.get(object.id).cloned().unwrap_or(Type::Error);
                let members = receiver.members();
                if members.len() > 1
                    && members
                        .iter()
                        .all(|t| matches!(crate::shape_of(t), Type::Array(_)))
                {
                    self.out.push(err(
                        "SELU0355",
                        "writing through a union of arrays is not supported",
                        &target.span,
                    ));
                }
                vec![read]
            }
            _ => {
                self.expr(target);
                self.out.push(
                    err(
                        "SELU0344",
                        "this expression is not an assignment target",
                        &target.span,
                    )
                    .with_help("assign to a variable, field or index"),
                );
                vec![]
            }
        };
        let read = if let ExprKind::Ident(name) = &target.kind {
            self.type_of(name).unwrap_or(Type::Error)
        } else {
            self.refined_access(target)
                .unwrap_or_else(|| Type::union(contexts.clone()))
        };
        let read = self.refresh_class_type(read);
        let read = self.restriction_value_type(target, read);
        self.types.set(target.id, read.clone());
        // If the RHS invalidates a receiver guard, enforce the complete
        // declared receiver arms without evaluating the location again.
        let (fallback, dependencies) = if let ExprKind::Member { object, .. }
        | ExprKind::Index { object, .. } = &target.kind
        {
            let path = self.access_path(object);
            let dependencies = self.flow_state().into_iter().filter(|(key, _)| {
                path.as_ref().is_some_and(|path| match key {
                    crate::FactKey::Binding(id) => matches!(path.root, crate::access_paths::Root::Binding(root) if root == *id),
                    crate::FactKey::Path(prefix) => prefix.root == path.root && path.steps.starts_with(&prefix.steps),
                    _ => false,
                })
            }).collect();
            let fallback = self.declared_location_type(object).and_then(|t| {
                t.members()
                    .iter()
                    .filter(|t| **t != Type::Null)
                    .map(|t| match (&target.kind, crate::shape_of(t)) {
                        (
                            ExprKind::Member { name, .. },
                            Type::Object(fields) | Type::Class(_, _, fields),
                        ) => fields.get(name).cloned(),
                        (ExprKind::Index { .. }, Type::Array(item)) => Some(*item),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
            });
            (fallback, dependencies)
        } else {
            (None, Default::default())
        };
        PreparedWrite {
            contexts,
            read,
            fallback,
            dependencies,
        }
    }
    fn declared_location_type(&self, e: &Expr) -> Option<Type> {
        match &e.kind {
            ExprKind::Ident(name) => Some(self.lookup(name)?.1.ty.clone()),
            ExprKind::This => self.types.get(e.id).cloned(),
            ExprKind::Member { object, name } => {
                let receiver = self.declared_location_type(object)?;
                Some(Type::union(
                    receiver
                        .members()
                        .iter()
                        .filter(|t| **t != Type::Null)
                        .map(|t| match crate::shape_of(t) {
                            Type::Object(fields) | Type::Class(_, _, fields) => {
                                fields.get(name).cloned()
                            }
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                ))
            }
            ExprKind::Index { object, .. } => {
                let receiver = self.declared_location_type(object)?;
                Some(Type::union(
                    receiver
                        .members()
                        .iter()
                        .map(|t| match crate::shape_of(t) {
                            Type::Array(item) => Some(*item),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                ))
            }
            _ => None,
        }
    }
    pub(crate) fn commit_write(&mut self, target: &Expr, prepared: &PreparedWrite, value: &Type) {
        self.saved_write_value(target, value, &target.span);
        if let ExprKind::Ident(name) = &target.kind {
            self.write(name, &target.span, value);
        } else {
            let current = self.flow_state();
            let invalidated = prepared.dependencies.iter().any(|(key, old)| {
                current
                    .get(key)
                    .is_none_or(|new| old.ty != new.ty || old.fn_depth != new.fn_depth)
            });
            let contexts = if invalidated {
                prepared.fallback.as_ref().unwrap_or(&prepared.contexts)
            } else {
                &prepared.contexts
            };
            for ty in contexts {
                if !value.assignable_to(ty) {
                    self.out.push(
                        err(
                            "SELU0355",
                            format!("cannot store `{value}` in `{ty}`"),
                            &target.span,
                        )
                        .with_help("the value must fit every possible receiver arm"),
                    );
                }
            }
            self.invalidate_path_writes();
        }
    }
}
