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

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Nominal identity, base identities, and the checked instance member shape.
    Class(String, Vec<String>, BTreeMap<String, Type>),
    ClassObject(String),
    Widget(String, Vec<String>),
    ApiOpaque(String, Vec<Type>),
    Number,
    String,
    Boolean,
    /// The type of the `null` literal itself.
    Null,
    /// `T?`: a `T` or `null`.
    Optional(Box<Type>),
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
    pub(crate) fn result_convention_matches(&self, other: &Type) -> bool {
        match (self, other) {
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

    pub fn optional(t: Type) -> Type {
        match t {
            Type::Optional(_) | Type::Untyped | Type::Error => t,
            Type::Null => Type::Null,
            t => Type::Optional(Box::new(t)),
        }
    }

    /// The `T` of a `T?` (and `T` itself otherwise).
    pub fn non_null(&self) -> Type {
        match self {
            Type::Optional(t) => (**t).clone(),
            t => t.clone(),
        }
    }

    /// The same type, treating unknown parts as matching anything.
    pub fn same(&self, other: &Type) -> bool {
        self.is_open()
            || other.is_open()
            || (self.assignable_to(other) && other.assignable_to(self))
    }

    /// Types the checker can't say anything about.
    pub fn is_open(&self) -> bool {
        matches!(self, Type::Untyped | Type::Error)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Optional(_) | Type::Null)
    }

    /// Whether a value of type `self` may be stored where `to` is expected.
    pub fn assignable_to(&self, to: &Type) -> bool {
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
        if self.is_open() || to.is_open() || self == to {
            return true;
        }
        match (self, to) {
            (Type::Class(have, bases, _), Type::Class(name, _, _)) => {
                have == name || bases.contains(name)
            }
            (Type::Class(_, _, have), Type::Object(want)) => want.iter().all(|(k, w)| {
                !matches!(w.non_null(), Type::Function { .. })
                    && match have.get(k) {
                        Some(h) => h.assignable_to(w),
                        None => w.is_optional(),
                    }
            }),
            (Type::ApiOpaque(_, members), target) if !members.is_empty() => {
                members.iter().all(|t| t.assignable_to(target))
            }
            (source, Type::ApiOpaque(_, members)) if !members.is_empty() => {
                members.iter().any(|t| source.assignable_to(t))
            }
            (Type::Widget(a, bases), Type::Widget(b, _)) => a == b || bases.contains(b),
            (Type::Null, Type::Optional(_)) => true,
            (Type::Optional(a), Type::Optional(b)) => a.assignable_to(b),
            (t, Type::Optional(inner)) => t.assignable_to(inner),
            // Arrays and tables are mutable, so their element types must
            // match exactly (`number[]` is not a `number?[]`).
            (Type::Array(a), Type::Array(b)) | (Type::Set(a), Type::Set(b)) => a.same(b),
            (Type::Map(k1, v1), Type::Map(k2, v2))
            | (Type::WeakMap(k1, v1), Type::WeakMap(k2, v2)) => k1.same(k2) && v1.same(v2),
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.assignable_to(y))
            }
            // Structural: every field `to` needs must be there and fit; a
            // missing field is fine when `to` allows null for it.
            (Type::Object(have), Type::Object(want)) => {
                want.iter().all(|(k, w)| match have.get(k) {
                    Some(h) => h.assignable_to(w),
                    None => w.is_optional(),
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
                self.result_convention_matches(to)
                    && required <= to_required
                    && (rest.is_some() || (to_rest.is_none() && params.len() >= to_params.len()))
                    && to_params.iter().enumerate().all(|(i, a)| {
                        params
                            .get(i)
                            .or(rest.as_deref())
                            .is_some_and(|b| a.assignable_to(b))
                    })
                    && to_rest.as_deref().is_none_or(|a| {
                        params
                            .iter()
                            .skip(to_params.len())
                            .all(|b| a.assignable_to(b))
                            && rest.as_deref().is_some_and(|b| a.assignable_to(b))
                    })
                    && (ret.assignable_to(to_ret) || **to_ret == Type::Void)
            }
            _ => false,
        }
    }

    /// The narrowest type both `a` and `b` fit in, if there is one.
    pub fn join(a: &Type, b: &Type) -> Option<Type> {
        if a.is_open() || b.is_open() {
            return Some(Type::Untyped);
        }
        if b.assignable_to(a) {
            return Some(a.clone());
        }
        if a.assignable_to(b) {
            return Some(b.clone());
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
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::Boolean => write!(f, "boolean"),
            Type::Null => write!(f, "null"),
            Type::Optional(t) => write!(f, "{t}?"),
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
                Some(Box::new(match last {
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
        "number" | "string" | "boolean" | "null" => 0,
        "Set" => 1,
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
        "number" => Type::Number,
        "string" => Type::String,
        "boolean" => Type::Boolean,
        "null" => Type::Null,
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
    types: HashMap<ExprId, Type>,
    stable: std::collections::HashSet<ExprId>,
    method_calls: std::collections::HashSet<ExprId>,
    single_values: std::collections::HashSet<ExprId>,
}

impl TypeTable {
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
        self.types.insert(id, t);
    }
    pub fn get(&self, id: ExprId) -> Option<&Type> {
        self.types.get(&id)
    }
    pub fn is_string(&self, id: ExprId) -> bool {
        self.get(id) == Some(&Type::String)
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
        assert!(ty("{ a: number; b: string }").assignable_to(&ty("{ a: number; c?: boolean }")));
        assert!(!ty("{ a: number }").assignable_to(&ty("{ a: number; b: string }")));
        assert!(!ty("number[]").assignable_to(&ty("number?[]")));
    }
}
