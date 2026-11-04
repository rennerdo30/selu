use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind, Program, StmtKind};

// Only a direct expression statement at module level can register an XML entry.
// Checking ids also rejects calls nested inside a module-level initializer.
pub(crate) fn module_statements(
    program: &Program,
) -> std::collections::HashSet<selu_syntax::ExprId> {
    program
        .stmts
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Expr(e) => Some(e.id),
            _ => None,
        })
        .collect()
}

impl Ctx<'_> {
    pub(crate) fn binding_call(&mut self, e: &Expr, args: &[Expr]) -> Type {
        if !self.binding_statements.contains(&e.id) {
            self.out.push(err("SELU0668", "A keybinding must be a top-level module statement.", &e.span).with_help("move the keybinding call outside functions, handlers, conditionals and other expressions"));
        }
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
        if let ExprKind::Str(id) = &args[0].kind {
            let mut public_names = vec![format!("{id}_Binding"), format!("BINDING_NAME_{id}")];
            if args.len() >= 4 {
                public_names.push(format!("BINDING_HEADER_{id}"));
            }
            for name in public_names {
                if self.lookup(&name).is_some() {
                    self.out.push(
                        err(
                            "SELU0667",
                            format!(
                                "Source name `{name}` collides with a generated binding global."
                            ),
                            &args[0].span,
                        )
                        .with_help(format!(
                            "rename the source declaration `{name}` or choose another binding name"
                        )),
                    );
                }
            }
        }
        for i in [1, 3] {
            if let Some(x) = args.get(i)
                && crate::shape_of(&self.value(x)) != Type::String
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
        let t = crate::shape_of(&self.value(&args[2]));
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
