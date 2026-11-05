use selu_diagnostics::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

/// A written type: `number`, `string?`, `number[]`, `Map<number, string>`,
/// `[string, number]`, `{ x: number; label?: string }`,
/// `(a: number) => string`. Names are resolved by the checker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExprKind {
    /// `number`, `Frame`, `Map<K, V>` (with `args`).
    Named { name: String, args: Vec<TypeExpr> },
    /// `T?`
    Optional(Box<TypeExpr>),
    /// `T[]`
    Array(Box<TypeExpr>),
    /// `[A, B]`
    Tuple(Vec<TypeExpr>),
    /// `{ a: T; b?: U }`
    Object(Vec<FieldType>),
    /// `(a: A, b: B) => R`
    Function {
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
        /// The last parameter is a rest array rather than a fixed argument.
        rest: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldType {
    pub name: String,
    pub name_span: Span,
    pub optional: bool,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    /// `x = 5`: the value used when the caller passes nothing.
    pub default: Option<Box<Expr>>,
    /// `...rest`: collects the remaining arguments into an array.
    pub rest: bool,
    pub span: Span,
}

/// The names a destructuring declaration binds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// `[a, , b]`: `None` skips a position.
    Array(Vec<Option<(String, Span)>>),
    /// `{ a, b: renamed }`: (field, bound name, bound-name span, field span).
    Object(Vec<(String, String, Span, Span)>),
}

/// What a `for...of` loop binds on each turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ForBinding {
    Name(String, Span),
    Pattern(Pattern),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StmtKind {
    TypeDecl(TypeDecl),
    Saved {
        name: String,
        name_span: Span,
        per_character: bool,
        adopted: bool,
        version_key: Option<String>,
        legacy: Option<String>,
        version: u32,
        defaults: Box<Expr>,
    },
    Migrate {
        name: String,
        name_span: Span,
        step_span: Span,
        from: u32,
        to: u32,
        param: Param,
        context: Option<Box<Param>>,
        body: Vec<Stmt>,
    },
    VarDecl {
        is_const: bool,
        name: String,
        name_span: Span,
        ty: Option<TypeExpr>,
        init: Option<Box<Expr>>,
    },
    /// `const [a, b] = f();` or `const { x, y } = point;`
    Destructure {
        is_const: bool,
        pattern: Pattern,
        init: Box<Expr>,
    },
    FuncDecl {
        name: String,
        name_span: Span,
        params: Vec<Param>,
        /// The written return type, if any.
        ret: Option<TypeExpr>,
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
    Throw {
        value: Box<Expr>,
    },
    Try {
        body: Vec<Stmt>,
        catch: Option<(Option<Param>, Vec<Stmt>)>,
        finally: Option<Vec<Stmt>>,
    },
    Return(Option<Box<Expr>>),
    If {
        cond: Box<Expr>,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        cond: Box<Expr>,
        body: Vec<Stmt>,
    },
    DoWhile {
        body: Vec<Stmt>,
        cond: Box<Expr>,
    },
    /// `for (init; cond; update) body`; every part may be missing.
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Box<Expr>>,
        update: Option<Box<Expr>>,
        body: Vec<Stmt>,
    },
    /// `for (const x of xs) body`
    ForOf {
        is_const: bool,
        binding: ForBinding,
        iter: Box<Expr>,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
    /// `import { a, b as c } from "./other";` or `import logo from "../x.svg";`
    Import {
        /// `{ a, b as c }`: (exported name, local name, local-name span, exported-name span).
        names: Vec<(String, String, Span, Span)>,
        /// `import logo from ...`: the local name and its span.
        default: Option<(String, Span)>,
        from: String,
        from_span: Span,
    },
    /// `export` before a declaration (`function`, `const`, `let`, `class`,
    /// `global`).
    Export(Box<Stmt>),
    Class(ClassDecl),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<(String, Option<TypeExpr>)>,
    pub param_spans: Vec<Span>,
    pub bases: Vec<TypeExpr>,
    pub body: TypeExpr,
}

/// `class Name extends Base { ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDecl {
    pub name: String,
    pub name_span: Span,
    pub extends: Option<(String, Span)>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassMember {
    pub is_static: bool,
    pub kind: ClassMemberKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClassMemberKind {
    /// `count: number = 0;` (a type, a starting value, or both).
    Field {
        name: String,
        name_span: Span,
        ty: Option<TypeExpr>,
        init: Option<Box<Expr>>,
    },
    /// `name(params): T { ... }`
    Method {
        name: String,
        name_span: Span,
        params: Vec<Param>,
        ret: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    /// `constructor(params) { ... }`
    Constructor { params: Vec<Param>, body: Vec<Stmt> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectField {
    pub key: String,
    pub key_span: Span,
    pub value: Expr,
}

/// The body of an arrow or function expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FnBody {
    Block(Vec<Stmt>),
    /// `x => x + 1`
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Number(String),
    Str(String),
    Bool(bool),
    Null,
    Ident(String),
    /// `this` inside a class method or constructor.
    This,
    /// `super` in `super(...)` and `super.method(...)`.
    Super,
    /// A template string: `quasis` are the text parts around the `${}`
    /// expressions, so `quasis.len() == exprs.len() + 1`. A template without
    /// `${}` is a plain `Str`.
    Template {
        quasis: Vec<String>,
        exprs: Vec<Expr>,
    },
    /// ``tag`...` ``, for example ``L`...` `` and ``log.warn`...` ``.
    TaggedTemplate {
        raw_quasis: Vec<(String, Span)>,
        tag: Box<Expr>,
        quasis: Vec<String>,
        exprs: Vec<Expr>,
    },
    Array(Vec<Expr>),
    Object(Vec<ObjectField>),
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    /// `cond ? a : b`
    Conditional {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `x += v` and friends; `op` is the arithmetic operator (`+`).
    CompoundAssign {
        op: String,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `x++`, `x--` (`op` is `++` or `--`), only as a statement or a `for`
    /// update.
    Update {
        op: String,
        target: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    /// `a?.b`
    OptionalMember {
        object: Box<Expr>,
        name: String,
    },
    /// `a[i]`
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `new Map<number, string>()`
    New {
        class: String,
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
        /// One-based source line of `new`; `new Error` records it.
        line: usize,
    },
    /// `(a, b) => a + b`, `x => { ... }`, `function (a) { ... }`.
    Function {
        params: Vec<Param>,
        ret: Option<TypeExpr>,
        body: FnBody,
        is_arrow: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    /// Decoded string byte boundaries mapped to absolute source offsets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_offsets: Option<Vec<usize>>,
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
