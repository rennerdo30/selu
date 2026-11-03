//! The types the checker knows (SPEC-V1 2.1), and the table of expression
//! types it hands to the lowering.

use selu_syntax::ExprId;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Number,
    String,
    Boolean,
    /// The type of the `null` literal itself.
    Null,
    /// `T?`: a `T` or `null`.
    Optional(Box<Type>),
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// What a function without a value returns.
    Void,
    /// `a && b` or `a || b` with a `T?` operand: fine as a condition, but its
    /// value isn't a boolean in Lua, so it can't be stored or passed.
    Cond,
    /// A value whose type nobody wrote down yet: results of game API calls
    /// (manifests don't carry types yet) and parameters without a type.
    /// Internal only; it is never checked. See ARCHITECTURE.md.
    Untyped,
    /// An expression that already produced an error; suppresses follow-ups.
    Error,
}

impl Type {
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

    /// Types the checker can't say anything about.
    pub fn is_open(&self) -> bool {
        matches!(self, Type::Untyped | Type::Error)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Optional(_) | Type::Null)
    }

    /// Whether a value of type `self` may be stored where `to` is expected.
    pub fn assignable_to(&self, to: &Type) -> bool {
        if self.is_open() || to.is_open() || self == to {
            return true;
        }
        match (self, to) {
            (Type::Null, Type::Optional(_)) => true,
            (Type::Optional(a), Type::Optional(b)) => a.assignable_to(b),
            (t, Type::Optional(inner)) => t.assignable_to(inner),
            (
                Type::Function { params, ret },
                Type::Function {
                    params: to_params,
                    ret: to_ret,
                },
            ) => {
                params.len() == to_params.len()
                    && to_params
                        .iter()
                        .zip(params)
                        .all(|(a, b)| a.assignable_to(b))
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
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::Boolean => write!(f, "boolean"),
            Type::Null => write!(f, "null"),
            Type::Optional(t) => write!(f, "{t}?"),
            Type::Function { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "({}) => {ret}", ps.join(", "))
            }
            Type::Void => write!(f, "void"),
            Type::Cond => write!(f, "condition"),
            Type::Untyped | Type::Error => write!(f, "unknown"),
        }
    }
}

/// Parses a written type (`number`, `string?`). `None` for names the
/// checker doesn't know.
pub fn parse_annotation(text: &str) -> Option<Type> {
    let (base, optional) = match text.strip_suffix('?') {
        Some(b) => (b, true),
        None => (text, false),
    };
    let t = match base {
        "number" => Type::Number,
        "string" => Type::String,
        "boolean" => Type::Boolean,
        _ => return None,
    };
    Some(if optional { Type::optional(t) } else { t })
}

/// The type of every checked expression, by expression id. The lowering
/// uses it to pick the Lua operator (`+` or `..`) and similar choices.
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    types: HashMap<ExprId, Type>,
}

impl TypeTable {
    pub fn set(&mut self, id: ExprId, t: Type) {
        self.types.insert(id, t);
    }
    pub fn get(&self, id: ExprId) -> Option<&Type> {
        self.types.get(&id)
    }
    pub fn is_string(&self, id: ExprId) -> bool {
        self.get(id) == Some(&Type::String)
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
        assert_eq!(parse_annotation("string?").unwrap().to_string(), "string?");
        assert_eq!(parse_annotation("Frame"), None);
    }
}
