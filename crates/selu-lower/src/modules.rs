//! Modules (package D1 in docs/WORK-PACKAGES.md). Until it lands, the
//! checker rejects these statements (SELU0390), so the lowering never sees
//! them.

use crate::Lowerer;
use crate::lua::LBlock;
use selu_syntax::Stmt;

impl Lowerer<'_> {
    pub(crate) fn import_stmt(&mut self, s: &Stmt, out: &mut LBlock) {
        let _ = out;
        unreachable!(
            "rejected by the checker until package D1 lands: {:?}",
            s.span
        )
    }

    /// `export <declaration>`. `forward`: the declared name was already
    /// declared with `local` at the top of the block.
    pub(crate) fn export_stmt(&mut self, s: &Stmt, inner: &Stmt, forward: bool, out: &mut LBlock) {
        let _ = (inner, forward, out);
        unreachable!(
            "rejected by the checker until package D1 lands: {:?}",
            s.span
        )
    }
}
