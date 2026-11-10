//! Generic functions: `function first<T>(xs: T[]): T?`.
//!
//! Inside the function a type parameter is an opaque slot, assignable only to
//! itself: the body can pass `T` values along, store them in `T[]` or
//! `Map<string, T>`, and return them, but never read one as a number, string or
//! table. Each call infers the type arguments from its arguments (callbacks
//! last, so their parameters see the inferred types) and is checked against the
//! substituted signature.
use crate::{Ctx, Type, err};
use selu_diagnostics::Span;
use selu_syntax::{Expr, ExprKind};
use std::collections::HashMap;

const PREFIX: &str = "@typeparam:";

fn slot(name: &str, at: &Span) -> String {
    format!("{PREFIX}{name}@{}:{}", at.file, at.start)
}

/// The written name of a type parameter slot, for messages.
pub(crate) fn param_name(slot: &str) -> Option<&str> {
    slot.strip_prefix(PREFIX)?.split('@').next()
}

/// `ty` with the generic slots recorded on its function type.
pub(crate) fn with_type_params(ty: Type, slots: Vec<String>) -> Type {
    match ty {
        Type::Function {
            multiple_results,
            unspecified_callback,
            api,
            mut api_semantics,
            params,
            ret,
            required,
            rest,
        } => {
            api_semantics.type_params = slots;
            Type::Function {
                multiple_results,
                unspecified_callback,
                api,
                api_semantics,
                params,
                ret,
                required,
                rest,
            }
        }
        other => other,
    }
}

fn mentions(t: &Type, slots: &[String]) -> bool {
    match t {
        Type::Opaque(s) => slots.contains(s),
        Type::Named(..) => {
            let shape = crate::shape_of(t);
            !matches!(shape, Type::Named(..)) && mentions(&shape, slots)
        }
        Type::Union(ts) | Type::Tuple(ts) => ts.iter().any(|t| mentions(t, slots)),
        Type::Optional(t)
        | Type::SavedRepair(t)
        | Type::Secret(t)
        | Type::Array(t)
        | Type::Set(t) => mentions(t, slots),
        Type::Map(k, v) | Type::WeakMap(k, v) => mentions(k, slots) || mentions(v, slots),
        Type::Object(fields) => fields.values().any(|t| mentions(t, slots)),
        Type::Function {
            params, ret, rest, ..
        } => {
            params.iter().any(|t| mentions(t, slots))
                || mentions(ret, slots)
                || rest.as_deref().is_some_and(|t| mentions(t, slots))
        }
        _ => false,
    }
}

/// A literal binds its base type: `first(["a"])` is a `string`, not `"a"`.
fn widen(t: Type) -> Type {
    match t {
        Type::StringLiteral(_) => Type::String,
        Type::NumberLiteral(_) => Type::Number,
        Type::BooleanLiteral(_) => Type::Boolean,
        Type::Union(ts) => {
            let widened: Vec<Type> = ts.into_iter().map(widen).collect();
            Type::union(widened)
        }
        other => other,
    }
}

fn unify(param: &Type, actual: &Type, slots: &[String], bound: &mut HashMap<String, Type>) {
    if !mentions(param, slots) {
        return;
    }
    let actual = crate::shape_of(actual);
    match param {
        Type::Opaque(s) if slots.contains(s) => {
            if matches!(actual, Type::Untyped | Type::Error) {
                return;
            }
            let actual = widen(actual);
            let next = match bound.get(s) {
                Some(previous) => Type::join(previous, &actual).unwrap_or(actual),
                None => actual,
            };
            bound.insert(s.clone(), next);
        }
        Type::Named(..) => unify(&crate::shape_of(param), &actual, slots, bound),
        Type::Optional(p) => match &actual {
            Type::Optional(a) => unify(p, a, slots, bound),
            Type::Null => {}
            a => unify(p, a, slots, bound),
        },
        Type::Array(p) | Type::Set(p) => {
            if let Type::Array(a) | Type::Set(a) = &actual {
                unify(p, a, slots, bound);
            }
        }
        Type::Map(pk, pv) | Type::WeakMap(pk, pv) => {
            if let Type::Map(ak, av) | Type::WeakMap(ak, av) = &actual {
                unify(pk, ak, slots, bound);
                unify(pv, av, slots, bound);
            }
        }
        Type::Tuple(ps) => {
            if let Type::Tuple(as_) = &actual {
                for (p, a) in ps.iter().zip(as_) {
                    unify(p, a, slots, bound);
                }
            }
        }
        Type::Object(pf) => {
            if let Type::Object(af) = &actual {
                for (k, p) in pf {
                    if let Some(a) = af.get(k) {
                        unify(p, a, slots, bound);
                    }
                }
            }
        }
        Type::Function {
            params: pp,
            ret: pr,
            ..
        } => {
            if let Type::Function {
                params: ap,
                ret: ar,
                ..
            } = &actual
            {
                for (p, a) in pp.iter().zip(ap) {
                    unify(p, a, slots, bound);
                }
                unify(pr, ar, slots, bound);
            }
        }
        Type::Union(ps) => {
            // `T | null`-like unions: the non-slot arms absorb matching parts.
            let open: Vec<&Type> = ps.iter().filter(|p| mentions(p, slots)).collect();
            if open.len() == 1 {
                let rest: Vec<Type> = ps.iter().filter(|p| !mentions(p, slots)).cloned().collect();
                let remaining: Vec<Type> = actual
                    .members()
                    .into_iter()
                    .filter(|a| !rest.iter().any(|r| a.assignable_to(r)))
                    .collect();
                if !remaining.is_empty() {
                    unify(open[0], &Type::union(remaining), slots, bound);
                }
            }
        }
        _ => {}
    }
}

/// `t` with every bound slot replaced; unbound slots become `fallback`.
pub(crate) fn substitute(
    t: &Type,
    slots: &[String],
    bound: &HashMap<String, Type>,
    fallback: &Type,
) -> Type {
    if !mentions(t, slots) {
        return t.clone();
    }
    let go = |t: &Type| substitute(t, slots, bound, fallback);
    match t {
        Type::Opaque(s) => bound.get(s).cloned().unwrap_or_else(|| fallback.clone()),
        Type::Named(..) => go(&crate::shape_of(t)),
        Type::Union(ts) => Type::union(ts.iter().map(go).collect()),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(go).collect()),
        Type::Optional(t) => Type::optional(go(t)),
        Type::SavedRepair(t) => Type::SavedRepair(Box::new(go(t))),
        Type::Secret(t) => Type::Secret(Box::new(go(t))),
        Type::Array(t) => Type::Array(Box::new(go(t))),
        Type::Set(t) => Type::Set(Box::new(go(t))),
        Type::Map(k, v) => Type::Map(Box::new(go(k)), Box::new(go(v))),
        Type::WeakMap(k, v) => Type::WeakMap(Box::new(go(k)), Box::new(go(v))),
        Type::Object(fields) => {
            Type::Object(fields.iter().map(|(k, v)| (k.clone(), go(v))).collect())
        }
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
            multiple_results: *multiple_results,
            unspecified_callback: *unspecified_callback,
            api: api.clone(),
            api_semantics: api_semantics.clone(),
            params: params.iter().map(go).collect(),
            ret: Box::new(go(ret)),
            required: *required,
            rest: rest.as_deref().map(|t| Box::new(go(t))),
        },
        other => other.clone(),
    }
}

impl Ctx<'_> {
    /// Makes `function f<T, U>`'s parameters resolvable as opaque slots while
    /// its signature and body are checked. Returns the slots (empty when the
    /// function is not generic); pair with `leave_type_params`.
    pub(crate) fn enter_type_params(
        &mut self,
        at: &Span,
        params: &[(String, Span)],
        report: bool,
    ) -> Vec<String> {
        if params.is_empty() {
            return Vec::new();
        }
        let mut names: HashMap<String, Type> = self
            .aliases
            .context_stack
            .last()
            .map(|c| (**c).clone())
            .unwrap_or_default();
        let mut slots = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (name, span) in params {
            if !seen.insert(name.clone()) {
                if !report {
                    continue;
                }
                self.out.push(
                    err(
                        "SELU0650",
                        format!("type parameter `{name}` is declared twice"),
                        span,
                    )
                    .with_help("give each type parameter its own name"),
                );
                continue;
            }
            let s = slot(name, at);
            names.insert(name.clone(), Type::Opaque(s.clone()));
            slots.push(s);
        }
        self.aliases.context_stack.push(std::sync::Arc::new(names));
        slots
    }

    pub(crate) fn leave_type_params(&mut self, slots: &[String]) {
        if !slots.is_empty() {
            self.aliases.context_stack.pop();
        }
    }

    /// A generic callee: infers its type arguments from `args` and returns
    /// the substituted signature, so the ordinary call checks (arity,
    /// argument types, contextual callbacks) see concrete types. Arguments
    /// are checked here once; their recorded types are reused afterwards.
    pub(crate) fn instantiate_generic(
        &mut self,
        call: &Span,
        args: &[Expr],
        callee: &Type,
    ) -> Option<(Type, Vec<Type>)> {
        let Type::Function {
            api_semantics,
            params,
            rest,
            ret,
            ..
        } = callee
        else {
            return None;
        };
        let slots = api_semantics.type_params.clone();
        if slots.is_empty() {
            return None;
        }
        let param_at = |i: usize| params.get(i).or(rest.as_deref());
        let mut bound = HashMap::new();
        // Plain arguments first, callbacks after: a callback's parameters
        // take their types from what the other arguments fixed.
        let mut deferred = Vec::new();
        let mut actuals = vec![Type::Untyped; args.len()];
        for (i, arg) in args.iter().enumerate() {
            let Some(param) = param_at(i) else {
                actuals[i] = self.value(arg);
                continue;
            };
            if matches!(arg.kind, ExprKind::Function { .. }) && mentions(param, &slots) {
                deferred.push(i);
                continue;
            }
            let expected = (!mentions(param, &slots)).then_some(param);
            let actual = self.value_expected(arg, expected);
            unify(param, &actual, &slots, &mut bound);
            actuals[i] = actual;
        }
        for i in deferred {
            let param = param_at(i).unwrap();
            let expected = substitute(param, &slots, &bound, &Type::Untyped);
            let actual = self.value_expected(&args[i], Some(&expected));
            unify(param, &actual, &slots, &mut bound);
            actuals[i] = actual;
        }
        for s in &slots {
            if !bound.contains_key(s) && mentions(ret, std::slice::from_ref(s)) {
                let name = param_name(s).unwrap_or("T");
                self.out.push(
                    err(
                        "SELU0651",
                        format!("the type of `{name}` cannot be inferred from this call"),
                        call,
                    )
                    .with_help(format!("pass an argument that uses `{name}`")),
                );
            }
        }
        let mut instance = substitute(callee, &slots, &bound, &Type::Error);
        if let Type::Function { api_semantics, .. } = &mut instance {
            api_semantics.type_params.clear();
        }
        Some((instance, actuals))
    }
}
