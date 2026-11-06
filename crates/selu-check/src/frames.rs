//! Template identities travel with nominal widgets, including aliases/imports.
use crate::{Ctx, Type, api, err};
use selu_manifest::{Declaration, Manifest};
use selu_syntax::{Expr, ExprKind};

// Outer Some means the member exists. Inner None is an untyped override and
// must stop lookup just as a documented signature does. Paths, not a global
// visited set, allow a later application of the same ancestor to override.
fn effective<'a>(
    m: &'a Manifest,
    template: &str,
    name: &str,
    path: &mut Vec<String>,
) -> Option<Option<&'a Declaration>> {
    if path.iter().any(|s| s == template) {
        return None;
    }
    let d = m
        .templates
        .iter()
        .chain(&m.data_globals)
        .find(|d| d.name == template)?;
    path.push(template.into());
    let mut result = None;
    for base in &d.inherits {
        if let Some(value) = effective(m, base, name, path) {
            result = Some(value);
        }
    }
    let mut mixin = None;
    for base in &d.mixins {
        if let Some(value) = effective(m, base, name, path) {
            mixin = Some(value);
        }
    }
    if mixin.is_some() {
        result = mixin;
    }
    let own = m.function(&format!("{template}.{name}"));
    if let Some(f) = own {
        result = Some(f.signature.as_ref());
    } else if d.fields.iter().chain(&d.methods).any(|s| s == name) {
        // Template name lists include their direct mixin projection. Preserve
        // that mixin's signature; Lua data entries list their own members.
        result = if m.templates.iter().any(|t| std::ptr::eq(t, d)) {
            mixin.or(Some(None))
        } else {
            Some(None)
        };
    }
    path.pop();
    result
}
fn applied<'a>(m: &'a Manifest, templates: &[&str], name: &str) -> Option<Option<&'a Declaration>> {
    templates
        .iter()
        .rev()
        .find_map(|t| effective(m, t, name, &mut Vec::new()))
}
impl Ctx<'_> {
    pub(crate) fn frame_type_names(&self) -> std::collections::HashMap<String, Type> {
        self.manifest
            .widgets
            .iter()
            .flat_map(|w| {
                let mut names = vec![w.name.clone()];
                if let Some(n) = w.name.strip_suffix("API") {
                    names.push(n.into());
                }
                names.into_iter().map(|n| {
                    let ty = api::named(self.manifest, &n, 0);
                    (n, ty)
                })
            })
            .collect()
    }
    pub(crate) fn frame_global(&self, name: &str) -> Option<Type> {
        let g = &self.manifest.globals[*self.index.globals.get(name)?];
        let mut ty = api::named(self.manifest, g.ty.as_deref()?, 0);
        if let Type::Widget(_, bases) = &mut ty {
            bases.push(format!("@global:{name}"));
            bases.extend(g.templates.iter().map(|t| format!("@template:{t}")));
        }
        Some(ty)
    }
    pub(crate) fn frame_templates(&mut self, mut base: Type, args: &[Expr]) -> Type {
        let Some(arg) = args.get(3) else {
            return base;
        };
        let ExprKind::Str(list) = &arg.kind else {
            return base;
        };
        let mut offset = 0;
        for entry in list.split(',') {
            let name = entry.trim();
            let start = offset + entry.len() - entry.trim_start().len();
            offset += entry.len() + 1;
            if name.is_empty() {
                continue;
            }
            let known = self
                .availability
                .manifests()
                .iter()
                .any(|m| m.templates.iter().any(|t| t.name == name));
            if !known {
                let mut span = arg.span.clone();
                if let Some(map) = &arg.string_offsets {
                    span.start = map[start];
                    span.end = map[start + name.len()];
                }
                self.out.push(
                    selu_diagnostics::Diagnostic::warning(
                        "SELU0580",
                        format!("unknown frame template `{name}` on {}", self.target()),
                    )
                    .with_span(span)
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
        let present: Vec<_> = self
            .availability
            .manifests()
            .iter()
            .filter(|m| self.target_is_active(m))
            .map(|m| {
                let names: Vec<_> = if let Some(g) = global {
                    m.globals
                        .iter()
                        .find(|v| v.name == g)
                        .map(|g| g.templates.iter().map(String::as_str).collect())
                        .unwrap_or_default()
                } else {
                    templates.clone()
                };
                (m, applied(m, &names, name))
            })
            .collect();
        if !present.iter().any(|(_, member)| member.is_some()) {
            // M3 records global names, not parentKey relationships. A name
            // prefix proves only a global exists, never a field on its parent.
            if let Some((prefix, suffix)) = bases
                .iter()
                .find_map(|s| s.strip_prefix("@family:").and_then(|s| s.split_once('\t')))
            {
                let child_suffix = format!("{suffix}{name}");
                if present.iter().any(|(m, _)| {
                    m.frame_families
                        .iter()
                        .any(|f| f.prefix == prefix && f.suffix == child_suffix)
                        || m.globals.iter().any(|g| {
                            g.name
                                .strip_prefix(prefix)
                                .and_then(|s| s.strip_suffix(&child_suffix))
                                .is_some_and(|n| {
                                    !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit())
                                })
                        })
                }) {
                    self.out.push(err("SELU0582", format!("`{prefix}...{child_suffix}` is recorded as a global, not a `{name}` field"), &object.span).with_help(format!("use frameFamily(\"{prefix}\", index, \"{child_suffix}\")")));
                    return Some(Type::Error);
                }
            }
            if let Some(g) = global {
                let child = format!("{g}{name}");
                if present
                    .iter()
                    .any(|(m, _)| m.globals.iter().any(|g| g.name == child))
                {
                    self.out.push(
                        err(
                            "SELU0582",
                            format!("`{child}` is recorded as a global, not a `{name}` field"),
                            &object.span,
                        )
                        .with_help(format!("use the global `{child}` directly")),
                    );
                    return Some(Type::Error);
                }
            }
            return None;
        }
        let missing: Vec<_> = present
            .iter()
            .filter(|(_, member)| member.is_none())
            .map(|(m, _)| m.target_string())
            .collect();
        if !missing.is_empty() {
            self.out.push(err("SELU0581", format!("template member `{name}` is unavailable on {}", missing.join(", ")), &object.span)
                .with_help("use templates and members available on every selected target, or guard with has(...)"));
            return Some(Type::Error);
        }
        // Use the active primary's signature, or an active fallback when this
        // target's branch is unreachable. Untyped overrides remain untyped.
        let (m, member) = present
            .iter()
            .find(|(m, _)| m.target_string() == self.target())
            .unwrap_or(&present[0]);
        Some(
            member
                .flatten()
                .map(|d| api::signature(m, d, 0))
                .unwrap_or(Type::Untyped),
        )
    }
}
