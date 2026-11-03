//! WoW features (package D4 in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects tagged templates (SELU0390) and treats built-in names as
//! ordinary names.

use crate::Lowerer;
use crate::lua::{LBlock, LExpr};
use selu_syntax::Expr;

impl Lowerer<'_> {
    pub(crate) fn tagged_template(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let _ = pre;
        unreachable!(
            "rejected by the checker until package D4 lands: {:?}",
            e.span
        )
    }

    /// A call to one of Selu's built-ins, or `None` for any other call.
    pub(crate) fn builtin_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> Option<LExpr> {
        let _ = (callee, args, pre);
        None
    }
}
