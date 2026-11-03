//! Functions (package C in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these constructs (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_syntax::{Expr, Stmt};

impl Lowerer<'_> {
    pub(crate) fn function_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package C lands: {:?}",
            e.span
        )
    }

    pub(crate) fn destructure(&mut self, s: &Stmt, out: &mut LBlock) {
        let _ = out;
        unreachable!(
            "rejected by the checker until package C lands: {:?}",
            s.span
        )
    }
}
