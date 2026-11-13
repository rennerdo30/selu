//! Template identities travel with nominal widgets, including aliases/imports.
use crate::{Ctx, Type, api, err};
use selu_manifest::{ChildKey, Declaration, Manifest};
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
    if let Some(field) = m
        .member_fields
        .iter()
        .find(|f| f.owner == template && f.entry.name == name)
    {
        // A reviewed field of this mixin or template (assigned in Lua).
        result = Some(Some(&field.entry));
    } else if let Some(f) = own {
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
/// Whether any of these templates (or mixins) declares `name`.
pub(crate) fn template_declares(m: &Manifest, templates: &[&str], name: &str) -> bool {
    applied(m, templates, name).is_some()
}
fn applied<'a>(m: &'a Manifest, templates: &[&str], name: &str) -> Option<Option<&'a Declaration>> {
    templates
        .iter()
        .rev()
        .find_map(|t| effective(m, t, name, &mut Vec::new()))
}
/// The keyed children one element (a named frame, a template, or a keyed
/// child `<owner>.<key>`) records itself.
fn own_key<'a>(m: &'a Manifest, owner: &str, key: &str) -> Option<(&'a ChildKey, String)> {
    let entry = m.child_keys.iter().find(|c| c.owner == owner)?;
    let child = entry.children.iter().find(|c| c.key == key)?;
    Some((
        child,
        child
            .element
            .clone()
            .unwrap_or_else(|| format!("{owner}.{key}")),
    ))
}
/// A template's own keyed child, else the one it inherits; a later base
/// overrides an earlier one, as for template members.
fn template_key<'a>(
    m: &'a Manifest,
    template: &str,
    key: &str,
    path: &mut Vec<String>,
) -> Option<(&'a ChildKey, String)> {
    if path.iter().any(|s| s == template) {
        return None;
    }
    if let Some(found) = own_key(m, template, key) {
        return Some(found);
    }
    let d = m.templates.iter().find(|d| d.name == template)?;
    path.push(template.into());
    let found = d
        .inherits
        .iter()
        .rev()
        .find_map(|base| template_key(m, base, key, path));
    path.pop();
    found
}
/// `frame.<key>` for a frame whose XML attaches a child under that key
/// (`parentKey`): the child with its element identity and templates.
fn child_key<'a>(m: &'a Manifest, bases: &[String], key: &str) -> Option<(&'a ChildKey, String)> {
    let mut templates: Vec<&str> = Vec::new();
    for base in bases {
        if let Some(element) = base.strip_prefix("@element:") {
            if let Some(found) = own_key(m, element, key) {
                return Some(found);
            }
        } else if let Some(global) = base.strip_prefix("@global:") {
            if let Some(found) = own_key(m, global, key) {
                return Some(found);
            }
            if let Some(g) = m.globals.iter().find(|g| g.name == global) {
                templates.extend(g.templates.iter().map(String::as_str));
            }
        } else if let Some(template) = base.strip_prefix("@template:") {
            templates.push(template);
        }
    }
    templates
        .iter()
        .rev()
        .find_map(|t| template_key(m, t, key, &mut Vec::new()))
}
/// Adds `@has:<template>` for every template in `templates` and all the
/// templates they inherit. Member lookup keeps using the `@template:` tags
/// and their order; `@has:` only answers "does this frame carry template X".
pub(crate) fn tag_template_closure(m: &Manifest, bases: &mut Vec<String>) {
    fn visit(m: &Manifest, template: &str, out: &mut Vec<String>) {
        let tag = format!("@has:{template}");
        if out.contains(&tag) {
            return;
        }
        out.push(tag);
        if let Some(d) = m.templates.iter().find(|d| d.name == template) {
            for base in &d.inherits {
                visit(m, base, out);
            }
        }
    }
    let mut templates: Vec<String> = bases
        .iter()
        .filter_map(|s| s.strip_prefix("@template:").map(str::to_owned))
        .collect();
    if let Some(global) = bases.iter().find_map(|s| s.strip_prefix("@global:"))
        && let Some(g) = m.globals.iter().find(|g| g.name == global)
    {
        templates.extend(g.templates.iter().cloned());
    }
    let mut has = Vec::new();
    for template in &templates {
        visit(m, template, &mut has);
    }
    for tag in has {
        if !bases.contains(&tag) {
            bases.push(tag);
        }
    }
}
impl Ctx<'_> {
    /// The game's written type names an annotation uses: widgets (and their
    /// names without "API"), templates (the template's widget carrying it),
    /// documented structures (`TradeSkillRecipeInfo`) and documented mixin
    /// values (`colorRGBA`), in that order of precedence. Only the names in
    /// `wanted` are resolved: resolving every structure for every annotation
    /// made checking a large project take minutes.
    pub(crate) fn frame_type_names(
        &self,
        wanted: &std::collections::HashSet<String>,
    ) -> std::collections::HashMap<String, Type> {
        wanted
            .iter()
            .filter_map(|n| self.frame_type_name(n).map(|t| (n.clone(), t)))
            .collect()
    }
    fn frame_type_name(&self, n: &str) -> Option<Type> {
        let m = self.manifest;
        if m.widgets
            .iter()
            .any(|w| w.name == n || w.name.strip_suffix("API") == Some(n))
        {
            return Some(api::named(m, n, 0));
        }
        if let Some(template) = m.templates.iter().find(|t| t.name == n)
            && let Some(widget) = template.ty.as_deref()
        {
            let mut ty = api::named(m, widget, 0);
            if let Type::Widget(_, bases) = &mut ty {
                bases.push(format!("@template:{}", template.name));
                tag_template_closure(m, bases);
                return Some(ty);
            }
        }
        if m.tables
            .iter()
            .any(|t| t.name == n && t.ty.as_deref() == Some("Structure"))
            || api::MIXIN_VALUES.iter().any(|(v, _)| *v == n)
        {
            return Some(api::named(m, n, 0));
        }
        None
    }
    pub(crate) fn frame_global(&self, name: &str) -> Option<Type> {
        let g = &self.manifest.globals[*self.index.globals.get(name)?];
        let mut ty = api::named(self.manifest, g.ty.as_deref()?, 0);
        if let Type::Widget(_, bases) = &mut ty {
            bases.push(format!("@global:{name}"));
            bases.extend(g.templates.iter().map(|t| format!("@template:{t}")));
            tag_template_closure(self.manifest, bases);
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
        if let Type::Widget(_, bases) = &mut base {
            tag_template_closure(self.manifest, bases);
        }
        base
    }
    /// True when `name` is a keyed child (`parentKey`), a list
    /// (`parentArray`) or a reviewed Lua field of this widget on a selected
    /// target. Such members belong to the game and are read-only.
    pub(crate) fn keyed_child(&self, ty: &Type, name: &str) -> bool {
        let Type::Widget(_, bases) = ty else {
            return false;
        };
        let templates: Vec<_> = bases
            .iter()
            .filter_map(|s| s.strip_prefix("@template:"))
            .collect();
        let global = bases.iter().find_map(|s| s.strip_prefix("@global:"));
        self.availability
            .manifests()
            .iter()
            .filter(|m| self.target_is_active(m))
            .any(|m| {
                if child_key(m, bases, name).is_some() {
                    return true;
                }
                let names: Vec<_> = if let Some(g) = global {
                    m.globals
                        .iter()
                        .find(|v| v.name == g)
                        .map(|g| g.templates.iter().map(String::as_str).collect())
                        .unwrap_or_default()
                } else {
                    templates.clone()
                };
                matches!(
                    applied(m, &names, name),
                    Some(Some(d)) if d.ty.as_deref().is_some_and(|ty| ty != "Function")
                )
            })
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
        // A keyed child (`parentKey`) is typed and takes precedence over a
        // member a mixin only names.
        let keyed: Vec<_> = self
            .availability
            .manifests()
            .iter()
            .filter(|m| self.target_is_active(m))
            .map(|m| (m, child_key(m, bases, name)))
            .collect();
        if keyed.iter().any(|(_, child)| child.is_some()) {
            let missing: Vec<_> = keyed
                .iter()
                .filter(|(_, child)| child.is_none())
                .map(|(m, _)| m.target_string())
                .collect();
            if !missing.is_empty() {
                self.out.push(err("SELU0581", format!("child frame `{name}` is unavailable on {}", missing.join(", ")), &object.span)
                    .with_help("use child frames available on every selected target, or guard with has(...)"));
                return Some(Type::Error);
            }
            let (m, child) = keyed
                .iter()
                .find(|(m, _)| m.target_string() == self.target())
                .unwrap_or(&keyed[0]);
            let (child, element) = child.as_ref().expect("present on every target");
            let mut ty = api::named(m, &child.ty, 0);
            if let Type::Widget(_, child_bases) = &mut ty {
                // A list element has no single place in the XML tree.
                if !child.array {
                    child_bases.push(format!("@element:{element}"));
                }
                child_bases.extend(child.templates.iter().map(|t| format!("@template:{t}")));
                tag_template_closure(m, child_bases);
            }
            return Some(if child.array {
                Type::Array(Box::new(ty))
            } else {
                ty
            });
        }
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
                .map(|d| {
                    if d.ty.as_deref() == Some("Function") || d.ty.is_none() {
                        api::signature(m, d, 0)
                    } else {
                        // A reviewed field: its declared value type.
                        api::field_type(m, d)
                    }
                })
                .unwrap_or(Type::Untyped),
        )
    }
}
