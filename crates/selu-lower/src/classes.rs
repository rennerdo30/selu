//! Classes (package D3 in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these constructs (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_syntax::{ClassDecl, Expr, Stmt};

impl Lowerer<'_> {
    /// `forward`: the class name was already declared with `local` at the
    /// top of the block.
    pub(crate) fn class_decl(&mut self, s: &Stmt, c: &ClassDecl, forward: bool, out: &mut LBlock) {
        let _ = (c, forward, out);
        unreachable!(
            "rejected by the checker until package D3 lands: {:?}",
            s.span
        )
    }

    pub(crate) fn class_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package D3 lands: {:?}",
            e.span
        )
    }
}
