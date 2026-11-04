//! Template identities travel with nominal widgets, including aliases/imports.
use crate::{Ctx, Type, api, err};
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind};

fn member(m: &Manifest, template: &str, name: &str, seen: &mut Vec<String>) -> bool {
    if seen.iter().any(|s| s == template) {
        return false;
    }
    seen.push(template.into());
    let Some(d) = m
        .templates
        .iter()
        .chain(&m.data_globals)
        .find(|d| d.name == template)
    else {
        return false;
    };
    d.fields.iter().chain(&d.methods).any(|s| s == name)
        || d.inherits
            .iter()
            .chain(&d.mixins)
            .any(|b| member(m, b, name, seen))
}

fn documented_member<'a>(
    m: &'a Manifest,
    template: &str,
    name: &str,
    seen: &mut Vec<String>,
) -> Option<&'a selu_manifest::Declaration> {
    if seen.iter().any(|s| s == template) {
        return None;
    }
    seen.push(template.into());
    if let Some(d) = m
        .function(&format!("{template}.{name}"))
        .and_then(|f| f.signature.as_ref())
    {
        return Some(d);
    }
    let d = m
        .templates
        .iter()
        .chain(&m.data_globals)
        .find(|d| d.name == template)?;
    d.inherits
        .iter()
        .chain(&d.mixins)
        .find_map(|b| documented_member(m, b, name, seen))
}

impl Ctx<'_> {
    pub(crate) fn frame_global(&self, name: &str) -> Option<Type> {
        let g = self.manifest.globals.iter().find(|g| g.name == name)?;
        let mut ty = api::named(self.manifest, g.ty.as_deref()?, 0);
        if let Type::Widget(_, bases) = &mut ty {
            bases.push(format!("@global:{name}"));
            bases.extend(g.templates.iter().map(|t| format!("@template:{t}")));
        }
        Some(ty)
    }

    pub(crate) fn frame_templates(&mut self, mut base: Type, args: &[Expr]) -> Type {
        let Some(Expr {
            kind: ExprKind::Str(list),
            span,
            ..
        }) = args.get(3)
        else {
            return base;
        };
        for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let known = self
                .availability
                .manifests()
                .iter()
                .any(|m| m.templates.iter().any(|t| t.name == name));
            if !known {
                self.out.push(
                    selu_diagnostics::Diagnostic::warning(
                        "SELU0580",
                        format!("unknown frame template `{name}` on {}", self.target()),
                    )
                    .with_span(span.clone())
                    .with_help(
                        "check the template spelling; addon templates may be absent from manifests",
                    ),
                );
            }
            if let Type::Widget(_, bases) = &mut base {
                bases.push(format!("@template:{name}"));
            }
        }
        base
    }

    pub(crate) fn frame_member(&mut self, ty: &Type, name: &str, object: &Expr) -> Option<Type> {
        let Type::Widget(_, bases) = ty else {
            return None;
        };
        let templates: Vec<_> = bases
            .iter()
            .filter_map(|s| s.strip_prefix("@template:"))
            .collect();
        let global = bases.iter().find_map(|s| s.strip_prefix("@global:"));
        let targets = self.availability.manifests();
        let present: Vec<_> = targets
            .iter()
            .map(|m| {
                let child =
                    global.and_then(|g| m.globals.iter().find(|c| c.name == format!("{g}{name}")));
                let has = if let Some(global) = global {
                    m.globals
                        .iter()
                        .find(|g| g.name == global)
                        .is_some_and(|g| {
                            g.templates
                                .iter()
                                .any(|t| member(m, t, name, &mut Vec::new()))
                        })
                } else {
                    templates
                        .iter()
                        .any(|t| member(m, t, name, &mut Vec::new()))
                };
                (m, child, has)
            })
            .collect();
        if !present
            .iter()
            .any(|(_, child, has)| child.is_some() || *has)
        {
            return None;
        }
        let missing: Vec<_> = present
            .iter()
            .filter(|(_, c, h)| c.is_none() && !h)
            .map(|(m, _, _)| m.target_string())
            .collect();
        if !missing.is_empty() {
            self.out.push(
                err(
                    "SELU0581",
                    format!(
                        "template member `{name}` is unavailable on {}",
                        missing.join(", ")
                    ),
                    &object.span,
                )
                .with_help("use templates and members available on every selected target"),
            );
            return Some(Type::Error);
        }
        if let Some((_, Some(child), _)) = present
            .iter()
            .find(|(m, _, _)| m.target_string() == self.target())
        {
            return self.frame_global(&child.name);
        }
        // M3 members carry names only. A separately documented signature is
        // usable, but source evidence alone never supplies a guessed signature.
        let full = templates
            .iter()
            .find_map(|t| documented_member(self.manifest, t, name, &mut Vec::new()));
        Some(
            full.map(|d| api::signature(self.manifest, d, 0))
                .unwrap_or(Type::Untyped),
        )
    }
}
