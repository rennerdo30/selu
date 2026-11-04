use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind};
impl Ctx<'_> {
    pub(crate) fn binding_call(&mut self, e: &Expr, args: &[Expr]) -> Type {
        if !(3..=5).contains(&args.len()) {
            self.out.push(
                err(
                    "SELU0660",
                    "A keybinding needs a name, label, callback, optional header and runOnUp.",
                    &e.span,
                )
                .with_help(r#"use keybinding("Addon_ACTION", "Action", () => {})"#),
            );
            return Type::Error;
        }
        if !matches!(&args[0].kind, ExprKind::Str(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && !s.as_bytes()[0].is_ascii_digit())
        {
            self.out.push(err("SELU0661", "A binding name must be a literal identifier using ASCII letters, digits and underscores.", &args[0].span).with_help("prefix the name with your addon name and an underscore"));
        }
        for i in [1, 3] {
            if let Some(x) = args.get(i)
                && self.value(x) != Type::String
            {
                self.out.push(
                    err(
                        "SELU0663",
                        "A binding label or header must be a string.",
                        &x.span,
                    )
                    .with_help("use a string or an L template"),
                );
            }
        }
        let t = self.value(&args[2]);
        if !matches!(t, Type::Function { required: 0, .. }) {
            self.out.push(
                err(
                    "SELU0664",
                    "A binding callback must be callable without arguments.",
                    &args[2].span,
                )
                .with_help("use () => { ... }"),
            );
        }
        if let Some(x) = args.get(4)
            && !matches!(x.kind, ExprKind::Bool(_))
        {
            self.out.push(
                err("SELU0665", "runOnUp must be a boolean literal.", &x.span)
                    .with_help("use true or false"),
            );
        }
        Type::Void
    }
}
