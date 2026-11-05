//! The types the checker knows (SPEC-V1 2.1), and the table of expression
//! types it hands to the lowering.

use selu_diagnostics::{Diagnostic, Span};
use selu_syntax::{ExprId, TypeExpr, TypeExprKind};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ApiSignature {
    pub name: String,
    pub parameters: Vec<String>,
    pub target: String,
}
#[derive(Debug, Clone, Default)]
pub struct ApiOrigin(pub Option<ApiSignature>);
impl PartialEq for ApiOrigin {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApiSemantics {
    pub untyped_return_tail: bool,
    pub typed_return_tail: Option<Box<Type>>,
    pub frame_factory: bool,
    pub animation_factory: bool,
    pub date_factory: bool,
    pub pool_factory: bool,
    pub script_callbacks: BTreeMap<String, Type>,
    pub native_callback: bool,
    pub pure_callback: bool,
    pub table_operation: Option<String>,
    pub secret_results: Vec<bool>,
    pub protected: bool,
    pub widget_protected: bool,
    pub dynamic_protected: bool,
    pub accepts_secrets: bool,
    pub plain_result: bool,
    pub overloads: Vec<Type>,
}
impl PartialEq for ApiSemantics {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone)]
pub enum ShapeBody {
    Definition(std::sync::Arc<std::sync::OnceLock<Type>>),
    Reference(std::sync::Weak<std::sync::OnceLock<Type>>),
}
#[derive(Debug, Clone)]
pub struct NamedShape {
    pub(crate) key: String,
    body: ShapeBody,
}
impl NamedShape {
    pub(crate) fn new(key: String) -> Self {
        Self {
            key,
            body: ShapeBody::Definition(Default::default()),
        }
    }
    pub(crate) fn reference(&self) -> Self {
        Self {
            key: self.key.clone(),
            body: match &self.body {
                ShapeBody::Definition(a) => ShapeBody::Reference(std::sync::Arc::downgrade(a)),
                ShapeBody::Reference(w) => ShapeBody::Reference(w.clone()),
            },
        }
    }
    pub(crate) fn get(&self) -> Option<Type> {
        match &self.body {
            ShapeBody::Definition(a) => a.get().cloned(),
            ShapeBody::Reference(w) => w.upgrade().and_then(|a| a.get().cloned()),
        }
    }
    pub(crate) fn set(&self, t: Type) {
        if let ShapeBody::Definition(a) = &self.body {
            let _ = a.set(t);
        }
    }
}

impl std::fmt::Debug for ShapeBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NamedShape")
    }
}
impl PartialEq for NamedShape {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Union(Vec<Type>),
    StringLiteral(String),
    NumberLiteral(String),
    BooleanLiteral(bool),
    Unknown,
    Named(String, NamedShape),
    /// Nominal identity, base identities, and the checked instance member shape.
    Class(String, Vec<String>, BTreeMap<String, Type>),
    ClassObject(String),
    Widget(String, Vec<String>),
    ApiOpaque(String, Vec<Type>),
    ApiIterator(Vec<Type>),
    Number,
    String,
    Boolean,
    /// The type of the `null` literal itself.
    Null,
    /// `T?`: a `T` or `null`.
    Optional(Box<Type>),
    /// Closed historical repair input: T | string | number | boolean.
    /// T must be a closed table shape; table members require a typeof guard.
    SavedRepair(Box<Type>),
    Secret(Box<Type>),
    /// `T[]`: a 0-based array without holes, a Lua sequence.
    Array(Box<Type>),
    /// `[A, B]`; also what a call with several results returns.
    Tuple(Vec<Type>),
    /// `{ a: T; b?: U }`: optional fields are stored as `T?`.
    Object(BTreeMap<String, Type>),
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
    /// `WeakMap<K, V>`: a `Map` whose table has weak keys (`__mode = "k"`).
    WeakMap(Box<Type>, Box<Type>),
    Function {
        multiple_results: bool,
        unspecified_callback: bool,
        api: ApiOrigin,
        api_semantics: ApiSemantics,
        params: Vec<Type>,
        ret: Box<Type>,
        required: usize,
        rest: Option<Box<Type>>,
    },
    /// What a function without a value returns.
    Void,
    /// `a && b` or `a || b` with a `T?` operand: fine as a condition, but its
    /// value isn't a boolean in Lua, so it can't be stored or passed.
    Cond,
    /// A value whose type nobody wrote down yet: names-only API values and
    /// parameters without a type.
    /// Internal only; it is never checked. See ARCHITECTURE.md.
    Untyped,
    /// An expression that already produced an error; suppresses follow-ups.
    Error,
}

impl Type {
    /// Runtime alternatives, resolving nested aliases and optional wrappers.
    pub(crate) fn members(&self) -> Vec<Type> {
        match crate::shape_of(self) {
            Type::Union(ts) => ts.iter().flat_map(Type::members).collect(),
            Type::Optional(t) => t.members().into_iter().chain([Type::Null]).collect(),
            Type::SavedRepair(t) => t
                .members()
                .into_iter()
                .chain([Type::String, Type::Number, Type::Boolean])
                .collect(),
            t => vec![t],
        }
    }
    pub(crate) fn union(ts: Vec<Type>) -> Type {
        let mut members = Vec::new();
        for t in ts.into_iter().flat_map(|t| t.members()) {
            if !members.contains(&t) {
                members.push(t);
            }
        }
        if members.len() == 1 {
            members.remove(0)
        } else {
            Type::Union(members)
        }
    }

    pub(crate) fn operand_type(&self) -> Type {
        match crate::shape_of(self) {
            Type::StringLiteral(_) => Type::String,
            Type::NumberLiteral(_) => Type::Number,
            Type::BooleanLiteral(_) => Type::Boolean,
            Type::Union(ts) if !ts.is_empty() => {
                let first = ts[0].operand_type();
                if ts.iter().all(|t| t.operand_type() == first) {
                    first
                } else {
                    self.clone()
                }
            }
            t => t,
        }
    }

    pub(crate) fn result_convention_matches(&self, other: &Type) -> bool {
        let a = crate::shape_of(self);
        let b = crate::shape_of(other);
        match (&a, &b) {
            (
                Type::Function {
                    multiple_results: a,
                    ..
                },
                Type::Function {
                    multiple_results: b,
                    unspecified_callback,
                    ..
                },
            ) => *unspecified_callback || a == b,
            _ => true,
        }
    }

    pub fn secret(t: Type) -> Type {
        if matches!(t, Type::Error | Type::Secret(_)) {
            t
        } else {
            Type::Secret(Box::new(t))
        }
    }
    pub fn optional(t: Type) -> Type {
        if t.is_optional() {
            return t;
        }
        if let Type::Secret(inner) = t {
            return Type::Secret(Box::new(Type::optional(*inner)));
        }
        match t {
            Type::Optional(_) | Type::Untyped | Type::Error => t,
            Type::Null => Type::Null,
            t => Type::Optional(Box::new(t)),
        }
    }

    /// The `T` of a `T?` (and `T` itself otherwise).
    pub fn non_null(&self) -> Type {
        crate::aliases::non_null_of(self)
    }

    /// The same type, treating unknown parts as matching anything.
    pub fn same(&self, other: &Type) -> bool {
        self.is_open()
            || other.is_open()
            || (self.assignable_to(other) && other.assignable_to(self))
    }

    /// Types the checker can't say anything about.
    pub fn is_open(&self) -> bool {
        matches!(crate::shape_of(self), Type::Untyped | Type::Error)
    }

    pub fn is_optional(&self) -> bool {
        match crate::aliases::shape_of(self) {
            Type::Optional(_) | Type::Null => true,
            Type::Secret(t) => t.is_optional(),
            Type::Union(ts) => ts.iter().any(Type::is_optional),
            _ => false,
        }
    }

    /// Whether a value of type `self` may be stored where `to` is expected.
    pub fn assignable_to(&self, to: &Type) -> bool {
        self.assignable_seen(to, &mut std::collections::HashSet::new())
    }
    fn assignable_seen(
        &self,
        to: &Type,
        seen: &mut std::collections::HashSet<(String, String)>,
    ) -> bool {
        if matches!(self, Type::Function { .. })
            && matches!(
                to,
                Type::Function {
                    unspecified_callback: true,
                    ..
                }
            )
        {
            return true;
        }
        if matches!(to, Type::Unknown | Type::Untyped)
            && crate::restrictions::secret_inner(self).is_some()
        {
            return false;
        }
        if matches!(to, Type::Unknown) {
            return true;
        }
        if let (Type::Named(_, a), Type::Named(_, b)) = (self, to)
            && a.key == b.key
        {
            return true;
        }
        if let Type::Named(_, t) = self {
            let key = (t.key.clone(), crate::aliases::identity_key(to));
            if !seen.insert(key.clone()) {
                return true;
            }
            let result = t.get().is_some_and(|t| t.assignable_seen(to, seen));
            seen.remove(&key);
            return result;
        }
        if let Type::Named(_, t) = to {
            if t.key.starts_with("@selu_closed:")
                && let (Type::Object(actual), Some(Type::Object(fields))) =
                    (crate::shape_of(self), t.get())
                && actual.keys().any(|key| !fields.contains_key(key))
            {
                return false;
            }
            let key = (
                crate::aliases::identity_key(self),
                format!("target:{}", t.key),
            );
            if !seen.insert(key.clone()) {
                return true;
            }
            let result = t.get().is_some_and(|t| self.assignable_seen(&t, seen));
            seen.remove(&key);
            return result;
        }
        if self.is_open() || to.is_open() || self == to {
            return true;
        }
        match (self, to) {
            (Type::NumberLiteral(a), Type::NumberLiteral(b)) => a
                .parse::<f64>()
                .ok()
                .zip(b.parse::<f64>().ok())
                .is_some_and(|(a, b)| a == b),
            (Type::Class(have, bases, _), Type::Class(name, _, _)) => {
                have == name || bases.contains(name)
            }
            (Type::Class(_, _, have), Type::Object(want)) => want.iter().all(|(k, w)| {
                !matches!(w.non_null(), Type::Function { .. })
                    && match have.get(k) {
                        Some(h) => {
                            h.assignable_seen(w, &mut seen.clone())
                                && w.assignable_seen(h, &mut seen.clone())
                        }
                        None => false,
                    }
            }),
            (Type::ApiOpaque(_, members), target) if !members.is_empty() => {
                members.iter().all(|t| t.assignable_seen(target, seen))
            }
            (source, Type::ApiOpaque(_, members)) if !members.is_empty() => {
                members.iter().any(|t| source.assignable_seen(t, seen))
            }
            (Type::Widget(a, bases), Type::Widget(b, _)) => a == b || bases.contains(b),
            (Type::Secret(a), Type::Optional(b)) => {
                Type::Secret(a.clone()).assignable_seen(b, seen)
            }
            (Type::Secret(a), Type::Secret(b)) => a.assignable_seen(b, seen),
            (Type::Secret(_), _) => false,
            (t, Type::Secret(b)) => t.assignable_seen(b, seen),
            (Type::Union(ts), target) => ts
                .iter()
                .all(|t| t.assignable_seen(target, &mut seen.clone())),
            (Type::Null, Type::Optional(_)) => true,
            (Type::Optional(a), Type::Optional(b)) => a.assignable_seen(b, seen),
            (t, Type::Optional(inner)) => t.assignable_seen(inner, seen),
            (Type::StringLiteral(_), Type::String)
            | (Type::NumberLiteral(_), Type::Number)
            | (Type::BooleanLiteral(_), Type::Boolean) => true,
            (Type::Optional(inner), Type::Union(_)) => {
                inner.assignable_seen(to, &mut seen.clone())
                    && Type::Null.assignable_seen(to, &mut seen.clone())
            }
            (source, Type::Union(ts)) => ts
                .iter()
                .any(|t| source.assignable_seen(t, &mut seen.clone())),
            (Type::SavedRepair(a), Type::SavedRepair(b)) => a.assignable_seen(b, seen),
            (Type::SavedRepair(_), _) => false,
            (Type::Number | Type::String | Type::Boolean, Type::SavedRepair(_)) => true,
            (source, Type::SavedRepair(table)) => source.assignable_seen(table, seen),
            // Arrays and tables are mutable, so their element types must
            // match exactly (`number[]` is not a `number?[]`).
            (Type::Array(a), Type::Array(b)) | (Type::Set(a), Type::Set(b)) => {
                a.assignable_seen(b, seen) && b.assignable_seen(a, seen)
            }
            (Type::Map(k1, v1), Type::Map(k2, v2))
            | (Type::WeakMap(k1, v1), Type::WeakMap(k2, v2)) => {
                k1.assignable_seen(k2, seen)
                    && k2.assignable_seen(k1, seen)
                    && v1.assignable_seen(v2, seen)
                    && v2.assignable_seen(v1, seen)
            }
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.assignable_seen(y, seen))
            }
            // Existing writable views must expose compatible writes both ways.
            // Fresh literals acquire absent optional fields contextually;
            // erased aliases cannot safely recover hidden writable fields.
            (Type::Object(have), Type::Object(want)) => {
                want.iter().all(|(k, w)| match have.get(k) {
                    Some(h) => {
                        h.assignable_seen(w, &mut seen.clone())
                            && w.assignable_seen(h, &mut seen.clone())
                    }
                    None => false,
                })
            }
            (
                Type::Function {
                    params,
                    ret,
                    required,
                    rest,
                    ..
                },
                Type::Function {
                    params: to_params,
                    ret: to_ret,
                    required: to_required,
                    rest: to_rest,
                    ..
                },
            ) => {
                if matches!(to, Type::Function { api_semantics, .. } if api_semantics.native_callback)
                {
                    return self.result_convention_matches(to)
                        && (*required <= *to_required
                            || params
                                .iter()
                                .take(*required)
                                .skip(*to_required)
                                .all(|p| Type::Null.assignable_seen(p, seen)))
                        && params.iter().enumerate().all(|(i, p)| {
                            to_params
                                .get(i)
                                .or(to_rest.as_deref())
                                .is_none_or(|value| value.assignable_seen(p, seen))
                        })
                        && rest.as_deref().is_none_or(|p| {
                            to_params
                                .iter()
                                .skip(params.len())
                                .all(|value| value.assignable_seen(p, seen))
                                && to_rest
                                    .as_deref()
                                    .is_none_or(|value| value.assignable_seen(p, seen))
                        })
                        && (ret.assignable_seen(to_ret, seen) || **to_ret == Type::Void);
                }
                self.result_convention_matches(to)
                    && required <= to_required
                    && (rest.is_some() || (to_rest.is_none() && params.len() >= to_params.len()))
                    && to_params.iter().enumerate().all(|(i, a)| {
                        params
                            .get(i)
                            .or(rest.as_deref())
                            .is_some_and(|b| a.assignable_seen(b, seen))
                    })
                    && to_rest.as_deref().is_none_or(|a| {
                        params
                            .iter()
                            .skip(to_params.len())
                            .all(|b| a.assignable_seen(b, seen))
                            && rest.as_deref().is_some_and(|b| a.assignable_seen(b, seen))
                    })
                    && (ret.assignable_seen(to_ret, seen) || **to_ret == Type::Void)
            }
            _ => false,
        }
    }

    /// The narrowest type both `a` and `b` fit in, if there is one.
    pub fn join(a: &Type, b: &Type) -> Option<Type> {
        if a.is_open() || b.is_open() {
            if !matches!(crate::shape_of(a), Type::Error)
                && !matches!(crate::shape_of(b), Type::Error)
                && (crate::restrictions::secret_inner(a).is_some()
                    || crate::restrictions::secret_inner(b).is_some())
            {
                return Some(Type::secret(Type::Untyped));
            }
            return Some(Type::Untyped);
        }
        // A conditional function value may select either API. Preserve the
        // strictest runtime policy even though policy is identity-neutral.
        let policy = |mut ty: Type| {
            if let (Type::Widget(_, left), Type::Widget(_, right), Type::Widget(_, result)) =
                (a, b, &mut ty)
                && !(left.iter().any(|b| b == "@selu_plain")
                    && right.iter().any(|b| b == "@selu_plain"))
            {
                result.retain(|b| b != "@selu_plain");
            }
            if let (Type::Widget(_, left), Type::Widget(_, right), Type::Widget(_, result)) =
                (a, b, &mut ty)
            {
                for marker in left
                    .iter()
                    .chain(right)
                    .filter(|s| s.starts_with("@family:"))
                {
                    if !result.contains(marker) {
                        result.push(marker.clone());
                    }
                }
            }
            if let (
                Type::Function {
                    api_semantics: left,
                    ..
                },
                Type::Function {
                    api_semantics: right,
                    ..
                },
                Type::Function {
                    api_semantics: result,
                    ..
                },
            ) = (a, b, &mut ty)
            {
                result.protected = left.protected || right.protected;
                result.widget_protected = left.widget_protected || right.widget_protected;
                result.dynamic_protected = left.dynamic_protected || right.dynamic_protected;
                result.accepts_secrets = left.accepts_secrets && right.accepts_secrets;
                result.plain_result = left.plain_result && right.plain_result;
                result.frame_factory = left.frame_factory && right.frame_factory;
                result.animation_factory = left.animation_factory && right.animation_factory;
                result.date_factory = left.date_factory && right.date_factory;
                result.pool_factory = left.pool_factory && right.pool_factory;
                result.native_callback = left.native_callback && right.native_callback;
                result.pure_callback = left.pure_callback && right.pure_callback;
                if left.table_operation != right.table_operation {
                    result.table_operation = None;
                }
                if left.typed_return_tail != right.typed_return_tail {
                    result.typed_return_tail = None;
                }
                result
                    .script_callbacks
                    .retain(|name, ty| right.script_callbacks.get(name) == Some(ty));
                let count = left.secret_results.len().max(right.secret_results.len());
                result.secret_results = (0..count)
                    .map(|i| {
                        left.secret_results.get(i) == Some(&true)
                            || right.secret_results.get(i) == Some(&true)
                    })
                    .collect();
            }
            ty
        };
        if b.assignable_to(a) {
            return Some(policy(a.clone()));
        }
        if a.assignable_to(b) {
            return Some(policy(b.clone()));
        }
        match (a, b) {
            (Type::Null, t) | (t, Type::Null) => Some(Type::optional(t.clone())),
            (Type::Optional(x), y) | (y, Type::Optional(x)) => Type::join(x, y).map(Type::optional),
            _ => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Class(n, _, _)
            | Type::ClassObject(n)
            | Type::Widget(n, _)
            | Type::ApiOpaque(n, _) => {
                write!(f, "{}", n.split('@').next().unwrap_or(n))
            }
            Type::ApiIterator(_) => write!(f, "API iterator"),
            Type::Unknown => write!(f, "unknown"),
            Type::Named(n, _) => write!(f, "{n}"),
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::Boolean => write!(f, "boolean"),
            Type::Null => write!(f, "null"),
            Type::Optional(t) => write!(f, "{t}?"),
            Type::StringLiteral(s) => write!(f, "{s:?}"),
            Type::NumberLiteral(s) => f.write_str(s),
            Type::BooleanLiteral(b) => write!(f, "{b}"),
            Type::Union(ts) => {
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{t}")?;
                }
                Ok(())
            }
            Type::SavedRepair(t) => write!(f, "SavedRepair<{t}>"),
            Type::Secret(t) => write!(f, "Secret<{t}>"),
            Type::Array(t) => write!(f, "{t}[]"),
            Type::Tuple(ts) => {
                let ts: Vec<String> = ts.iter().map(|t| t.to_string()).collect();
                write!(f, "[{}]", ts.join(", "))
            }
            Type::Object(fields) => {
                let fs: Vec<String> = fields.iter().map(|(k, t)| format!("{k}: {t}")).collect();
                write!(f, "{{ {} }}", fs.join("; "))
            }
            Type::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            Type::Set(t) => write!(f, "Set<{t}>"),
            Type::WeakMap(k, v) => write!(f, "WeakMap<{k}, {v}>"),
            Type::Function {
                params,
                ret,
                required,
                rest,
                ..
            } => {
                let mut ps: Vec<String> = params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        if i >= *required {
                            format!("{p} = ...")
                        } else {
                            p.to_string()
                        }
                    })
                    .collect();
                if let Some(t) = rest {
                    ps.push(format!("...{t}[]"));
                }
                write!(f, "({}) => {ret}", ps.join(", "))
            }
            Type::Void => write!(f, "void"),
            Type::Cond => write!(f, "condition"),
            Type::Untyped | Type::Error => write!(f, "unknown"),
        }
    }
}

/// Resolves a written type. Unknown names are reported (SELU0320) and
/// become `Error`, so one typo gives one message.
pub fn resolve(t: &TypeExpr, out: &mut Vec<Diagnostic>) -> Type {
    resolve_classes(t, out, &HashMap::new())
}

pub fn resolve_classes(
    t: &TypeExpr,
    out: &mut Vec<Diagnostic>,
    classes: &HashMap<String, Type>,
) -> Type {
    match &t.kind {
        TypeExprKind::StringLiteral(s) => Type::StringLiteral(s.clone()),
        TypeExprKind::NumberLiteral(s) => Type::NumberLiteral(s.clone()),
        TypeExprKind::BooleanLiteral(b) => Type::BooleanLiteral(*b),
        TypeExprKind::Union(ts) => Type::Union(
            ts.iter()
                .map(|t| resolve_classes(t, out, classes))
                .collect(),
        ),
        TypeExprKind::Optional(inner) => Type::optional(resolve_classes(inner, out, classes)),
        TypeExprKind::Array(inner) => Type::Array(Box::new(resolve_classes(inner, out, classes))),
        TypeExprKind::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|i| resolve_classes(i, out, classes))
                .collect(),
        ),
        TypeExprKind::Object(fields) => Type::Object(
            fields
                .iter()
                .map(|f| {
                    let t = resolve_classes(&f.ty, out, classes);
                    (
                        f.name.clone(),
                        if f.optional { Type::optional(t) } else { t },
                    )
                })
                .collect(),
        ),
        TypeExprKind::Function { params, ret, rest } => {
            let mut fixed: Vec<Type> = params
                .iter()
                .map(|p| resolve_classes(p, out, classes))
                .collect();
            let rest = if *rest {
                let last = fixed.pop().expect("a rest type has a parameter");
                Some(Box::new(match crate::aliases::shape_of(&last) {
                    Type::Array(t) if !t.is_optional() => *t,
                    _ => {
                        out.push(
                            Diagnostic::error(
                                "SELU0370",
                                "A rest parameter needs an array type with non-null elements.",
                            )
                            .with_span(params.last().unwrap().span.clone())
                            .with_help("write `...xs: T[]` with a non-null T"),
                        );
                        Type::Error
                    }
                }))
            } else {
                None
            };
            Type::Function {
                multiple_results: false,
                unspecified_callback: false,
                api: Default::default(),
                api_semantics: Default::default(),
                required: fixed.len(),
                rest,
                params: fixed,
                ret: Box::new(if is_void(ret) {
                    Type::Void
                } else {
                    resolve_classes(ret, out, classes)
                }),
            }
        }
        TypeExprKind::Named { name, args } => {
            let args: Vec<Type> = args
                .iter()
                .map(|a| resolve_classes(a, out, classes))
                .collect();
            if args.is_empty()
                && let Some(t) = classes.get(name)
            {
                return t.clone();
            }
            named(name, args, &t.span, out)
        }
    }
}

/// `void` is only meaningful as a return type.
pub fn is_void(t: &TypeExpr) -> bool {
    matches!(&t.kind, TypeExprKind::Named { name, args } if name == "void" && args.is_empty())
}

fn named(name: &str, mut args: Vec<Type>, span: &Span, out: &mut Vec<Diagnostic>) -> Type {
    let want = match name {
        "Error" | "unknown" | "number" | "string" | "boolean" | "null" => 0,
        "Set" | "Secret" | "SavedRepair" => 1,
        "Map" | "WeakMap" => 2,
        _ => {
            out.push(
                Diagnostic::error("SELU0320", format!("unknown type `{name}`"))
                    .with_span(span.clone())
                    .with_help("known types are `number`, `string`, `boolean`, `T?`, `T[]`, `[A, B]`, `{ a: T }`, `Map<K, V>`, `Set<T>`, `WeakMap<K, V>` and function types `(a: A) => R`"),
            );
            return Type::Error;
        }
    };
    if args.len() != want {
        out.push(
            Diagnostic::error(
                "SELU0319",
                format!(
                    "`{name}` takes {want} type argument{}, not {}",
                    if want == 1 { "" } else { "s" },
                    args.len()
                ),
            )
            .with_span(span.clone()),
        );
        return Type::Error;
    }
    let mut next = || Box::new(args.remove(0));
    match name {
        "Error" => crate::errors::error_type(),
        "unknown" => Type::Unknown,
        "number" => Type::Number,
        "string" => Type::String,
        "boolean" => Type::Boolean,
        "null" => Type::Null,
        "Secret" => Type::secret(*next()),
        "SavedRepair" => {
            let table = next();
            if !crate::saved::closed_schema(&table, 0)
                || !matches!(
                    crate::shape_of(&table),
                    Type::Object(_) | Type::Array(_) | Type::Map(..)
                )
            {
                out.push(
                    Diagnostic::error(
                        "SELU0901",
                        "SavedRepair requires a closed serializable table shape",
                    )
                    .with_span(span.clone()),
                );
                Type::Error
            } else {
                Type::SavedRepair(table)
            }
        }
        "Set" => Type::Set(next()),
        "Map" => Type::Map(next(), next()),
        _ => Type::WeakMap(next(), next()),
    }
}

/// A parameter type as a manifest writes it (`string`, `number`...).
/// Anything else stays untyped until manifests carry full types.
pub fn manifest_type(name: &str) -> Type {
    match name {
        "number" => Type::Number,
        "string" => Type::String,
        "boolean" => Type::Boolean,
        _ => Type::Untyped,
    }
}

/// Resolves a type without reporting errors (for placeholders).
pub fn resolve_quiet(t: &TypeExpr) -> Type {
    resolve(t, &mut Vec::new())
}

/// The type of every checked expression, by expression id. The lowering
/// uses it to pick the Lua operator (`+` or `..`) and similar choices.
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    pub(crate) saved_adoptions: std::collections::HashSet<String>,
    pub saved_contexts: HashMap<usize, Type>,
    pub(crate) saved_parameters: std::collections::HashSet<usize>,
    pub saved_schemas: HashMap<usize, Type>,
    pub saved_readers: HashMap<usize, Type>,
    pub(crate) named_roots: Vec<NamedShape>,
    types: HashMap<ExprId, Type>,
    stable: std::collections::HashSet<ExprId>,
    frame_families: std::collections::HashSet<ExprId>,
    method_calls: std::collections::HashSet<ExprId>,
    single_values: std::collections::HashSet<ExprId>,
}

impl TypeTable {
    pub fn mark_frame_family(&mut self, id: ExprId) {
        self.frame_families.insert(id);
    }
    pub fn is_frame_family(&self, id: ExprId) -> bool {
        self.frame_families.contains(&id)
    }
    pub fn mark_single_value(&mut self, id: ExprId) {
        self.single_values.insert(id);
    }
    pub fn is_single_value(&self, id: ExprId) -> bool {
        self.single_values.contains(&id)
    }
    /// Marks a callee that refers to a declared instance method, including inherited methods.
    pub fn mark_method_call(&mut self, id: ExprId) {
        self.method_calls.insert(id);
    }
    /// Whether this callee needs the implicit instance receiver in Lua.
    pub fn is_method_call(&self, id: ExprId) -> bool {
        self.method_calls.contains(&id)
    }

    pub fn set(&mut self, id: ExprId, t: Type) {
        self.types.insert(id, crate::aliases::shape_of(&t));
    }
    pub fn get(&self, id: ExprId) -> Option<&Type> {
        self.types.get(&id)
    }
    pub fn is_string(&self, id: ExprId) -> bool {
        self.get(id)
            .is_some_and(|t| t.operand_type() == Type::String)
    }
    /// Marks a name use whose value can't change while the program runs: a
    /// `const`, a function declaration, or a game global (Selu code can't
    /// replace those, SELU0315).
    pub fn mark_stable(&mut self, id: ExprId) {
        self.stable.insert(id);
    }
    /// See [`TypeTable::mark_stable`]. The lowering reads such a name again
    /// instead of copying it into a temporary to keep the evaluation order.
    pub fn is_stable(&self, id: ExprId) -> bool {
        self.stable.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignability_and_join() {
        let num_opt = Type::optional(Type::Number);
        assert!(Type::Number.assignable_to(&num_opt));
        assert!(Type::Null.assignable_to(&num_opt));
        assert!(!num_opt.assignable_to(&Type::Number));
        assert!(!Type::String.assignable_to(&Type::Number));
        assert!(Type::Untyped.assignable_to(&Type::Number));
        assert_eq!(
            Type::join(&Type::Number, &Type::Null),
            Some(num_opt.clone())
        );
        assert_eq!(Type::join(&num_opt, &Type::Number), Some(num_opt));
        assert_eq!(Type::join(&Type::Number, &Type::String), None);
        let ty = |src: &str| {
            let p = selu_syntax::parse("t.selu", &format!("let x: {src};")).unwrap();
            match &p.stmts[0].kind {
                selu_syntax::StmtKind::VarDecl { ty: Some(t), .. } => resolve_quiet(t),
                _ => unreachable!(),
            }
        };
        let show = |src: &str| ty(src).to_string();
        assert_eq!(show("string?"), "string?");
        assert_eq!(show("number[]?"), "number[]?");
        assert_eq!(show("Map<string, number[]>"), "Map<string, number[]>");
        assert_eq!(
            show("{ a: number; b?: string }"),
            "{ a: number; b: string? }"
        );
        assert_eq!(show("(a: number) => void"), "(number) => void");
        assert_eq!(show("[string, boolean]"), "[string, boolean]");
        assert_eq!(show("Frame"), "unknown");
        // An existing view may hide an incompatible c field. Optional writable
        // fields can be added by a fresh literal context, not by alias conversion.
        assert!(!ty("{ a: number; b: string }").assignable_to(&ty("{ a: number; c?: boolean }")));
        assert!(
            ty("{ a: number; b: string; c?: boolean }")
                .assignable_to(&ty("{ a: number; c?: boolean }"))
        );
        assert!(!ty("{ value: string }").assignable_to(&ty("{ value: string | number }")));
        assert!(!ty("{ a: number }").assignable_to(&ty("{ a: number; b: string }")));
        assert!(!ty("number[]").assignable_to(&ty("number?[]")));
    }
}
