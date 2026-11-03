//! Manifest signatures and widget inheritance, shared by direct and aliased calls.
use crate::{Ctx, Type, err};
use selu_manifest::{Declaration, Manifest};
use selu_syntax::{Expr, ExprKind};

fn closest<'a>(name: &str, choices: impl Iterator<Item = &'a str>) -> String {
    choices
        .min_by_key(|s| {
            let mut row: Vec<usize> = (0..=s.chars().count()).collect();
            for (i, a) in name.chars().enumerate() {
                let mut prev = row[0];
                row[0] = i + 1;
                for (j, b) in s.chars().enumerate() {
                    let old = row[j + 1];
                    row[j + 1] = (row[j] + 1).min(old + 1).min(prev + usize::from(a != b));
                    prev = old;
                }
            }
            *row.last().unwrap()
        })
        .unwrap_or("Frame")
        .into()
}
// Scalar-union spellings are metadata, shared by argument and return checking.
const SCALAR_UNIONS: &[(&str, &[Type])] = &[
    ("SpellIdentifier", &[Type::Number, Type::String]),
    ("uiAddon", &[Type::Number, Type::String]),
];
fn flag(d: &Declaration, key: &str) -> bool {
    d.attributes.get(key).and_then(|v| v.as_bool()) == Some(true)
}
fn map(m: &Manifest, d: &Declaration, depth: usize) -> Type {
    if depth > 12 {
        return Type::Untyped;
    }
    let name = d.ty.as_deref().unwrap_or("");
    let t = primitive(name).unwrap_or_else(|| match name {
        "Function" | "function" => signature(m, d, depth + 1),
        "table" => d
            .attributes
            .get("InnerType")
            .and_then(|v| v.as_str())
            .map(|n| Type::Array(Box::new(named(m, n, depth + 1))))
            .unwrap_or(Type::Untyped),
        n => named(m, n, depth + 1),
    });
    if flag(d, "Nilable") {
        Type::optional(t)
    } else {
        t
    }
}
fn primitive(name: &str) -> Option<Type> {
    Some(match name {
        "LuaFunctionReference" => Type::Function {
            multiple_results: false,
            unspecified_callback: true,
            api: Default::default(),
            params: Vec::new(),
            required: 0,
            rest: Some(Box::new(Type::Untyped)),
            ret: Box::new(Type::Untyped),
        },
        "bool" | "boolean" => Type::Boolean,
        "string" | "cstring" | "WOWGUID" | "UnitToken" | "UnitTokenVariant" | "FramePoint"
        | "DrawLayer" => Type::String,
        "textureAtlas"
        | "textureKit"
        | "stringView"
        | "UnitTokenNamePlate"
        | "UnitTokenPvPRestrictedForAddOns"
        | "UnitTokenRestrictedForAddOns"
        | "mouseButton"
        | "MouseButton"
        | "JustifyHorizontal"
        | "JustifyVertical"
        | "FrameStrata"
        | "ScriptTypeName"
        | "BlendMode"
        | "FontAlphabet"
        | "HTMLTextType"
        | "Orientation"
        | "InsertMode"
        | "LoopType"
        | "FilterMode"
        | "SimpleButtonStateToken"
        | "ClickButton" => Type::String,
        "number" | "luaIndex" | "uiUnit" | "WOWMONEY" | "fileID" | "Seconds" | "BigUInteger"
        | "BigInteger" | "time_t" | "Milliseconds" | "normalizedValue" | "SingleColorValue"
        | "uiFontHeight" | "size" => Type::Number,
        "nil" => Type::Null,
        _ => return None,
    })
}
fn named(m: &Manifest, n: &str, depth: usize) -> Type {
    if let Some(t) = primitive(n) {
        return t;
    }
    if depth > 12 {
        return Type::Untyped;
    }
    if widget_decl(m, n).is_some() {
        return widget(m, n);
    }
    if m.enums.iter().any(|e| e.name == n) {
        return Type::Number;
    }
    if let Some(d) = m.tables.iter().find(|t| t.name == n) {
        if !d.arguments.is_empty() || d.ty.as_deref() == Some("CallbackType") {
            let mut t = signature(m, d, depth + 1);
            if d.returns.is_empty()
                && let Type::Function { ret, .. } = &mut t
            {
                **ret = Type::Void;
            }
            return t;
        }
        if !matches!(d.ty.as_deref(), Some("Structure" | "Constants")) {
            return Type::ApiOpaque(n.into(), Vec::new());
        }
        return Type::Object(
            d.fields
                .iter()
                .map(|f| (f.name.clone(), map(m, f, depth + 1)))
                .collect(),
        );
    }
    if let Some((_, members)) = SCALAR_UNIONS.iter().find(|(name, _)| *name == n) {
        return Type::ApiOpaque(n.into(), members.to_vec());
    }
    // A referenced name is not a definition. Missing type documentation must
    // not reject valid engine values based on a guessed nominal identity.
    Type::Untyped
}
fn signature(m: &Manifest, d: &Declaration, depth: usize) -> Type {
    let mut returns: Vec<Type> = d.returns.iter().map(|r| map(m, r, depth + 1)).collect();
    if flag(d, "MayReturnNothing") {
        returns = returns.into_iter().map(Type::optional).collect();
    }
    Type::Function {
        multiple_results: d.ty.as_deref() != Some("CallbackType"),
        unspecified_callback: false,
        api: crate::types::ApiOrigin(Some(crate::types::ApiSignature {
            name: d
                .namespace
                .as_ref()
                .map(|n| format!("{n}.{}", d.name))
                .unwrap_or_else(|| d.name.clone()),
            parameters: d.arguments.iter().map(|a| a.name.clone()).collect(),
            target: m.target_string(),
        })),
        required: d
            .arguments
            .iter()
            .rposition(|a| {
                !flag(a, "Nilable") && !a.attributes.contains_key("Default") && a.name != "..."
            })
            .map_or(0, |i| i + 1),
        params: d
            .arguments
            .iter()
            .filter(|a| a.name != "...")
            .map(|a| map(m, a, depth + 1))
            .collect(),
        rest: d
            .arguments
            .iter()
            .find(|a| a.name == "...")
            .map(|a| Box::new(map(m, a, depth + 1))),
        ret: Box::new(Type::Tuple(returns)),
    }
}
fn signature_for(m: &Manifest, name: &str, d: &Declaration) -> Type {
    let mut ty = signature(m, d, 0);
    if let Type::Function { api, .. } = &mut ty
        && let Some(origin) = &mut api.0
    {
        origin.name = name.into();
    }
    ty
}
fn widget_decl<'a>(m: &'a Manifest, name: &str) -> Option<&'a Declaration> {
    m.widgets
        .iter()
        .find(|w| w.name == name)
        .or_else(|| m.widgets.iter().find(|w| w.name == format!("{name}API")))
}
fn widget(m: &Manifest, name: &str) -> Type {
    fn bases(m: &Manifest, name: &str, out: &mut Vec<String>) {
        if let Some(w) = widget_decl(m, name) {
            for b in &w.inherits {
                if !out.contains(b) {
                    out.push(b.clone());
                    bases(m, b, out);
                }
            }
        }
    }
    let mut inherited = Vec::new();
    bases(m, name, &mut inherited);
    Type::Widget(name.into(), inherited)
}
fn method<'a>(
    m: &'a Manifest,
    widget: &str,
    name: &str,
    seen: &mut Vec<String>,
) -> Option<&'a Declaration> {
    if seen.iter().any(|s| s == widget) {
        return None;
    }
    seen.push(widget.into());
    let w = widget_decl(m, widget)?;
    w.methods
        .iter()
        .find(|f| f.name == name)
        .or_else(|| w.inherits.iter().find_map(|b| method(m, b, name, seen)))
}
impl Ctx<'_> {
    pub(crate) fn api_global(&self, name: &str) -> Type {
        if let Some(f) = self.manifest.function(name) {
            return f
                .signature
                .as_ref()
                .map(|d| signature_for(self.manifest, &f.name, d))
                .unwrap_or(Type::Untyped);
        }
        if name == "Enum" {
            return Type::Object(
                self.manifest
                    .enums
                    .iter()
                    .map(|e| {
                        (
                            e.name.clone(),
                            Type::Object(
                                e.values.keys().map(|k| (k.clone(), Type::Number)).collect(),
                            ),
                        )
                    })
                    .collect(),
            );
        }
        self.manifest
            .globals
            .iter()
            .find(|g| g.name == name)
            .and_then(|g| g.ty.as_deref())
            .map(|n| named(self.manifest, n, 0))
            .unwrap_or(Type::Untyped)
    }
    pub(crate) fn api_member(&mut self, t: &Type, name: &str, object: &Expr) -> Option<Type> {
        if let Some(root) = object.dotted_name()
            && self.lookup(root.split('.').next().unwrap_or("")).is_none()
            && let Some(f) = self.manifest.function(&format!("{root}.{name}"))
        {
            return Some(
                f.signature
                    .as_ref()
                    .map(|d| signature_for(self.manifest, &f.name, d))
                    .unwrap_or(Type::Untyped),
            );
        }
        let Type::Widget(widget, _) = t else {
            return None;
        };
        if let Some(d) = method(self.manifest, widget, name, &mut Vec::new()) {
            return Some(signature(self.manifest, d, 0));
        }
        self.out.push(err(
            "SELU0482",
            format!(
                "widget `{widget}` has no method `{name}` on {}; did you mean `{}`?",
                self.target(),
                closest(
                    name,
                    self.manifest
                        .widgets
                        .iter()
                        .flat_map(|w| w.methods.iter().map(|m| m.name.as_str()))
                )
            ),
            &object.span,
        ));
        Some(Type::Error)
    }
    pub(crate) fn api_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let full = callee.dotted_name()?;
        let root = full.split('.').next()?;
        if self.lookup(root).is_some() {
            return None;
        }
        if root.starts_with("C_") && self.manifest.function(&full).is_none() {
            self.missing(&full, callee.span.clone());
            for a in args {
                self.value(a);
            }
            return Some(Type::Error);
        }
        let f = self.manifest.function(&full)?.clone();
        let d = f.signature?;
        let ty = signature_for(self.manifest, &full, &d);
        self.types.set(callee.id, ty.clone());
        let ts = self.function_arguments(args, &ty);
        let Type::Function { ret, .. } = ty else {
            unreachable!()
        };
        let check_ty = signature_for(self.manifest, &full, &d);
        self.api_check_call(e, args, &ts, &check_ty);
        if full == "CreateFrame" {
            let widget_name = match args.first().map(|e| &e.kind) {
                Some(ExprKind::Str(s)) => s.as_str(),
                _ => "Frame",
            };
            if !self.manifest.widgets.iter().any(|w| w.name == widget_name) {
                self.out.push(err(
                    "SELU0483",
                    format!(
                        "unknown widget `{widget_name}` on {}; did you mean `{}`?",
                        self.target(),
                        closest(
                            widget_name,
                            self.manifest.widgets.iter().map(|w| w.name.as_str())
                        )
                    ),
                    &e.span,
                ));
                return Some(Type::Error);
            }
            return Some(Type::Tuple(vec![widget(self.manifest, widget_name)]));
        }
        Some(*ret)
    }
    pub(crate) fn api_check_call(
        &mut self,
        e: &Expr,
        args: &[Expr],
        ts: &[Type],
        ty: &Type,
    ) -> bool {
        let Type::Function {
            params,
            required,
            rest,
            api,
            ..
        } = ty
        else {
            return false;
        };
        let Some(origin) = &api.0 else {
            return false;
        };
        if args.len() < *required || (rest.is_none() && args.len() > params.len()) {
            self.out.push(err(
                "SELU0480",
                format!(
                    "`{}` on {} takes {required}..{} arguments, got {}",
                    origin.name,
                    origin.target,
                    params.len(),
                    args.len()
                ),
                &e.span,
            ));
        }
        for (i, (t, arg)) in ts.iter().zip(args).enumerate() {
            if let Some(p) = params.get(i).or(rest.as_deref())
                && !t.assignable_to(p)
            {
                let parameter = origin
                    .parameters
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("...");
                self.out.push(err(
                    "SELU0481",
                    format!(
                        "parameter `{parameter}` of `{}` on {} expects {p}, got {t}",
                        origin.name, origin.target
                    ),
                    &arg.span,
                ));
            }
        }
        true
    }
    pub(crate) fn api_event_types(
        &mut self,
        entry: &selu_manifest::EventEntry,
        params: &[selu_syntax::Param],
        span: &selu_diagnostics::Span,
    ) -> Vec<Type> {
        let payload = entry
            .signature
            .as_ref()
            .map(|d| {
                d.arguments
                    .iter()
                    .map(|p| map(self.manifest, p, 0))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                entry
                    .params
                    .iter()
                    .map(|p| crate::types::manifest_type(&p.ty))
                    .collect()
            });
        if params.len() > payload.len() {
            self.out.push(err(
                "SELU0484",
                format!(
                    "event `{}` on {} has {} payload parameters, got {}",
                    entry.name,
                    self.target(),
                    payload.len(),
                    params.len()
                ),
                span,
            ));
        }
        payload
    }
}

pub(crate) fn mismatch_help(
    d: selu_diagnostics::Diagnostic,
    have: &Type,
    want: &Type,
) -> selu_diagnostics::Diagnostic {
    if matches!(
        (have, want),
        (
            Type::Function {
                multiple_results: true,
                ..
            },
            Type::Function {
                multiple_results: false,
                unspecified_callback: false,
                ..
            }
        )
    ) {
        let (name, parameters) = match have {
            Type::Function { api, .. } => api
                .0
                .as_ref()
                .map(|origin| (origin.name.clone(), origin.parameters.join(", ")))
                .unwrap_or_else(|| ("f".into(), "arg".into())),
            _ => unreachable!(),
        };
        let mut diagnostic = selu_diagnostics::Diagnostic::error(
            "SELU0485",
            "A function with multiple results cannot be used as a single-result function.",
        )
        .with_fix(format!("wrap it: `({parameters}) => {name}({parameters})`"));
        if let Some(span) = &d.span {
            diagnostic = diagnostic.with_span(span.clone());
        }
        let d = diagnostic;
        return d;
    }
    crate::classes::mismatch_help(d, have, want)
}
