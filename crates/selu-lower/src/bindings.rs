use crate::{
    Lowerer,
    lua::{LBlock, LExpr, LStmt, LStmtKind, name},
};
use selu_syntax::{Expr, ExprKind};
#[derive(Debug, Clone)]
pub struct Binding {
    pub span: selu_diagnostics::Span,
    pub name: String,
    pub header: bool,
    pub run_on_up: bool,
}
pub fn xml(bindings: &[Binding]) -> String {
    let mut out = String::from("<Bindings>\n");
    for b in bindings {
        let header = if b.header {
            format!(" header=\"{}\"", b.name)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  <Binding name=\"{}\"{} runOnUp=\"{}\">\n    {}_Binding()\n  </Binding>\n",
            b.name,
            header,
            if b.run_on_up { "true" } else { "false" },
            b.name
        ));
    }
    out.push_str("</Bindings>\n");
    out
}
impl Lowerer<'_> {
    pub(crate) fn binding_call(&mut self, args: &[Expr], pre: &mut LBlock) -> LExpr {
        let ExprKind::Str(id) = &args[0].kind else {
            unreachable!()
        };
        let label = self.expr(&args[1], pre);
        pre.push(LStmt::new(
            LStmtKind::Assign(vec![name(&format!("BINDING_NAME_{id}"))], vec![label]),
            &args[1].span,
        ));
        let callback = self.expr(&args[2], pre);
        pre.push(LStmt::new(
            LStmtKind::Assign(vec![name(&format!("{id}_Binding"))], vec![callback]),
            &args[2].span,
        ));
        if let Some(header) = args.get(3) {
            let label = self.expr(header, pre);
            pre.push(LStmt::new(
                LStmtKind::Assign(vec![name(&format!("BINDING_HEADER_{id}"))], vec![label]),
                &header.span,
            ));
        }
        self.wow.bindings.push(Binding {
            span: args[0].span.clone(),
            name: id.clone(),
            header: args.len() >= 4,
            run_on_up: matches!(args.get(4).map(|x| &x.kind), Some(ExprKind::Bool(true))),
        });
        LExpr::Nil
    }
}
