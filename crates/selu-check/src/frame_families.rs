//! Closed manifest-backed numbered frame lookups.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind};
impl Ctx<'_> {
    pub(crate) fn frame_family(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        if !matches!(&callee.kind, ExprKind::Ident(n) if n == "frameFamily" && self.lookup(n).is_none())
        {
            return None;
        }
        self.types.mark_frame_family(e.id);
        for a in args {
            self.expr(a);
        }
        if !(2..=3).contains(&args.len()) {
            self.out.push(err(
                "SELU0646",
                "frameFamily expects a family, index and optional suffix",
                &e.span,
            ));
            return Some(Type::Error);
        }
        let ExprKind::Str(prefix) = &args[0].kind else {
            self.out.push(err(
                "SELU0642",
                "frame family must be a string literal",
                &args[0].span,
            ));
            return Some(Type::Error);
        };
        let suffix = if args.len() == 3 {
            let ExprKind::Str(s) = &args[2].kind else {
                self.out.push(err(
                    "SELU0642",
                    "frame suffix must be a string literal",
                    &args[2].span,
                ));
                return Some(Type::Error);
            };
            s.as_str()
        } else {
            ""
        };
        if self.types.get(args[1].id) != Some(&Type::Number) {
            self.out.push(err(
                "SELU0643",
                "frame index must be a number",
                &args[1].span,
            ));
        }
        let mut result = None;
        for m in self.availability.manifests() {
            let Some(f) = m
                .frame_families
                .iter()
                .find(|f| f.prefix == *prefix && f.suffix == suffix)
            else {
                let code = if m.frame_families.iter().any(|f| f.prefix == *prefix) {
                    "SELU0641"
                } else if self
                    .availability
                    .manifests()
                    .iter()
                    .any(|t| t.frame_families.iter().any(|f| f.prefix == *prefix))
                {
                    "SELU0644"
                } else {
                    "SELU0640"
                };
                self.out.push(err(
                    code,
                    format!(
                        "unknown frame family or suffix `{prefix}...{suffix}` on {}",
                        m.target_string()
                    ),
                    &e.span,
                ));
                continue;
            };
            let literal = match &args[1].kind {
                ExprKind::Number(n) => n.parse::<f64>().ok(),
                ExprKind::Unary { op, expr } if op == "-" => match &expr.kind {
                    ExprKind::Number(n) => n.parse::<f64>().ok().map(|n| -n),
                    _ => None,
                },
                _ => None,
            };
            if literal.is_some_and(|n| {
                n < f.min_index as f64 || n > f.max_index as f64 || n.fract() != 0.0
            }) {
                self.out.push(err(
                    "SELU0645",
                    format!(
                        "frame index outside {}..{} on {}",
                        f.min_index,
                        f.max_index,
                        m.target_string()
                    ),
                    &args[1].span,
                ));
            }
            let ty = crate::api::named(m, &f.widget, 0);
            result = Some(match result {
                None => ty,
                Some(old) if old == ty => old,
                Some(_) => {
                    self.out.push(err(
                        "SELU0647",
                        "frame widget types differ between targets",
                        &e.span,
                    ));
                    Type::Error
                }
            });
        }
        Some(Type::optional(result.unwrap_or(Type::Error)))
    }
}
