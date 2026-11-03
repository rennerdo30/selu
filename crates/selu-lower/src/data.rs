//! Data (package B in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these constructs (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_syntax::Expr;

impl Lowerer<'_> {
    pub(crate) fn data_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package B lands: {:?}",
            e.span
        )
    }

    /// An assignment target other than a name or field (`xs[i] = v`).
    pub(crate) fn data_target(&mut self, target: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package B lands: {:?}",
            target.span
        )
    }
}
