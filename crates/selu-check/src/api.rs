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
    if let Some(types) = d.attributes.get("SeluIterator").and_then(|v| v.as_array()) {
        return Type::ApiIterator(
            types
                .iter()
                .map(|t| named(m, t.as_str().unwrap_or(""), depth + 1))
                .collect(),
        );
    }
    if let Some(types) = d
        .attributes
        .get("SeluScalarUnion")
        .and_then(|v| v.as_array())
    {
        let t = Type::ApiOpaque(
            d.name.clone(),
            types
                .iter()
                .filter_map(|t| t.as_str().and_then(primitive))
                .collect(),
        );
        return if flag(d, "Nilable") {
            Type::optional(t)
        } else {
            t
        };
    }
    let name = d.ty.as_deref().unwrap_or("");
    let t = d
        .attributes
        .get("SeluOpaque")
        .and_then(|v| v.as_str())
        .map(|slot| Type::Opaque(slot.into()))
        .or_else(|| primitive(name))
        .unwrap_or_else(|| match name {
            "Function" | "function" => signature(m, d, depth + 1),
            "table" if d.attributes.contains_key("KeyType") => Type::Map(
                Box::new(named(
                    m,
                    d.attributes
                        .get("KeyType")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    depth + 1,
                )),
                Box::new(named(
                    m,
                    d.attributes
                        .get("InnerType")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    depth + 1,
                )),
            ),
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
// XML widget names are literal global keys, even when they contain dots.
// Only explicitly typed scalar entries authorize namespace-property lookup.
pub(crate) fn qualified_global(m: &Manifest, name: &str) -> Option<Type> {
    let global = m.globals.iter().find(|g| g.name == name)?;
    let ty = primitive(global.ty.as_deref()?)?;
    matches!(ty, Type::Number | Type::String | Type::Boolean).then_some(ty)
}

pub(crate) fn named(m: &Manifest, n: &str, depth: usize) -> Type {
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
        if let Some(types) = d.attributes.get("SeluUnion").and_then(|v| v.as_array()) {
            return Type::union(
                types
                    .iter()
                    .map(|t| named(m, t.as_str().unwrap_or(""), depth + 1))
                    .collect(),
            );
        }
        if d.ty.as_deref() == Some("table") {
            return map(m, d, depth + 1);
        }
        if d.attributes.contains_key("SeluScalarUnion") {
            return map(m, d, depth + 1);
        }
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
        let object = Type::Object(
            d.fields
                .iter()
                .map(|f| (f.name.clone(), map(m, f, depth + 1)))
                .collect(),
        );
        let prefix = if flag(d, "SeluClosed") {
            "@selu_closed"
        } else {
            "@manifest"
        };
        let shape = crate::types::NamedShape::new(format!("{prefix}:{n}"));
        shape.set(object);
        return Type::Named(n.into(), shape);
    }
    if let Some((_, members)) = SCALAR_UNIONS.iter().find(|(name, _)| *name == n) {
        return Type::ApiOpaque(n.into(), members.to_vec());
    }
    // A referenced name is not a definition. Missing type documentation must
    // not reject valid engine values based on a guessed nominal identity.
    Type::Untyped
}
fn argument_type(m: &Manifest, owner: &Declaration, argument: &Declaration, depth: usize) -> Type {
    let mapped = map(m, argument, depth);
    if owner.ty.as_deref() == Some("CallbackType") && mapped == Type::Untyped {
        let opaque = Type::Opaque(format!("{}.{}", owner.name, argument.name));
        if flag(argument, "Nilable") {
            Type::optional(opaque)
        } else {
            opaque
        }
    } else {
        mapped
    }
}

pub(crate) fn signature(m: &Manifest, d: &Declaration, depth: usize) -> Type {
    // A single trailing StrideIndex=1 argument is a homogeneous rest group.
    // Multi-field strides require tuple-group checking and remain fixed arity.
    let stride_rest = d
        .arguments
        .last()
        .filter(|a| {
            a.attributes.get("StrideIndex").and_then(|v| v.as_u64()) == Some(1)
                && d.arguments
                    .iter()
                    .filter(|a| a.attributes.contains_key("StrideIndex"))
                    .count()
                    == 1
        })
        .map(|a| a.name.as_str());
    let is_rest = |a: &Declaration| a.name == "..." || stride_rest == Some(a.name.as_str());
    let secret = crate::restrictions::secret_results(m, d);
    let mut returns: Vec<Type> = d
        .returns
        .iter()
        .zip(&secret)
        .map(|(r, secret)| {
            let t = map(m, r, depth + 1);
            if *secret { Type::secret(t) } else { t }
        })
        .collect();

    if flag(d, "MayReturnNothing") {
        returns = returns.into_iter().map(Type::optional).collect();
    }
    Type::Function {
        multiple_results: d.ty.as_deref() != Some("CallbackType"),
        unspecified_callback: false,
        api_semantics: crate::types::ApiSemantics {
            type_params: Vec::new(),
            opaque_inputs: d
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, argument)| {
                    let slot = argument.attributes.get("SeluOpaqueInput")?.as_str()?;
                    let value = Type::Opaque(slot.into());
                    Some((
                        index,
                        if flag(argument, "Nilable") {
                            Type::optional(value)
                        } else {
                            value
                        },
                    ))
                })
                .collect(),
            untyped_return_tail: flag(d, "SeluUntypedReturnTail"),
            typed_return_tail: d
                .attributes
                .get("SeluReturnTail")
                .and_then(|v| v.as_str())
                .map(|name| Box::new(Type::optional(named(m, name, depth + 1)))),
            script_callbacks: d
                .attributes
                .get("SeluScriptCallbacks")
                .and_then(|v| v.as_object())
                .map(|scripts| {
                    scripts
                        .iter()
                        .filter_map(|(name, value)| {
                            let declaration: Declaration =
                                serde_json::from_value(value.clone()).ok()?;
                            let mut callback = signature(m, &declaration, depth + 1);
                            if let Type::Function { ret, .. } = &mut callback {
                                **ret = Type::Void;
                            }
                            Some((name.clone(), callback))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            animation_factory: d.name == "CreateAnimation"
                && d.returns
                    .first()
                    .is_some_and(|r| r.ty.as_deref() == Some("SimpleAnim")),
            date_factory: d.name == "date"
                && d.returns
                    .first()
                    .is_some_and(|r| r.ty.as_deref() == Some("LuaDateResult")),
            native_callback: d.ty.as_deref() == Some("CallbackType"),
            pure_callback: flag(d, "SeluPureCallback"),
            table_operation: d
                .attributes
                .get("SeluTableOperation")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            frame_factory: false,
            pool_factory: flag(d, "SeluPoolFactory"),
            secret_results: crate::restrictions::secret_results(m, d),
            protected: flag(d, "IsProtectedFunction"),
            widget_protected: false,
            dynamic_protected: false,
            accepts_secrets: crate::restrictions::accepts_secrets(d),
            plain_result: false,
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
                !is_rest(a)
                    && (flag(a, "Required")
                        || (!flag(a, "Optional")
                            && !flag(a, "Nilable")
                            && !a.attributes.contains_key("Default")))
            })
            .map_or(0, |i| i + 1),
        params: d
            .arguments
            .iter()
            .filter(|a| !is_rest(a))
            .map(|a| argument_type(m, d, a, depth + 1))
            .collect(),
        rest: d
            .arguments
            .iter()
            .find(|a| is_rest(a))
            .map(|a| Box::new(argument_type(m, d, a, depth + 1))),
        ret: Box::new(Type::Tuple(returns)),
    }
}
pub(crate) fn signature_for(m: &Manifest, name: &str, d: &Declaration) -> Type {
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
pub(crate) fn widget_decl<'a>(m: &'a Manifest, name: &str) -> Option<&'a Declaration> {
    m.widgets
        .iter()
        .find(|w| w.name == name)
        .or_else(|| m.widgets.iter().find(|w| w.name == format!("{name}API")))
}
pub(crate) fn widget(m: &Manifest, name: &str) -> Type {
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
    // Runtime widget identity guards are available on the common object
    // returned by GetParent. Reuse the documented reflection signatures,
    // exposing no other frame methods until the guard proves a subtype.
    if widget == "SimpleObjectAPI" && matches!(name, "IsObjectType" | "GetObjectType") {
        return widget_decl(m, "SimpleFrameScriptObjectAPI")?
            .methods
            .iter()
            .find(|f| f.name == name);
    }
    w.methods
        .iter()
        .find(|f| f.name == name)
        .or_else(|| w.inherits.iter().find_map(|b| method(m, b, name, seen)))
}
fn widget_field<'a>(
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
    w.fields.iter().find(|f| f.name == name).or_else(|| {
        w.inherits
            .iter()
            .find_map(|b| widget_field(m, b, name, seen))
    })
}
impl Ctx<'_> {
    // Only immutable declarations borrowed from prepared manifests may be cached.
    fn cached_api_signature(&self, m: &Manifest, d: &Declaration, name: Option<&str>) -> Type {
        let key = (
            m as *const Manifest as usize,
            d as *const Declaration as usize,
            name.is_some(),
        );
        if let Some(ty) = self.api_types.borrow().get(&key) {
            return ty.clone();
        }
        let ty = match name {
            Some(name) => signature_for(m, name, d),
            None => signature(m, d, 0),
        };
        self.api_types.borrow_mut().insert(key, ty.clone());
        ty
    }

    pub(crate) fn api_global(&self, name: &str) -> Type {
        if let Some(t) = self.frame_global(name) {
            return t;
        }
        if let Some(f) = self.index.function(self.manifest, name) {
            return f
                .signature
                .as_ref()
                .map(|d| self.cached_api_signature(self.manifest, d, Some(&f.name)))
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
            && let Some(f) = self
                .index
                .function(self.manifest, &format!("{root}.{name}"))
        {
            self.types.mark_api_receiver(object.id);
            return Some(
                f.signature
                    .as_ref()
                    .map(|d| self.cached_api_signature(self.manifest, d, Some(&f.name)))
                    .unwrap_or(Type::Untyped),
            );
        }
        if let Some(root) = object.dotted_name()
            && self.lookup(root.split('.').next().unwrap_or("")).is_none()
            && let Some(ty) = qualified_global(self.manifest, &format!("{root}.{name}"))
        {
            return Some(ty);
        }
        let Type::Widget(widget, _) = t else {
            return None;
        };
        let declared = self
            .availability
            .manifests()
            .iter()
            .filter(|m| self.target_is_active(m))
            .find_map(|m| method(m, widget, name, &mut Vec::new()).map(|d| (m, d)));
        if let Some((m, d)) = declared {
            if let Some(missing) = self
                .availability
                .manifests()
                .iter()
                .filter(|m| self.target_is_active(m))
                .find(|m| method(m, widget, name, &mut Vec::new()).is_none())
            {
                self.out.push(err(
                    "SELU0581",
                    format!(
                        "widget member `{name}` is unavailable on {}",
                        missing.target_string()
                    ),
                    &object.span,
                ));
                return Some(Type::Error);
            }
            // The effective manifest carries combined secret/protection policy.
            // Use an active target only for declarations absent from that view.
            let (m, d) = method(self.manifest, widget, name, &mut Vec::new())
                .map(|d| (self.manifest, d))
                .unwrap_or((m, d));
            let mut ty = self.cached_api_signature(m, d, None);
            if let Type::Function { api_semantics, .. } = &mut ty {
                for callback in api_semantics.script_callbacks.values_mut() {
                    if let Type::Function { params, .. } = callback
                        && let Some(receiver) = params.first_mut()
                    {
                        *receiver = t.clone();
                    }
                }
                api_semantics.widget_protected = api_semantics.protected;
                let receiver = if self.types.is_method_call(object.id) {
                    match &object.kind {
                        ExprKind::Member { object, .. } => object.as_ref(),
                        _ => object,
                    }
                } else {
                    object
                };
                let stable_receiver = match &receiver.kind {
                    ExprKind::Ident(n) => self.lookup(n).is_some_and(|(_, b)| {
                        matches!(b.kind, crate::Kind::Const | crate::Kind::Import)
                    }),
                    ExprKind::Call { .. } => true,
                    _ => false,
                };
                if stable_receiver
                    && matches!(t, Type::Widget(_, bases) if bases.iter().any(|b| b == "@selu_plain"))
                {
                    api_semantics.protected = false;
                    api_semantics.widget_protected = false;
                    api_semantics.plain_result = d.name.starts_with("Create");
                }
            }
            return Some(ty);
        }
        let mut field_type = None;
        let active: Vec<_> = self
            .availability
            .manifests()
            .iter()
            .filter(|m| self.target_is_active(m))
            .collect();
        if active
            .iter()
            .any(|m| widget_field(m, widget, name, &mut Vec::new()).is_some())
        {
            for m in active {
                let Some(field) = widget_field(m, widget, name, &mut Vec::new()) else {
                    self.out.push(err(
                        "SELU0581",
                        format!(
                            "widget field `{name}` is unavailable on {}",
                            m.target_string()
                        ),
                        &object.span,
                    ));
                    return Some(Type::Error);
                };
                let ty = map(m, field, 0);
                if ty.is_open() {
                    self.out.push(err(
                        "SELU0582",
                        format!("widget field `{name}` has no checked type"),
                        &object.span,
                    ));
                    return Some(Type::Error);
                }
                field_type = match field_type {
                    None => Some(ty),
                    Some(old) => match Type::join(&old, &ty) {
                        Some(ty) => Some(ty),
                        None => {
                            self.out.push(err(
                                "SELU0581",
                                format!("widget field `{name}` has incompatible target types"),
                                &object.span,
                            ));
                            return Some(Type::Error);
                        }
                    },
                };
            }
            return field_type;
        }
        if let Some(member) = self.frame_member(t, name, object)
            && member != Type::Untyped
        {
            return Some(member);
        }
        self.out.push(err("SELU0582",format!("widget `{widget}` has no declared member `{name}` on {}",self.target()),&object.span).with_help("use a declared widget member or a typed frameFamily suffix lookup for a named child global"));
        Some(Type::Error)
    }

    pub(crate) fn api_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let full = callee.dotted_name()?;
        let root = full.split('.').next()?;
        if self.lookup(root).is_some() {
            return None;
        }
        if root.starts_with("C_") && self.index.function(self.manifest, &full).is_none() {
            self.missing(&full, callee.span.clone());
            for a in args {
                self.value(a);
            }
            return Some(Type::Error);
        }
        let f = self.index.function(self.manifest, &full)?.clone();
        if let ExprKind::Member { object, .. } = &callee.kind {
            self.types.mark_api_receiver(object.id);
        }
        let d = f.signature?;
        let ty = signature_for(self.manifest, &full, &d);
        self.types.set(callee.id, ty.clone());
        let ts = self.function_arguments(args, &ty);
        let ty = self.table_signature(&ty, ts.first(), ts.get(1), args.len());
        let ty = self.opaque_signature(&ty, &ts);
        self.types.set(callee.id, ty.clone());
        self.api_check_call(e, args, &ts, &ty);
        let Type::Function { ret, .. } = &ty else {
            unreachable!()
        };
        Some(self.api_result(e, args, &ty, (**ret).clone()))
    }
    pub(crate) fn api_result(&mut self, e: &Expr, args: &[Expr], ty: &Type, ret: Type) -> Type {
        if matches!(ty, Type::Function { api_semantics, .. } if api_semantics.animation_factory)
            && let Some(ExprKind::Str(kind)) = args.first().map(|e| &e.kind)
            && let Some(declaration) = self.manifest.widgets.iter().find(|w| {
                w.attributes
                    .get("SeluAnimationType")
                    .and_then(|v| v.as_str())
                    == Some(kind)
            })
        {
            let specialized = widget(self.manifest, &declaration.name);
            if let Type::Tuple(mut values) = ret {
                if let Some(first) = values.first_mut() {
                    *first = preserve_result_policy(first, specialized);
                }
                return Type::Tuple(values);
            }
            return preserve_result_policy(&ret, specialized);
        }
        if matches!(ty, Type::Function { api_semantics, .. } if api_semantics.date_factory)
            && (args.is_empty()
                || matches!(
                    args.first().map(|e| &e.kind),
                    Some(ExprKind::Str(_) | ExprKind::Null)
                ))
            && self
                .manifest
                .tables
                .iter()
                .any(|t| t.name == "LuaDateResult")
        {
            return Type::Tuple(vec![
                if matches!(args.first().map(|e| &e.kind), Some(ExprKind::Str(format)) if matches!(format.as_str(), "*t" | "!*t"))
                {
                    named(self.manifest, "LuaDateTable", 0)
                } else {
                    Type::String
                },
            ]);
        }
        if matches!(ty, Type::Function {api_semantics,..} if api_semantics.plain_result) {
            return crate::restrictions::plain_results(ret);
        }
        if matches!(ty,Type::Function { api_semantics, .. } if api_semantics.pool_factory) {
            if let Some(ExprKind::Str(name)) = args.first().map(|e| &e.kind)
                && widget_decl(self.manifest, name).is_some()
            {
                let prefix = match &ret {
                    Type::Tuple(values) => match values.first() {
                        Some(Type::Widget(pool, _)) => pool.strip_suffix("_Frame"),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(prefix) = prefix else {
                    return ret;
                };
                let specialized = format!("{prefix}_{name}");
                if widget_decl(self.manifest, &specialized).is_some() {
                    let mut result = match ret {
                        Type::Tuple(values) => values,
                        other => vec![other],
                    };
                    if let Some(first) = result.first_mut() {
                        *first = widget(self.manifest, &specialized);
                    }
                    return Type::Tuple(result);
                }
            }
            return ret;
        }
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
                        "unknown frame type `{name}` on {}; using Frame; did you mean `{}`?",
                        self.target(),
                        closest(name, self.manifest.widgets.iter().map(|w| w.name.as_str()))
                    ),
                )
                .with_span(e.span.clone()),
            );
            return Type::Tuple(vec![widget(self.manifest, "Frame")]);
        }
        let mut base = widget(self.manifest, name);
        let plain = args.get(3).is_none_or(|e| matches!(&e.kind, ExprKind::Str(s) if s.split(',').all(|t| !t.trim().starts_with("Secure"))));
        if plain && let Type::Widget(_, bases) = &mut base {
            bases.push("@selu_plain".into());
        }
        Type::Tuple(vec![self.frame_templates(base, args)])
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

    pub(crate) fn table_signature(
        &self,
        ty: &Type,
        first: Option<&Type>,
        second: Option<&Type>,
        count: usize,
    ) -> Type {
        let mut result = ty.clone();
        let Some(Type::Array(element)) = first.map(crate::shape_of) else {
            return result;
        };
        let Type::Function {
            params,
            required,
            ret,
            api_semantics,
            ..
        } = &mut result
        else {
            return result;
        };
        let Some(operation) = api_semantics.table_operation.as_deref() else {
            return result;
        };
        // Array contents are mutable even when the array binding is const.
        // Reuse the same identity/protection boundary as indexed reads.
        let element = Box::new(crate::restrictions::forget_plain(*element));
        let array = Type::Array(element.clone());
        match operation {
            "insert" => {
                *params = if count == 3 {
                    vec![array, Type::Number, (*element).clone()]
                } else {
                    vec![array, (*element).clone()]
                };
                *required = params.len();
                api_semantics.overloads.clear();
            }
            "remove" | "unpack" => {
                params[0] = array;
                let value = Type::optional((*element).clone());
                **ret = Type::Tuple(vec![value.clone()]);
                if operation == "unpack" {
                    api_semantics.untyped_return_tail = false;
                    api_semantics.typed_return_tail = Some(Box::new(value));
                }
            }
            "concat" => {
                if element.assignable_to(&named(self.manifest, "LuaStringNumber", 0)) {
                    params[0] = array;
                }
            }
            "sort" => {
                params[0] = if (count == 1 || second.is_some_and(Type::is_optional))
                    && !element.assignable_to(&Type::Number)
                    && !element.assignable_to(&Type::String)
                    && !element.is_open()
                {
                    Type::union(vec![
                        Type::Array(Box::new(Type::Number)),
                        Type::Array(Box::new(Type::String)),
                    ])
                } else {
                    array
                };
                let callback = Type::Function {
                    multiple_results: false,
                    unspecified_callback: false,
                    api: Default::default(),
                    api_semantics: crate::types::ApiSemantics {
                        native_callback: true,
                        ..Default::default()
                    },
                    params: vec![(*element).clone(), (*element).clone()],
                    required: 2,
                    rest: None,
                    ret: Box::new(Type::Boolean),
                };
                params[1] = Type::optional(callback);
            }
            _ => {}
        }
        result
    }

    pub(crate) fn script_signature(&self, args: &[Expr], ty: &Type) -> Type {
        let mut result = ty.clone();
        if let Type::Function {
            params,
            api_semantics,
            ..
        } = &mut result
            && let Some(ExprKind::Str(name)) = args.first().map(|e| &e.kind)
            && let Some(callback) = api_semantics.script_callbacks.get(name)
            && let Some(parameter) = params.get_mut(1)
        {
            *parameter = if parameter.non_null() != *parameter {
                Type::optional(callback.clone())
            } else {
                callback.clone()
            };
        }
        result
    }

    /// A proven native round trip admits only the matching opaque slot. Ordinary
    /// arguments still use the original input signature; this never widens it.
    pub(crate) fn opaque_signature(&self, ty: &Type, arguments: &[Type]) -> Type {
        let mut result = ty.clone();
        if let Type::Function {
            params,
            api_semantics,
            ..
        } = &mut result
        {
            for (index, slot) in &api_semantics.opaque_inputs {
                if arguments.get(*index).is_some_and(|argument| {
                    argument.contains_opaque() && argument.assignable_to(slot)
                }) && let Some(parameter) = params.get_mut(*index)
                {
                    *parameter = slot.clone();
                }
            }
        }
        result
    }

    pub(crate) fn api_check_call(
        &mut self,
        e: &Expr,
        args: &[Expr],
        ts: &[Type],
        ty: &Type,
    ) -> bool {
        let specialized = self.script_signature(args, ty);
        let ty = &specialized;
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
                                .is_some_and(|p| !api_argument_assignable(t, p, ty))
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
                && !api_argument_assignable(t, p, ty)
            {
                let parameter = origin
                    .parameters
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("...");
                let message = format!(
                    "parameter `{parameter}` of `{}` on {} expects {p}, got {t}",
                    origin.name, origin.target
                );
                // A known opaque boundary is a hard checking rule even when
                // incomplete ordinary API signatures are advisory.
                let diagnostic = if p.contains_opaque() || t.contains_opaque() {
                    err("SELU0481", message, &arg.span)
                        .with_help("ignore the payload, pass it to its matching native slot, or narrow it with typeof")
                } else {
                    self.api_diagnostic("SELU0481", message, &arg.span)
                };
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
    if crate::restrictions::secret_inner(have).is_some() {
        let mut result = selu_diagnostics::Diagnostic::error(
            "SELU0601",
            format!("{have} cannot be read as {want}"),
        )
        .with_help("use `value ?? fallback` or `readable(value)` before reading it");
        if let Some(span) = &d.span {
            result = result.with_span(span.clone());
        }
        return result;
    }
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

fn preserve_result_policy(original: &Type, replacement: Type) -> Type {
    match original {
        Type::Secret(inner) => Type::secret(preserve_result_policy(inner, replacement)),
        Type::Optional(inner) => Type::optional(preserve_result_policy(inner, replacement)),
        _ => replacement,
    }
}

fn api_argument_assignable(have: &Type, want: &Type, function: &Type) -> bool {
    if let Type::Function { api_semantics, .. } = function
        && api_semantics.table_operation.as_deref() == Some("insert")
    {
        if !api_semantics.accepts_secrets && crate::restrictions::secret_inner(have).is_some() {
            return false;
        }
        return have.assignable_to(want);
    }
    if matches!(function,Type::Function {api_semantics,..} if api_semantics.accepts_secrets)
        && let Some(inner) = crate::restrictions::secret_inner(have)
    {
        return api_assignable(&inner, want);
    }
    api_assignable(have, want)
}
fn api_assignable(have: &Type, want: &Type) -> bool {
    if let Type::Secret(inner) = crate::shape_of(want) {
        let have = crate::restrictions::secret_inner(have).unwrap_or_else(|| have.clone());
        return api_assignable(&have, &inner);
    }
    if have.assignable_to(want) {
        return true;
    }
    // Native Lua callbacks may ignore supplied trailing arguments. Every
    // parameter they consume remains contravariant, and required parameters
    // must be guaranteed by the native callback contract.
    let have_shape = crate::shape_of(have);
    let want_shape = crate::shape_of(&want.non_null());
    if let (
        Type::Function {
            params,
            required,
            rest,
            ret,
            ..
        },
        Type::Function {
            params: supplied,
            required: guaranteed,
            rest: supplied_rest,
            ret: expected_ret,
            ..
        },
    ) = (&have_shape, &want_shape)
    {
        return have_shape.result_convention_matches(&want_shape)
            && required <= guaranteed
            && params.iter().enumerate().all(|(i, parameter)| {
                supplied
                    .get(i)
                    .or(supplied_rest.as_deref())
                    .is_none_or(|value| value.assignable_to(parameter))
            })
            && rest.as_deref().is_none_or(|parameter| {
                supplied
                    .iter()
                    .skip(params.len())
                    .all(|value| value.assignable_to(parameter))
                    && supplied_rest
                        .as_deref()
                        .is_none_or(|value| value.assignable_to(parameter))
            })
            && (ret.assignable_to(expected_ret) || **expected_ret == Type::Void);
    }
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
