//! WoW features (package D, later in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these constructs (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_syntax::Expr;

impl Lowerer<'_> {
    pub(crate) fn tagged_template(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package D lands: {:?}",
            e.span
        )
    }
}
