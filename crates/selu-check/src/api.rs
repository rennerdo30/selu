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
            api_semantics: Default::default(),
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
    let secret = crate::restrictions::secret_results(m, d);
    let mut returns: Vec<Type> = d
        .returns
        .iter()
        .zip(&secret)
        .map(|(r, secret)| {
            let t = map(m, r, depth + 1);
            if *secret { Type::optional(t) } else { t }
        })
        .collect();
    if flag(d, "MayReturnNothing") {
        returns = returns.into_iter().map(Type::optional).collect();
    }
    Type::Function {
        multiple_results: d.ty.as_deref() != Some("CallbackType"),
        unspecified_callback: false,
        api_semantics: crate::types::ApiSemantics {
            frame_factory: false,
            secret_results: crate::restrictions::secret_results(m, d),
            protected: flag(d, "IsProtectedFunction"),
            overloads: d
                .overloads
                .iter()
                .map(|o| signature(m, o, depth + 1))
                .collect(),
        },
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
    if name == "CreateFrame"
        && let Type::Function { api_semantics, .. } = &mut ty
    {
        api_semantics.frame_factory = true;
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
    if let Some(alias) = widget_decl(m, name)
        .and_then(|w| w.attributes.get("SeluAliasOf"))
        .and_then(|v| v.as_str())
    {
        return widget(m, alias);
    }
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
        let mut candidates = Vec::new();
        fn collect(m: &Manifest, n: &str, out: &mut Vec<String>, seen: &mut Vec<String>) {
            if seen.iter().any(|s| s == n) {
                return;
            }
            seen.push(n.into());
            if let Some(w) = widget_decl(m, n) {
                out.extend(w.methods.iter().map(|m| m.name.clone()));
                for b in &w.inherits {
                    collect(m, b, out, seen);
                }
            }
        }
        collect(self.manifest, widget, &mut candidates, &mut Vec::new());
        let suggestion = closest(name, candidates.iter().map(String::as_str));
        if !candidates.is_empty() && (1..=2).contains(&edit_distance(name, &suggestion)) {
            self.out.push(
                selu_diagnostics::Diagnostic::warning(
                    "SELU0482",
                    format!(
                        "unknown widget member `{name}` on {}; did you mean `{suggestion}`?",
                        self.target()
                    ),
                )
                .with_span(object.span.clone()),
            );
        }
        Some(Type::Untyped)
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
        self.api_check_call(e, args, &ts, &ty);
        let Type::Function { ret, .. } = &ty else {
            unreachable!()
        };
        Some(self.api_result(e, args, &ty, (**ret).clone()))
    }
    pub(crate) fn api_result(&mut self, e: &Expr, args: &[Expr], ty: &Type, ret: Type) -> Type {
        if !matches!(ty,Type::Function { api_semantics, .. } if api_semantics.frame_factory) {
            return ret;
        }
        let name = match args.first().map(|e| &e.kind) {
            Some(ExprKind::Str(s)) => s.as_str(),
            _ => "Frame",
        };
        if widget_decl(self.manifest, name).is_none() {
            self.out.push(
                selu_diagnostics::Diagnostic::warning(
                    "SELU0483",
                    format!(
                        "unknown frame type `{name}` on {}; using open Frame; did you mean `{}`?",
                        self.target(),
                        closest(name, self.manifest.widgets.iter().map(|w| w.name.as_str()))
                    ),
                )
                .with_span(e.span.clone()),
            );
            return Type::Tuple(vec![widget(self.manifest, "Frame")]);
        }
        Type::Tuple(vec![widget(self.manifest, name)])
    }
    fn api_diagnostic(
        &self,
        code: &str,
        message: String,
        span: &selu_diagnostics::Span,
    ) -> selu_diagnostics::Diagnostic {
        if self.options.strict_api {
            err(code, message, span)
        } else {
            selu_diagnostics::Diagnostic::warning(code, message).with_span(span.clone())
        }
    }

    pub(crate) fn api_check_call(
        &mut self,
        e: &Expr,
        args: &[Expr],
        ts: &[Type],
        ty: &Type,
    ) -> bool {
        let candidates = std::iter::once(ty).chain(match ty {
            Type::Function { api_semantics, .. } => api_semantics.overloads.iter(),
            _ => [].iter(),
        });
        let score = |ty: &Type| {
            if let Type::Function {
                params,
                required,
                rest,
                ..
            } = ty
            {
                let arity = required.saturating_sub(args.len())
                    + if rest.is_none() {
                        args.len().saturating_sub(params.len())
                    } else {
                        0
                    };
                arity
                    + ts.iter()
                        .enumerate()
                        .filter(|(i, t)| {
                            params
                                .get(*i)
                                .or(rest.as_deref())
                                .is_some_and(|p| !api_assignable(t, p))
                        })
                        .count()
            } else {
                usize::MAX
            }
        };
        let ty = candidates.min_by_key(|t| score(t)).unwrap_or(ty);
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
            self.out.push(self.api_diagnostic(
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
                && !api_assignable(t, p)
            {
                let parameter = origin
                    .parameters
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("...");
                let diagnostic = self.api_diagnostic(
                    "SELU0481",
                    format!(
                        "parameter `{parameter}` of `{}` on {} expects {p}, got {t}",
                        origin.name, origin.target
                    ),
                    &arg.span,
                );
                let diagnostic = if let Some(fix) = result_wrapper_fix(t, &p.non_null()) {
                    diagnostic.with_fix(fix)
                } else {
                    diagnostic
                };
                self.out.push(diagnostic);
            }
        }
        true
    }
    pub(crate) fn api_event_types(
        &mut self,
        entry: &selu_manifest::EventEntry,
        params: &[selu_syntax::Param],
        _span: &selu_diagnostics::Span,
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
                ) + &format!("; unexpected parameter `{}`", params[payload.len()].name),
                &params[payload.len()].span,
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
    if let Some(fix) = result_wrapper_fix(have, want) {
        let mut diagnostic = selu_diagnostics::Diagnostic::error(
            "SELU0485",
            "A function with multiple results cannot be used as a single-result function.",
        )
        .with_fix(fix);
        if let Some(span) = &d.span {
            diagnostic = diagnostic.with_span(span.clone());
        }
        return diagnostic;
    }
    crate::classes::mismatch_help(d, have, want)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let mut row: Vec<usize> = (0..=b.chars().count()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, y) in b.chars().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j] + 1).min(old + 1).min(prev + usize::from(x != y));
            prev = old;
        }
    }
    *row.last().unwrap()
}
fn api_assignable(have: &Type, want: &Type) -> bool {
    have.assignable_to(want)
        || matches!(
            (crate::shape_of(have), crate::shape_of(&want.non_null())),
            (Type::Number, Type::String)
        )
}

fn result_wrapper_fix(have: &Type, want: &Type) -> Option<String> {
    let hs = crate::shape_of(have);
    let ws = crate::shape_of(want);
    let have = &hs;
    let want = &ws;
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
                .map(|o| (o.name.clone(), o.parameters.join(", ")))
                .unwrap_or_else(|| ("f".into(), "arg".into())),
            _ => unreachable!(),
        };
        Some(format!("wrap it: `({parameters}) => {name}({parameters})`"))
    } else {
        None
    }
}
