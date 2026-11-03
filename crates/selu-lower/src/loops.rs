//! Control flow (package A in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these constructs (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_diagnostics::Span;
use selu_syntax::{Expr, Stmt};

impl Lowerer<'_> {
    pub(crate) fn loop_stmt(&mut self, s: &Stmt, out: &mut LBlock) {
        let _ = out;
        unreachable!(
            "loops are rejected by the checker until package A lands: {:?}",
            s.span
        )
    }

    /// `x += v`, `x++` as statements.
    pub(crate) fn control_stmt(&mut self, e: &Expr, span: &Span, out: &mut LBlock) {
        let _ = (out, e);
        unreachable!(
            "compound assignment is rejected by the checker until package A lands: {span:?}"
        )
    }

    /// `cond ? a : b`.
    pub(crate) fn control_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package A lands: {:?}",
            e.span
        )
    }
}
