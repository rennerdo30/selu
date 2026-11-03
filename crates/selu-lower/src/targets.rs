//! Compile-time capability folding and shared-client runtime tests.
use crate::{
    Lowerer,
    lua::{self, BinOp, LBlock, LExpr, LStmt, LStmtKind, bin, name},
};
use selu_diagnostics::Span;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Stmt};
pub(crate) struct TargetLowering {
    pub manifest: Manifest,
    pub universe: Manifest,
    pub shared: bool,
}
impl Lowerer<'_> {
    pub(crate) fn target_prelude(&self, program: &selu_syntax::Program, out: &mut LBlock) {
        fn expr(l: &Lowerer<'_>, e: &Expr) -> bool {
            if let ExprKind::Call { callee, args } = &e.kind
                && matches!(l.has_call(callee,args), Some(LExpr::Call(ref f,_)) if **f == name("__selu_is_event_valid"))
            {
                return true;
            }
            let mut found = false;
            selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| found |= child(l, c));
            found
        }
        fn stmt(l: &Lowerer<'_>, s: &Stmt) -> bool {
            let mut found = false;
            selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| found |= child(l, c));
            found
        }
        fn child(l: &Lowerer<'_>, c: selu_syntax::visit::Child<'_>) -> bool {
            match c {
                selu_syntax::visit::Child::Expr(e) => expr(l, e),
                selu_syntax::visit::Child::Stmt(s) => stmt(l, s),
            }
        }
        if self.target.shared && program.stmts.iter().any(|s| stmt(self, s)) {
            out.push(LStmt {
                kind: LStmtKind::Local(
                    vec!["__selu_is_event_valid".into()],
                    vec![LExpr::Field(
                        Box::new(name("C_EventUtils")),
                        "IsEventValid".into(),
                    )],
                ),
                span: None,
            });
        }
    }

    pub(crate) fn has_call(&self, callee: &Expr, args: &[Expr]) -> Option<LExpr> {
        if callee.dotted_name().as_deref() != Some("has")
            || self.types.get(callee.id).is_some()
            || args.len() != 1
        {
            return None;
        }
        let n = args[0].dotted_name()?;
        if !self.target.shared {
            return Some(LExpr::Bool(selu_check::targets::exists(
                &self.target.manifest,
                &n,
            )));
        }
        if self.target.universe.event(&n).is_some() {
            return Some(lua::call(
                name("__selu_is_event_valid"),
                vec![LExpr::Str(n)],
            ));
        }
        let mut parts = n.split('.');
        let mut value = name(parts.next()?);
        let mut test = bin(BinOp::Ne, value.clone(), LExpr::Nil);
        for part in parts {
            value = LExpr::Field(Box::new(value), part.into());
            test = bin(BinOp::And, test, bin(BinOp::Ne, value.clone(), LExpr::Nil));
        }
        Some(test)
    }
    fn folded_has(&self, e: &Expr) -> Option<bool> {
        match &e.kind {
            ExprKind::Call { callee, args } => match self.has_call(callee, args)? {
                LExpr::Bool(b) => Some(b),
                _ => None,
            },
            ExprKind::Unary { op, expr } if op == "!" => self.folded_has(expr).map(|b| !b),
            _ => None,
        }
    }
    pub(crate) fn target_if(
        &mut self,
        cond: &Expr,
        yes: &[Stmt],
        no: Option<&[Stmt]>,
        span: &Span,
        out: &mut LBlock,
    ) -> bool {
        let Some(b) = self.folded_has(cond) else {
            return false;
        };
        let body = self.block(if b { yes } else { no.unwrap_or_default() });
        if !body.is_empty() {
            out.push(LStmt::new(LStmtKind::Do(body), span));
        }
        true
    }
}
pub fn generate_shared_toc(addon: &str, manifests: &[Manifest], files: &[String]) -> String {
    let mut toc = crate::generate_toc(addon, &manifests[0], files);
    let interfaces = manifests
        .iter()
        .map(|m| m.interface.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let targets = manifests
        .iter()
        .map(Manifest::target_string)
        .collect::<Vec<_>>()
        .join(", ");
    toc = toc.replacen(
        &format!("## Interface: {}", manifests[0].interface),
        &format!("## Interface: {interfaces}"),
        1,
    );
    toc.replacen(
        &format!("## X-Selu-Target: {}", manifests[0].target_string()),
        &format!("## X-Selu-Target: {targets}"),
        1,
    )
}

pub(crate) fn contains_event(s: &Stmt) -> bool {
    if matches!(s.kind, selu_syntax::StmtKind::OnHandler { .. }) {
        return true;
    }
    let mut found = false;
    selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| match c {
        selu_syntax::visit::Child::Stmt(s) => found |= contains_event(s),
        selu_syntax::visit::Child::Expr(e) => found |= expr_event(e),
    });
    found
}
fn expr_event(e: &Expr) -> bool {
    let mut found = false;
    selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| match c {
        selu_syntax::visit::Child::Stmt(s) => found |= contains_event(s),
        selu_syntax::visit::Child::Expr(e) => found |= expr_event(e),
    });
    found
}
