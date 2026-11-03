use selu_diagnostics::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StmtKind {
    VarDecl {
        is_const: bool,
        name: String,
        name_span: Span,
        ty: Option<String>,
        init: Option<Box<Expr>>,
    },
    FuncDecl {
        name: String,
        name_span: Span,
        params: Vec<Param>,
        /// The written return type, if any.
        ret: Option<String>,
        body: Vec<Stmt>,
    },
    OnHandler {
        event: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
    },
    /// `global name = value;`: declares and writes a Lua global. The only
    /// way Selu code creates a global (SPEC-V1 2.2).
    Global {
        name: String,
        name_span: Span,
        init: Box<Expr>,
    },
    Return(Option<Box<Expr>>),
    If {
        cond: Box<Expr>,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Number(String),
    Str(String),
    Bool(bool),
    Null,
    Ident(String),
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    /// Unique within one parsed file; later passes key facts about an
    /// expression (its type) by this id.
    pub id: ExprId,
    pub kind: ExprKind,
    pub span: Span,
}

pub type ExprId = u32;

impl Expr {
    /// `a.b.c` as `a.b.c`, when the whole chain is plain identifiers.
    pub fn dotted_name(&self) -> Option<String> {
        match &self.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::Member { object, name } => {
                Some(format!("{}.{}", object.dotted_name()?, name))
            }
            _ => None,
        }
    }
}
