use super::{arena::*, layout};
use crate::core::{Ctx, DrawLayer, Strata, TextureFill};
use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};
use std::{cell::RefCell, rc::Rc};
type CreateFrameArgs = (
    Option<String>,
    Option<String>,
    Option<Table>,
    Option<String>,
    Option<i32>,
);
fn number(v: Option<&Value>, default: f64) -> f64 {
    match v {
        Some(Value::Number(n)) => *n,
        Some(Value::Integer(n)) => *n as f64,
        _ => default,
    }
}
fn string(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    }
}
fn boolean(v: Option<&Value>) -> bool {
    !matches!(v, None | Some(Value::Nil) | Some(Value::Boolean(false)))
}
fn sync_button_textures(lua: &Lua, ctx: &Ctx, button: &Table) -> mlua::Result<()> {
    let enabled = store(lua).borrow().regions[id(button)?].enabled;
    let disabled = button.raw_get::<Option<Table>>("__selu_DisabledTexture")?;
    for (key, shown) in [
        ("__selu_NormalTexture", enabled || disabled.is_none()),
        ("__selu_DisabledTexture", !enabled),
    ] {
        if let Some(texture) = button.raw_get::<Option<Table>>(key)? {
            call(
                lua,
                ctx,
                if shown { "Show" } else { "Hide" },
                MultiValue::from_vec(vec![Value::Table(texture)]),
            )?;
        }
    }
    Ok(())
}
pub fn callback(ctx: &Ctx, t: Table, name: &str, args: Vec<Value>) {
    if matches!(name, "OnShow" | "OnHide" | "OnSizeChanged")
        && t.raw_get::<bool>("__selu_xml_constructing")
            .unwrap_or(false)
    {
        return;
    }
    if matches!(name, "OnShow" | "OnHide")
        && t.raw_get::<bool>("__selu_initial_visibility")
            .unwrap_or(false)
    {
        return;
    }
    // A transition without a handler must not retain the frame until the
    // outer dispatch finishes (large addon construction can create thousands).
    if !matches!(
        t.raw_get::<Table>("__scripts")
            .and_then(|scripts| scripts.raw_get::<Value>(name)),
        Ok(Value::Function(_))
    ) {
        return;
    }
    let deferred = name == "OnSizeChanged";
    let name = name.to_owned();
    let ctx2 = ctx.clone();
    let pending: crate::core::Pending = Box::new(move |lua| {
        if let Some(f) = t
            .raw_get::<Table>("__scripts")?
            .raw_get::<Option<Function>>(name.clone())?
        {
            ctx2.record(crate::TraceKind::Script, &name, vec![]);
            let mut call = MultiValue::from_vec(vec![Value::Table(t)]);
            call.extend(args);
            crate::core::script_callback(lua, &ctx2, || crate::core::call_lua::<()>(lua, f, call))?;
        }
        Ok(())
    });
    if deferred {
        ctx.queue(pending);
    } else {
        ctx.scripts.lock().unwrap().push_back(pending);
    }
}
pub fn install(lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
    lua.set_app_data(ctx.clone());
    lua.set_app_data(Rc::new(RefCell::new(Arena::default())) as Store);
    lua.globals().raw_set(
        "__selu_child_regions",
        lua.create_function(|lua, (parent, regions): (Table, bool)| {
            let _profile = crate::profile::Span::new("frames.enumerate_children");
            let parent = id(&parent)?;
            let list = lua.create_table()?;
            let arena = store(lua);
            let arena = arena.borrow();
            let mut index = 1;
            for child in &arena.regions {
                if child.parent == Some(parent) && is_region(&child.kind) == regions {
                    list.raw_set(index, child.table.get())?;
                    index += 1;
                }
            }
            Ok(list)
        })?,
    )?;
    let ctx = ctx.clone();
    lua.globals().set(
        "CreateFrame",
        lua.create_function(
            move |lua, (kind, name, parent, templates, frame_id): CreateFrameArgs| {
                let mut kind = kind.unwrap_or_else(|| "Frame".into());
                let mut templates = templates;
                if let Some(resolve) = lua
                    .globals()
                    .raw_get::<Option<Function>>("__selu_xml_intrinsic")?
                    && let Some(base) = resolve.call::<Option<String>>(kind.clone())?
                {
                    templates = Some(match templates {
                        Some(names) => format!("{kind},{names}"),
                        None => kind.clone(),
                    });
                    kind = base;
                }
                let t = create(lua, &ctx, &kind, name, parent)?;
                if let Some(frame_id) = frame_id {
                    store(lua).borrow_mut().regions[id(&t)?].id = frame_id;
                }
                crate::regions::templates(lua, &t, templates.as_deref())?;
                if templates.as_deref().is_some_and(|s| s.contains("Secure")) {
                    ctx.not_simulated("secure templates");
                }
                let registry: Table = lua.globals().get("__selu_sim_frames")?;
                registry.raw_set(registry.raw_len() + 1, t.clone())?;
                Ok(t)
            },
        )?,
    )?;
    lua.globals().set(
        "GetCursorPosition",
        lua.create_function(|lua, ()| Ok(store(lua).borrow().cursor))?,
    )?;
    lua.globals().set(
        "__selu_sim_visible",
        lua.create_function(|lua, t: Table| {
            let a = store(lua);
            let mut a = a.borrow_mut();
            let ctx = lua.app_data_ref::<Ctx>().unwrap().clone();
            layout::resolve(&mut a, &ctx);
            Ok(a.regions[id(&t)?].visible)
        })?,
    )?;
    Ok(())
}
pub fn create(
    lua: &Lua,
    ctx: &Ctx,
    kind: &str,
    name: Option<String>,
    parent: Option<Table>,
) -> mlua::Result<Table> {
    let _profile = crate::profile::Span::new("frames.create");
    let kind = [
        "Frame",
        "Button",
        "CheckButton",
        "EditBox",
        "Slider",
        "StatusBar",
        "ScrollFrame",
        "Minimap",
        "GameTooltip",
        "Model",
        "PlayerModel",
        "DressUpModel",
        "SimpleHTML",
        "Cooldown",
        "Texture",
        "FontString",
        "MaskTexture",
    ]
    .into_iter()
    .find(|candidate| candidate.eq_ignore_ascii_case(kind))
    .unwrap_or(kind);
    let a = store(lua);
    let index = a.borrow().regions.len();
    let parent = parent.or_else(|| {
        lua.globals()
            .raw_get::<Option<Table>>("UIParent")
            .ok()
            .flatten()
            .filter(|table| id(table).is_ok())
    });
    let pid = parent.as_ref().map(id).transpose()?;
    let name = name.map(|s| {
        s.replace(
            "$parent",
            pid.and_then(|p| a.borrow().regions[p].name.clone())
                .as_deref()
                .unwrap_or(""),
        )
    });
    let t = lua.create_table()?;
    t.raw_set(identity_key(), index)?;
    t.raw_set("__protected", false)?;
    t.raw_set("__kind", kind)?;
    t.raw_set("__scripts", lua.create_table()?)?;
    t.raw_set("__events", lua.create_table()?)?;
    let (strata, level) = pid
        .map(|p| {
            let a = a.borrow();
            (a.regions[p].strata, a.regions[p].level + 1)
        })
        .unwrap_or((Strata::Medium, 0));
    let initial_visible = a.borrow().layout_batches == 0;
    a.borrow_mut().regions.push(Region {
        table: RegionTable::new(lua, &t)?,
        kind: kind.into(),
        name: name.clone(),
        parent: pid,
        width: if index == 0 { 1920.0 } else { 0.0 },
        height: if index == 0 { 1080.0 } else { 0.0 },
        scale: 1.0,
        alpha: 1.0,
        shown: true,
        strata,
        level,
        layer: (DrawLayer::Artwork, 0),
        points: vec![],
        rect: None,
        visible: initial_visible,
        effective_scale: 1.0,
        effective_alpha: 1.0,
        text: None,
        texture: None,
        atlas_uv: None,
        visual: crate::core::VisualStyle::default(),
        font_overrides: 0,
        clips_children: false,
        scroll_child: None,
        scroll: (0.0, 0.0),
        animation_offset: (0.0, 0.0),
        font_size: 12.0,
        multi_line: false,
        font_path: "fixed".into(),
        font_flags: String::new(),
        text_value: Value::Nil,
        strata_explicit: false,
        id: 0,
        level_explicit: false,
        mouse: matches!(kind, "Button" | "CheckButton" | "EditBox" | "Slider"),
        enabled: true,
        keyboard: kind == "EditBox",
        propagate_keyboard: false,
        wheel: false,
        thumb: None,
        status_texture: None,
        step: 0.0,
        obey_step: false,
        orientation: if kind == "StatusBar" {
            "HORIZONTAL"
        } else {
            "VERTICAL"
        }
        .into(),
        movable: false,
        clamped: false,
        drag_buttons: vec![],
        click_buttons: vec!["LeftButtonUp".into()],
        motion_disabled: false,
        auto_focus: true,
        cursor_position: 0,
        selection: (0, 0),
        moving: false,
        checked: false,
        value: 0.0,
        min: 0.0,
        max: 1.0,
        created_at: lua.inspect_stack(1).and_then(|d| {
            let line = d.curr_line();
            let source = d.source();
            if line > 0 {
                source.source.map(|chunk| crate::SourcePos {
                    chunk: chunk.trim_start_matches('@').to_owned(),
                    line: line as u32,
                })
            } else {
                None
            }
        }),
    });
    {
        let mut a = a.borrow_mut();
        a.dirty = true;
        a.dirty_regions.insert(index);
        if index == 0 {
            a.full_layout = true;
        }
    }
    // Shared method table per type. Missing addon fields remain nil.
    let key = format!("selu.methods.{kind}");
    let methods = if let Ok(m) = lua.named_registry_value::<Table>(&key) {
        m
    } else {
        let m = lua.create_table()?;
        for method in METHODS.split_whitespace().chain([
            "GetAttribute",
            "SetAttribute",
            "SetStatusBarTexture",
            "GetStatusBarTexture",
            "SetStatusBarColor",
            "GetStatusBarColor",
            "GetNormalTexture",
            "GetAtlas",
            "GetScaledRect",
            "SetNormalAtlas",
            "SetPushedAtlas",
            "SetHighlightAtlas",
            "SetDisabledAtlas",
            "SetUnit",
            "GetUnit",
            "SetGUIDMatch",
            "SetClampRectInsets",
            "SetAllowOverlappedModels",
            "SetIndentedWordWrap",
            "IsOwned",
            "GetViewInsets",
            "GetHighlightTexture",
            "GetPushedTexture",
            "GetDisabledTexture",
            "SetDisabledTexture",
            "SetFontString",
            "GetFontString",
            "SetNormalFontObject",
            "SetHighlightFontObject",
            "SetDisabledFontObject",
            "SetFormattedText",
        ]) {
            let method = method.to_owned();
            let context = ctx.clone();
            m.set(
                method.clone(),
                lua.create_function(move |lua, args: MultiValue| {
                    let result = call(lua, &context, &method, args);
                    crate::core::flush_scripts(lua, &context)?;
                    crate::core::check_budget(&context)?;
                    result
                })?,
            )?;
        }
        let enumerate = crate::host_lua::factory(
            lua,
            "selu.helper.enumerate",
            "return function(regions,count) return function(self) local children=__selu_child_regions(self,regions); if count then return #children end; return unpack(children) end end",
        )?;
        for (name, regions, count) in [
            ("GetChildren", false, false),
            ("GetRegions", true, false),
            ("GetNumChildren", false, true),
            ("GetNumRegions", true, true),
        ] {
            m.raw_set(name, enumerate.call::<Function>((regions, count))?)?;
        }
        lua.set_named_registry_value(&key, m.clone())?;
        m
    };
    let methods = RegionTable::new(lua, &methods)?;
    let mt = lua.create_table()?;
    let context = ctx.clone();
    let widget = kind.to_owned();
    mt.set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, String)| {
            let _profile = crate::profile::Span::new("frames.method_lookup");
            let known = methods.raw_get::<Value>(key.clone())?;
            if !known.is_nil() {
                return Ok(known);
            }
            let catalog = lua.app_data_ref::<super::MethodCatalog>();
            let fallback =
                super::missing_method(&widget, &key, catalog.as_deref().and_then(|c| c.0.as_ref()));
            drop(catalog);
            if let Some(fallback) = fallback {
                let context = context.clone();
                Ok(Value::Function(lua.create_function(
                    move |lua, _: MultiValue| {
                        context.not_simulated(&fallback.target);
                        let mut out = MultiValue::new();
                        for c in fallback.empty_shape.chars() {
                            out.push_back(match c {
                                'b' => Value::Boolean(false),
                                'd' => Value::Number(0.0),
                                's' => Value::String(lua.create_string("")?),
                                't' => Value::Table(lua.create_table()?),
                                _ => Value::Nil,
                            });
                        }
                        Ok(out)
                    },
                )?))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    t.set_metatable(Some(mt));
    if let Some(name) = name {
        lua.globals().set(name, t.clone())?;
    }
    if kind == "GameTooltip" {
        let initialize =
            crate::host_lua::factory(lua, "selu.helper.tooltip", include_str!("tooltip.lua"))?;
        initialize.call::<()>(t.clone())?;
    }
    Ok(t)
}
const METHODS: &str = "SetDesaturated IsDesaturated IsMovable IsClampedToScreen SetMotionScriptsWhileDisabled SetCursorPosition GetCursorPosition HighlightText IsAutoFocus SetThumbTexture GetThumbTexture SetHorizTile SetVertTile GetHorizTile GetVertTile GetTextColor GetShadowColor GetVertexColor EnableKeyboard IsKeyboardEnabled SetPropagateKeyboardInput GetPropagateKeyboardInput EnableMouseWheel IsMouseWheelEnabled SetOrientation GetOrientation SetValueStep GetValueStep SetObeyStepOnDrag GetObeyStepOnDrag SetID GetID CreateAnimationGroup SetTextInsets GetTextInsets SetGradient SetGradientAlpha SetNonSpaceWrap SetEllipsis SetSpacing GetSpacing CanWordWrap CanNonSpaceWrap GetMaxLines IsTruncated SetFontObject GetFontObject Raise Lower SetClipsChildren DoesClipChildren GetScrollChild GetHorizontalScroll GetVerticalScroll GetHorizontalScrollRange GetVerticalScrollRange UpdateScrollChildRect SetShadowColor SetShadowOffset SetWordWrap SetMaxLines SetBlendMode GetName GetObjectType IsObjectType GetParent SetParent SetSize SetWidth SetHeight GetWidth GetHeight GetSize SetPoint GetPoint GetNumPoints ClearAllPoints SetAllPoints GetRect GetLeft GetRight GetTop GetBottom GetCenter SetScale GetScale GetEffectiveScale SetAlpha GetAlpha GetEffectiveAlpha SetFrameStrata GetFrameStrata SetFrameLevel GetFrameLevel SetDrawLayer GetDrawLayer Show Hide SetShown IsShown IsVisible IsProtected SetText GetText SetFont GetFont GetUnboundedStringWidth GetStringWidth GetStringHeight SetTexture GetTexture SetAtlas SetColorTexture SetScript GetScript HookScript HasScript RegisterEvent UnregisterEvent UnregisterAllEvents CreateTexture CreateMaskTexture CreateFontString EnableMouse IsMouseEnabled Enable Disable IsEnabled SetChecked GetChecked SetValue GetValue SetMinMaxValues GetMinMaxValues SetFocus ClearFocus HasFocus StartMoving StopMovingOrSizing RegisterForClicks RegisterForDrag SetMovable SetClampedToScreen SetNormalTexture SetHighlightTexture SetPushedTexture SetAutoFocus SetMultiLine SetJustifyH SetJustifyV SetTextColor SetTexCoord SetVertexColor SetScrollChild SetHorizontalScroll SetVerticalScroll SetCooldown SetOwner GetOwner GetTextureFileID";
fn call(lua: &Lua, ctx: &Ctx, method: &str, args: MultiValue) -> mlua::Result<MultiValue> {
    let _profile = crate::profile::Span::new("frames.methods");
    if !ctx
        .stale_sources
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty()
    {
        return Err(mlua::Error::runtime(
            "SELU0623: stale frame after sim.reload(); look it up again with sim.frame",
        ));
    }
    let args = args.into_vec();
    let Some(Value::Table(t)) = args.first() else {
        return Err(mlua::Error::runtime("region expected"));
    };
    let i = id(t)?;
    let v = &args[1..];
    let a = store(lua);
    macro_rules! out {
        ($x:expr) => {
            return $x.into_lua_multi(lua)
        };
    }
    if matches!(
        method,
        "SetNormalAtlas" | "SetPushedAtlas" | "SetHighlightAtlas" | "SetDisabledAtlas"
    ) {
        let slot = method.replace("Atlas", "Texture");
        if matches!(v.first(), None | Some(Value::Nil)) {
            return call(
                lua,
                ctx,
                &slot,
                MultiValue::from_vec(vec![Value::Table(t.clone()), Value::Nil]),
            );
        }
        let key = format!("__selu_{}", &slot[3..]);
        let child = if let Some(child) = t.raw_get::<Option<Table>>(key)? {
            child
        } else {
            create(lua, ctx, "Texture", None, Some(t.clone()))?
        };
        call(
            lua,
            ctx,
            "SetAtlas",
            MultiValue::from_vec(vec![
                Value::Table(child.clone()),
                v.first().cloned().unwrap_or(Value::Nil),
            ]),
        )?;
        call(
            lua,
            ctx,
            "SetAllPoints",
            MultiValue::from_vec(vec![Value::Table(child.clone()), Value::Table(t.clone())]),
        )?;
        return call(
            lua,
            ctx,
            &slot,
            MultiValue::from_vec(vec![Value::Table(t.clone()), Value::Table(child)]),
        );
    }
    if method == "SetUnit" {
        t.raw_set("__selu_unit", v.first().cloned().unwrap_or(Value::Nil))?;
        ctx.not_simulated("native unit-bound widget data");
        out!(());
    }
    if method == "GetUnit" {
        out!(t.raw_get::<Value>("__selu_unit")?);
    }
    if method == "IsOwned" {
        out!(!t.raw_get::<Value>("__tooltip_owner")?.is_nil());
    }
    if method == "GetViewInsets" {
        ctx.not_simulated("model view insets");
        out!((0.0, 0.0, 0.0, 0.0));
    }
    if method == "GetAtlas" {
        out!(t.raw_get::<Value>("__selu_atlas_name")?);
    }
    if method == "SetAtlas" {
        t.raw_set(
            "__selu_atlas_name",
            v.first().cloned().unwrap_or(Value::Nil),
        )?;
    }
    if matches!(method, "SetTexture" | "SetColorTexture") {
        t.raw_set("__selu_atlas_name", Value::Nil)?;
    }
    if method == "SetFormattedText" {
        let format = crate::host_lua::factory(
            lua,
            "selu.helper.format",
            "return function(...) return string.format(...) end",
        )?;
        let text = format.call::<String>(MultiValue::from_vec(v.to_vec()))?;
        return call(
            lua,
            ctx,
            "SetText",
            MultiValue::from_vec(vec![
                Value::Table(t.clone()),
                Value::String(lua.create_string(text)?),
            ]),
        );
    }
    if method == "GetFontString" {
        out!(t.raw_get::<Value>("__selu_button_text")?);
    }
    if method == "SetFontString" {
        t.raw_set(
            "__selu_button_text",
            v.first().cloned().unwrap_or(Value::Nil),
        )?;
        out!(());
    }
    if matches!(
        method,
        "SetNormalFontObject" | "SetHighlightFontObject" | "SetDisabledFontObject"
    ) {
        t.raw_set(
            format!("__selu_{method}"),
            v.first().cloned().unwrap_or(Value::Nil),
        )?;
        if method == "SetNormalFontObject"
            && let Some(child) = t.raw_get::<Option<Table>>("__selu_button_text")?
        {
            call(
                lua,
                ctx,
                "SetFontObject",
                MultiValue::from_vec(vec![
                    Value::Table(child),
                    v.first().cloned().unwrap_or(Value::Nil),
                ]),
            )?;
        }
        out!(());
    }
    if method == "GetText"
        && matches!(
            a.borrow().regions[i].kind.as_str(),
            "Button" | "CheckButton"
        )
        && let Some(child) = t.raw_get::<Option<Table>>("__selu_button_text")?
    {
        return call(
            lua,
            ctx,
            "GetText",
            MultiValue::from_vec(vec![Value::Table(child)]),
        );
    }
    if method == "SetText"
        && matches!(
            a.borrow().regions[i].kind.as_str(),
            "Button" | "CheckButton"
        )
    {
        let child = if let Some(child) = t.raw_get::<Option<Table>>("__selu_button_text")? {
            child
        } else {
            let child = create(lua, ctx, "FontString", None, Some(t.clone()))?;
            call(
                lua,
                ctx,
                "SetAllPoints",
                MultiValue::from_vec(vec![Value::Table(child.clone()), Value::Table(t.clone())]),
            )?;
            t.raw_set("__selu_button_text", child.clone())?;
            child
        };
        if let Some(font) = t.raw_get::<Option<Table>>("__selu_SetNormalFontObject")? {
            call(
                lua,
                ctx,
                "SetFontObject",
                MultiValue::from_vec(vec![Value::Table(child.clone()), Value::Table(font)]),
            )?;
        }
        call(
            lua,
            ctx,
            "SetText",
            MultiValue::from_vec(vec![
                Value::Table(child),
                v.first().cloned().unwrap_or(Value::Nil),
            ]),
        )?;
        out!(());
    }
    if matches!(
        method,
        "SetNormalTexture"
            | "GetNormalTexture"
            | "SetHighlightTexture"
            | "GetHighlightTexture"
            | "SetPushedTexture"
            | "GetPushedTexture"
            | "SetDisabledTexture"
            | "GetDisabledTexture"
    ) {
        let key = format!("__selu_{}", &method[3..]);
        let previous = t.raw_get::<Option<Table>>(key.as_str())?;
        if method.starts_with("Get") {
            out!(previous);
        }
        let child = match v.first() {
            Some(Value::Table(child)) => Some(child.clone()),
            Some(Value::String(_) | Value::Integer(_) | Value::Number(_)) => {
                let child = if let Some(child) = previous.clone() {
                    child
                } else {
                    create(lua, ctx, "Texture", None, Some(t.clone()))?
                };
                call(
                    lua,
                    ctx,
                    "SetTexture",
                    MultiValue::from_vec(vec![Value::Table(child.clone()), v[0].clone()]),
                )?;
                call(
                    lua,
                    ctx,
                    "SetAllPoints",
                    MultiValue::from_vec(vec![
                        Value::Table(child.clone()),
                        Value::Table(t.clone()),
                    ]),
                )?;
                Some(child)
            }
            _ => None,
        };
        if let Some(previous) = previous {
            call(
                lua,
                ctx,
                "Hide",
                MultiValue::from_vec(vec![Value::Table(previous)]),
            )?;
        }
        if let Some(child) = &child {
            call(
                lua,
                ctx,
                if method == "SetNormalTexture" {
                    "Show"
                } else {
                    "Hide"
                },
                MultiValue::from_vec(vec![Value::Table(child.clone())]),
            )?;
        }
        t.raw_set(key, child)?;
        sync_button_textures(lua, ctx, &t)?;
        out!(());
    }
    if method == "CreateAnimationGroup" {
        let constructor = crate::host_lua::factory(
            lua,
            "selu.helper.animations",
            include_str!("animations.lua"),
        )?;
        let context = ctx.clone();
        let note = lua.create_function(move |_, name: String| {
            context.not_simulated(&name);
            Ok(())
        })?;
        let translate = lua.create_function(|lua, (target, x, y): (Table, f64, f64)| {
            store(lua).borrow_mut().regions[id(&target)?].animation_offset = (x, y);
            Ok(())
        })?;
        out!(constructor.call::<Table>((t.clone(), note, translate))?);
    }
    if matches!(method, "GetAttribute" | "SetAttribute") {
        let attributes = t
            .raw_get::<Option<Table>>("__attributes")?
            .unwrap_or(lua.create_table()?);
        let name =
            string(v.first()).ok_or_else(|| mlua::Error::runtime("attribute name expected"))?;
        let old = attributes.raw_get::<Value>(name.as_str())?;
        if method == "GetAttribute" {
            out!(old);
        }
        let value = v.get(1).cloned().unwrap_or(Value::Nil);
        attributes.raw_set(name.as_str(), value.clone())?;
        t.raw_set("__attributes", attributes)?;
        if matches!(old, Value::LightUserData(_))
            || matches!(value, Value::LightUserData(_))
            || old != value
        {
            callback(
                ctx,
                t.clone(),
                "OnAttributeChanged",
                vec![Value::String(lua.create_string(name)?), value],
            );
        }
        out!(());
    }
    if matches!(
        method,
        "CreateTexture" | "CreateMaskTexture" | "CreateFontString"
    ) {
        let kind = &method[6..];
        let child = create(lua, ctx, kind, string(v.first()), Some(t.clone()))?;
        if let Some(layer) = string(v.get(1)) {
            call(
                lua,
                ctx,
                "SetDrawLayer",
                MultiValue::from_vec(vec![
                    Value::Table(child.clone()),
                    Value::String(lua.create_string(layer)?),
                    v.get(3).cloned().unwrap_or(Value::Integer(0)),
                ]),
            )?;
        }
        if kind == "FontString"
            && let Some(template) = string(v.get(2))
        {
            let object = crate::fonts::template(lua, &template)?;
            call(
                lua,
                ctx,
                "SetFontObject",
                MultiValue::from_vec(vec![Value::Table(child.clone()), Value::Table(object)]),
            )?;
        }
        out!(child);
    }
    if matches!(
        method,
        "GetScript"
            | "SetScript"
            | "HookScript"
            | "HasScript"
            | "RegisterEvent"
            | "UnregisterEvent"
            | "UnregisterAllEvents"
    ) {
        let scripts: Table = t.raw_get("__scripts")?;
        let name = string(v.first()).unwrap_or_default();
        let intrinsic = t
            .raw_get::<Option<Table>>("__selu_intrinsic_scripts")?
            .map(|handlers| handlers.raw_get::<Option<Table>>(name.as_str()))
            .transpose()?
            .flatten();
        let (script_owner, script_key) = if let Some(slot) = intrinsic {
            (slot, "normal".to_owned())
        } else {
            (scripts.clone(), name.clone())
        };
        match method {
            "GetScript" => out!(script_owner.raw_get::<Value>(script_key)?),
            "HasScript" => out!(has_script(&a.borrow().regions[i].kind, &name)),
            "SetScript" => {
                script_owner.raw_set(script_key, v.get(1).cloned().unwrap_or(Value::Nil))?
            }
            "HookScript" => {
                let old = script_owner.raw_get::<Option<Function>>(script_key.clone())?;
                let hook = match v.get(1) {
                    Some(Value::Function(f)) => f.clone(),
                    _ => return Err(mlua::Error::runtime("function expected")),
                };
                let old = old.map(|f| lua.create_registry_value(f)).transpose()?;
                let hook = lua.create_registry_value(hook)?;
                let context = ctx.clone();
                script_owner.raw_set(
                    script_key,
                    lua.create_function(move |lua, args: MultiValue| {
                        if let Some(old) = &old {
                            crate::core::script_callback(lua, &context, || {
                                crate::core::call_lua::<()>(
                                    lua,
                                    lua.registry_value::<Function>(old)?,
                                    args.clone(),
                                )
                            })?;
                            crate::core::flush(lua, &context);
                        }
                        let result = crate::core::script_callback(lua, &context, || {
                            crate::core::call_lua::<()>(
                                lua,
                                lua.registry_value::<Function>(&hook)?,
                                args,
                            )
                        });
                        crate::core::flush(lua, &context);
                        result
                    })?,
                )?;
            }
            "RegisterEvent" => t.raw_get::<Table>("__events")?.raw_set(name, true)?,
            "UnregisterEvent" => t.raw_get::<Table>("__events")?.raw_set(name, Value::Nil)?,
            _ => t.raw_set("__events", lua.create_table()?)?,
        }
        out!(());
    }
    if method == "SetFontObject" {
        let object = match v.first() {
            Some(Value::String(name)) => lua.globals().get::<Table>(name.to_str()?.as_ref())?,
            Some(Value::Table(t)) => t.clone(),
            _ => return Err(mlua::Error::runtime("font object expected")),
        };
        let get: Function = object.get("GetFont")?;
        let (path, size, flags): (String, f64, String) = get.call(object.clone())?;
        t.raw_set("__font_object", object)?;
        let mut a = a.borrow_mut();
        a.regions[i].font_overrides &= !64;
        a.regions[i].font_path = path;
        a.regions[i].font_size = size;
        a.regions[i].font_flags = flags;
        a.dirty = true;
        a.dirty_regions.insert(i);
        out!(());
    }
    if method == "SetStatusBarColor" || method == "GetStatusBarColor" {
        let mut arena = a.borrow_mut();
        if method == "GetStatusBarColor" {
            let c = arena.regions[i].visual.tint;
            out!((c[0], c[1], c[2], c[3]));
        }
        let color = [
            number(v.first(), 1.0) as f32,
            number(v.get(1), 1.0) as f32,
            number(v.get(2), 1.0) as f32,
            number(v.get(3), 1.0) as f32,
        ];
        arena.regions[i].visual.tint = color;
        if let Some(child) = arena.regions[i].status_texture {
            arena.regions[child].visual.tint = color;
        }
        out!(());
    }
    if matches!(method, "SetThumbTexture" | "SetStatusBarTexture") {
        let status = method == "SetStatusBarTexture";
        let child = match v.first() {
            Some(Value::Table(child)) => child.clone(),
            _ => {
                let existing = if status {
                    a.borrow().regions[i].status_texture
                } else {
                    a.borrow().regions[i].thumb
                };
                let child = if let Some(index) = existing {
                    a.borrow().regions[index].table.get()
                } else {
                    create(lua, ctx, "Texture", None, Some(t.clone()))?
                };
                call(
                    lua,
                    ctx,
                    "SetTexture",
                    MultiValue::from_vec(vec![
                        Value::Table(child.clone()),
                        v.first().cloned().unwrap_or(Value::Nil),
                    ]),
                )?;
                child
            }
        };
        let index = id(&child)?;
        let mut arena = a.borrow_mut();
        arena.regions[index].parent = Some(i);
        if status {
            arena.regions[i].status_texture = Some(index);
            arena.regions[index].visual.tint = arena.regions[i].visual.tint;
        } else {
            arena.regions[i].thumb = Some(index);
        }
        arena.dirty = true;
        arena.dirty_regions.insert(i);
        arena.dirty_regions.insert(index);
        layout::resolve(&mut arena, ctx);
        if status {
            out!(true);
        }
        out!(());
    }
    if method == "GetStatusBarTexture" {
        out!(
            a.borrow().regions[i]
                .status_texture
                .map(|index| a.borrow().regions[index].table.get())
        );
    }
    if method == "GetThumbTexture" {
        out!(
            a.borrow().regions[i]
                .thumb
                .map(|index| a.borrow().regions[index].table.get())
        );
    }
    if method == "GetFontObject" {
        out!(t.raw_get::<Value>("__font_object")?);
    }
    if matches!(method, "GetTextColor" | "GetShadowColor" | "GetVertexColor") {
        let key = match method {
            "GetTextColor" => "_TextColor",
            "GetShadowColor" => "_ShadowColor",
            _ => "_VertexColor",
        };
        let mut object = Some(t.clone());
        for _ in 0..64 {
            let Some(current) = object else { break };
            if let Some(color) = current.raw_get::<Option<Table>>(key)? {
                out!((
                    color.raw_get::<f64>(1)?,
                    color.raw_get::<f64>(2)?,
                    color.raw_get::<f64>(3)?,
                    color.raw_get::<f64>(4).unwrap_or(1.0)
                ));
            }
            if current.raw_get::<Option<Table>>("_parent")?.is_none()
                && let Some(defaults) = current.raw_get::<Option<Table>>("_defaults")?
                && let Some(color) =
                    defaults.raw_get::<Option<Table>>(key.trim_start_matches('_'))?
            {
                out!((
                    color.raw_get::<f64>(1)?,
                    color.raw_get::<f64>(2)?,
                    color.raw_get::<f64>(3)?,
                    color.raw_get::<f64>(4).unwrap_or(1.0)
                ));
            }
            object = current
                .raw_get::<Option<Table>>("__font_object")?
                .or(current.raw_get::<Option<Table>>("_parent")?);
        }
    }
    if a.borrow().layout_batches > 0
        && matches!(
            method,
            "IsVisible" | "GetEffectiveScale" | "GetEffectiveAlpha"
        )
    {
        let arena = a.borrow();
        let mut current = Some(i);
        let (mut visible, mut scale, mut alpha) = (true, 1.0, 1.0);
        for _ in 0..128 {
            let Some(index) = current else { break };
            let region = &arena.regions[index];
            visible &= region.shown;
            scale *= region.scale;
            alpha *= region.alpha;
            current = region.parent;
        }
        match method {
            "IsVisible" => out!(visible && current.is_none()),
            "GetEffectiveScale" => out!(scale),
            _ => out!(alpha),
        }
    }
    layout::resolve(&mut a.borrow_mut(), ctx);
    let old = a.borrow().regions[i].clone();
    let n = number(v.first(), 0.0);
    let s = string(v.first()).unwrap_or_default();
    let mut a = a.borrow_mut();
    if matches!(method, "Raise" | "Lower") {
        let strata = a.regions[i].strata;
        let levels = a
            .regions
            .iter()
            .filter(|r| r.strata == strata && !is_region(&r.kind))
            .map(|r| r.level);
        let level = if method == "Raise" {
            levels.max().unwrap_or(0).saturating_add(1)
        } else {
            levels.min().unwrap_or(0).saturating_sub(1)
        };
        a.regions[i].level = level;
        a.regions[i].level_explicit = true;
    }
    if method == "SetScrollChild" {
        let child = match v.first() {
            Some(Value::Table(t)) => Some(id(t)?),
            _ => None,
        };
        if let Some(c) = child {
            let mut ancestor = Some(i);
            for _ in 0..a.regions.len() {
                let Some(p) = ancestor else { break };
                if p == c {
                    return Err(mlua::Error::runtime("scroll parent cycle"));
                }
                ancestor = a.regions[p].parent;
            }
            a.regions[c].parent = Some(i);
            a.regions[c].points = vec![Point {
                point: "TOPLEFT".into(),
                relative: i,
                anchor: "TOPLEFT".into(),
                x: 0.0,
                y: 0.0,
            }];
            a.dirty_regions.insert(c);
        }
        a.regions[i].scroll_child = child;
    }
    if method == "GetScrollChild" {
        out!(a.regions[i].scroll_child.map(|c| a.regions[c].table.get()));
    }
    if matches!(
        method,
        "GetHorizontalScrollRange" | "GetVerticalScrollRange"
    ) {
        let r = &a.regions[i];
        let range = r
            .scroll_child
            .map(|c| {
                let c = &a.regions[c];
                if method == "GetHorizontalScrollRange" {
                    c.width - r.width
                } else {
                    c.height - r.height
                }
            })
            .unwrap_or(0.0)
            .max(0.0);
        out!(range);
    }
    if matches!(
        method,
        "GetUnboundedStringWidth" | "GetStringWidth" | "GetStringHeight" | "IsTruncated"
    ) {
        let r = &a.regions[i];
        let width = if matches!(method, "GetStringWidth" | "GetUnboundedStringWidth") {
            None
        } else {
            r.rect
                .map(|q| q.width / r.effective_scale)
                .or_else(|| (r.width > 0.0).then_some(r.width))
        };
        let height = if method == "IsTruncated" {
            r.rect.map(|q| q.height / r.effective_scale)
        } else {
            None
        };
        let mut style = r.text_style();
        if method == "GetUnboundedStringWidth" {
            style.max_lines = 0;
            style.ellipsis = false;
        }
        let shaped = a
            .fonts
            .layout(r.display_text().unwrap_or(""), &style, width, height);
        match method {
            "GetStringWidth" | "GetUnboundedStringWidth" => out!(shaped.width),
            "GetStringHeight" => out!(shaped.height),
            _ => out!(shaped.truncated),
        }
    }
    let asset_available = method != "SetTexture"
        || a.fonts.asset(&if s.is_empty() {
            n.to_string()
        } else {
            s.clone()
        });
    let current_asset_available = match &a.regions[i].texture {
        Some(TextureFill::File(path)) => a.fonts.asset(path),
        _ => false,
    };
    let current_asset_id = if method == "GetTextureFileID" && current_asset_available {
        match &a.regions[i].texture {
            Some(TextureFill::File(path)) => a.fonts.assets.file_id(path),
            _ => None,
        }
    } else {
        None
    };
    let atlas = if method == "SetAtlas" {
        a.fonts.assets.atlas(&s)
    } else {
        None
    };
    let r = &mut a.regions[i];
    r.font_overrides |= match method {
        "SetTextColor" => 1,
        "SetShadowColor" => 2,
        "SetShadowOffset" => 4,
        "SetJustifyH" => 8,
        "SetJustifyV" => 16,
        "SetSpacing" => 32,
        "SetFont" => 64,
        _ => 0,
    };
    match method {
        "Raise" | "Lower" | "SetScrollChild" | "UpdateScrollChildRect" => {}
        "SetClipsChildren" => r.clips_children = boolean(v.first()),
        "DoesClipChildren" => out!(r.clips_children),
        "SetHorizontalScroll" => r.scroll.0 = n.max(0.0),
        "SetVerticalScroll" => r.scroll.1 = n.max(0.0),
        "GetHorizontalScroll" => out!(r.scroll.0),
        "GetVerticalScroll" => out!(r.scroll.1),
        "SetJustifyH" => r.visual.justify_h = s,
        "SetJustifyV" => r.visual.justify_v = s,
        "SetNonSpaceWrap" => r.visual.non_space_wrap = boolean(v.first()),
        "CanWordWrap" => out!(r.visual.wrap),
        "CanNonSpaceWrap" => out!(r.visual.non_space_wrap),
        "GetMaxLines" => out!(r.visual.max_lines),
        "SetEllipsis" => r.visual.ellipsis = boolean(v.first()),
        "SetSpacing" => r.visual.spacing = n,
        "GetSpacing" => out!(r.visual.spacing),
        "SetWordWrap" => r.visual.wrap = boolean(v.first()),
        "SetMaxLines" => r.visual.max_lines = n.max(0.0) as usize,
        "SetBlendMode" => r.visual.blend = s,
        "SetShadowOffset" => r.visual.shadow_offset = (n, number(v.get(1), 0.0)),
        "SetTextColor" | "SetVertexColor" | "SetShadowColor" => {
            let key = match method {
                "SetTextColor" => "_TextColor",
                "SetShadowColor" => "_ShadowColor",
                _ => "_VertexColor",
            };
            t.raw_set(
                key,
                lua.create_sequence_from([
                    n,
                    number(v.get(1), 1.0),
                    number(v.get(2), 1.0),
                    number(v.get(3), 1.0),
                ])?,
            )?;
            let color = [
                n as f32,
                number(v.get(1), 1.0) as f32,
                number(v.get(2), 1.0) as f32,
                number(v.get(3), 1.0) as f32,
            ];
            match method {
                "SetTextColor" => r.visual.color = color,
                "SetVertexColor" => r.visual.tint = color,
                _ => r.visual.shadow = color,
            }
        }
        "SetTexCoord" => {
            r.visual.uv = [
                n as f32,
                number(v.get(1), 1.0) as f32,
                number(v.get(2), 0.0) as f32,
                number(v.get(3), 1.0) as f32,
            ]
        }
        "SetOwner" => {
            t.raw_set("__tooltip_owner", v.first().cloned().unwrap_or(Value::Nil))?;
        }
        "GetOwner" => out!(t.raw_get::<Value>("__tooltip_owner")?),
        "GetName" => out!(r.name.clone()),
        "GetObjectType" => out!(r.kind.clone()),
        "IsObjectType" => out!(super::manifest::inherits(&r.kind, &s)),
        "GetParent" => {
            let p = r.parent;
            out!(p.map(|p| a.regions[p].table.get()));
        }
        "SetParent" => {
            r.parent = match v.first() {
                Some(Value::Table(p)) => Some(id(p)?),
                _ => None,
            };
        }
        "SetSize" => {
            r.width = n;
            r.height = number(v.get(1), 0.0);
        }
        "SetWidth" => r.width = n,
        "SetHeight" => r.height = n,
        "GetWidth" => out!(
            r.rect
                .map(|q| q.width / r.effective_scale)
                .unwrap_or(r.width)
        ),
        "GetHeight" => out!(
            r.rect
                .map(|q| q.height / r.effective_scale)
                .unwrap_or(r.height)
        ),
        "GetSize" => out!((
            r.rect
                .map(|q| q.width / r.effective_scale)
                .unwrap_or(r.width),
            r.rect
                .map(|q| q.height / r.effective_scale)
                .unwrap_or(r.height)
        )),
        "SetScale" => {
            if n <= 0.0 || !n.is_finite() {
                return Err(mlua::Error::runtime("invalid scale"));
            }
            r.scale = n;
            if i == 0 {
                r.width = lua
                    .globals()
                    .raw_get::<f64>("__selu_screen_width")
                    .unwrap_or(1920.0)
                    / n;
                r.height = lua
                    .globals()
                    .raw_get::<f64>("__selu_screen_height")
                    .unwrap_or(1080.0)
                    / n;
            }
        }
        "GetScale" => out!(r.scale),
        "GetEffectiveScale" => out!(r.effective_scale),
        "SetAlpha" => r.alpha = n,
        "GetAlpha" => out!(r.alpha),
        "GetEffectiveAlpha" => out!(r.effective_alpha),
        "SetFrameStrata" => {
            r.strata_explicit = true;
            r.strata = match s.as_str() {
                "WORLD" => Strata::World,
                "BACKGROUND" => Strata::Background,
                "LOW" => Strata::Low,
                "HIGH" => Strata::High,
                "DIALOG" => Strata::Dialog,
                "FULLSCREEN" => Strata::Fullscreen,
                "FULLSCREEN_DIALOG" => Strata::FullscreenDialog,
                "TOOLTIP" => Strata::Tooltip,
                _ => Strata::Medium,
            };
        }
        "GetFrameStrata" => out!(
            format!("{:?}", r.strata)
                .replace("FullscreenDialog", "FULLSCREEN_DIALOG")
                .to_uppercase()
        ),
        "SetFrameLevel" => {
            r.level = n as i32;
            r.level_explicit = true;
        }
        "GetFrameLevel" => out!(r.level),
        "SetID" => {
            r.id = n as i32;
        }
        "GetID" => out!(r.id),
        "SetDrawLayer" => {
            r.layer = (
                match s.as_str() {
                    "BACKGROUND" => DrawLayer::Background,
                    "BORDER" => DrawLayer::Border,
                    "OVERLAY" => DrawLayer::Overlay,
                    "HIGHLIGHT" => DrawLayer::Highlight,
                    _ => DrawLayer::Artwork,
                },
                number(v.get(1), 0.0).clamp(-8.0, 7.0) as i8,
            );
        }
        "GetDrawLayer" => out!((format!("{:?}", r.layer.0).to_uppercase(), r.layer.1)),
        "Show" => r.shown = true,
        "Hide" => r.shown = false,
        "SetShown" => r.shown = boolean(v.first()),
        "IsShown" => out!(r.shown),
        "IsVisible" => out!(r.visible),
        "IsProtected" => out!(t.raw_get::<Option<bool>>("__protected")?.unwrap_or(false)),
        "EnableKeyboard" => r.keyboard = boolean(v.first()),
        "IsKeyboardEnabled" => out!(r.keyboard),
        "SetPropagateKeyboardInput" => r.propagate_keyboard = boolean(v.first()),
        "GetPropagateKeyboardInput" => out!(r.propagate_keyboard),
        "EnableMouseWheel" => r.wheel = boolean(v.first()),
        "IsMouseWheelEnabled" => out!(r.wheel),
        "GetTextColor" | "GetShadowColor" | "GetVertexColor" => {
            let c = match method {
                "GetTextColor" => r.visual.color,
                "GetShadowColor" => r.visual.shadow,
                _ => r.visual.tint,
            };
            out!((c[0], c[1], c[2], c[3]));
        }
        "SetOrientation" => {
            if s != "HORIZONTAL" && s != "VERTICAL" {
                return Err(mlua::Error::runtime("invalid orientation"));
            }
            r.orientation = s;
        }
        "GetOrientation" => out!(r.orientation.clone()),
        "SetValueStep" => r.step = n.max(0.0),
        "GetValueStep" => out!(r.step),
        "SetObeyStepOnDrag" => r.obey_step = boolean(v.first()),
        "GetObeyStepOnDrag" => out!(r.obey_step),
        "EnableMouse" => r.mouse = boolean(v.first()),
        "IsMouseEnabled" => out!(r.mouse),
        "Enable" => r.enabled = true,
        "Disable" => r.enabled = false,
        "IsEnabled" => out!(r.enabled),
        "SetChecked" => r.checked = boolean(v.first()),
        "GetChecked" => out!(r.checked),
        "SetText" => {
            r.cursor_position = 0;
            r.selection = (0, 0);
            r.text_value = v.first().cloned().unwrap_or(Value::Nil);
            r.text = if matches!(v.first(), None | Some(Value::Nil | Value::LightUserData(_))) {
                None
            } else {
                Some(crate::value_string(&v[0]))
            };
            if r.kind == "EditBox" {
                r.cursor_position = r.text.as_deref().unwrap_or("").len();
                r.selection = (r.cursor_position, r.cursor_position);
            }
            if !matches!(r.text_value, Value::LightUserData(_)) {
                // Ordinary text already lives in Rust; retaining a second Lua
                // string consumes one auxiliary-stack slot per text region.
                r.text_value = Value::Nil;
            }
        }
        "GetText" => {
            if matches!(r.text_value, Value::LightUserData(_)) {
                out!(r.text_value.clone());
            }
            out!(r.text.clone());
        }
        "SetMultiLine" => r.multi_line = boolean(v.first()),
        "SetTextInsets" => {
            r.visual.text_insets = [
                n,
                number(v.get(1), 0.0),
                number(v.get(2), 0.0),
                number(v.get(3), 0.0),
            ]
        }
        "GetTextInsets" => out!((
            r.visual.text_insets[0],
            r.visual.text_insets[1],
            r.visual.text_insets[2],
            r.visual.text_insets[3]
        )),
        "SetFont" => {
            r.font_size = number(v.get(1), 12.0);
            r.font_path = s;
            r.font_flags = string(v.get(2)).unwrap_or_default();
        }
        "GetFont" => out!((r.font_path.clone(), r.font_size, r.font_flags.clone())),
        "SetHorizTile" => r.visual.tile[0] = boolean(v.first()),
        "SetVertTile" => r.visual.tile[1] = boolean(v.first()),
        "GetHorizTile" => out!(r.visual.tile[0]),
        "GetVertTile" => out!(r.visual.tile[1]),
        "SetTexture" => {
            for axis in 0..2 {
                let mode = string(v.get(axis + 1)).unwrap_or("CLAMP".into());
                if !matches!(mode.as_str(), "CLAMP" | "REPEAT" | "MIRROR") {
                    return Err(mlua::Error::runtime("invalid texture wrap mode"));
                }
                r.visual.wrap_modes[axis] = mode;
            }
            r.atlas_uv = None;
            // Zero is the native empty/invalid FileDataID sentinel, not an asset.
            let empty = matches!(v.first(), None | Some(Value::Nil))
                || matches!(v.first(), Some(Value::Integer(0)))
                || matches!(v.first(), Some(Value::Number(value)) if *value == 0.0);
            if !asset_available && !empty {
                ctx.not_simulated(&format!("unavailable texture: {s}"));
            }
            r.texture = if empty {
                None
            } else {
                Some(TextureFill::File(if s.is_empty() {
                    n.to_string()
                } else {
                    s
                }))
            };
        }
        "GetTextureFileID" => out!(current_asset_id),
        "GetTexture" => out!(match &r.texture {
            Some(TextureFill::File(s)) if current_asset_available => Some(s.clone()),
            _ => None,
        }),
        "SetAtlas" => {
            if let Some(atlas) = atlas {
                r.texture = Some(TextureFill::File(atlas.file_data_id.to_string()));
                r.atlas_uv = Some([
                    atlas.left as f32 / atlas.sheet_width as f32,
                    atlas.right as f32 / atlas.sheet_width as f32,
                    atlas.top as f32 / atlas.sheet_height as f32,
                    atlas.bottom as f32 / atlas.sheet_height as f32,
                ]);
                if boolean(v.get(3)) {
                    r.visual.uv = [0.0, 1.0, 0.0, 1.0];
                }
                if boolean(v.get(1)) {
                    r.width = atlas.width as f64;
                    r.height = atlas.height as f64;
                }
            } else {
                ctx.not_simulated(&format!("unavailable atlas: {s}"));
                r.texture = Some(TextureFill::Atlas(s));
            }
        }
        "SetGradient" | "SetGradientAlpha" => {
            if s != "VERTICAL" && s != "HORIZONTAL" {
                return Err(mlua::Error::runtime("invalid gradient orientation"));
            }
            let color = |start: usize, alpha: bool| -> mlua::Result<[f32; 4]> {
                if let Some(Value::Table(c)) = v.get(start) {
                    let get: Function = c.get("GetRGBA")?;
                    let (r, g, b, a): (f32, f32, f32, f32) = get.call(c.clone())?;
                    Ok([r, g, b, a])
                } else {
                    Ok([
                        number(v.get(start), 0.0) as f32,
                        number(v.get(start + 1), 0.0) as f32,
                        number(v.get(start + 2), 0.0) as f32,
                        if alpha {
                            number(v.get(start + 3), 1.0) as f32
                        } else {
                            1.0
                        },
                    ])
                }
            };
            let alpha = method == "SetGradientAlpha";
            let modern = matches!(v.get(1), Some(Value::Table(_)));
            r.texture = Some(TextureFill::Gradient {
                vertical: s == "VERTICAL",
                min: color(1, alpha)?,
                max: color(
                    if modern {
                        2
                    } else if alpha {
                        5
                    } else {
                        4
                    },
                    alpha,
                )?,
            });
        }
        "SetColorTexture" => {
            r.texture = Some(TextureFill::Color([
                n as f32,
                number(v.get(1), 0.0) as f32,
                number(v.get(2), 0.0) as f32,
                number(v.get(3), 1.0) as f32,
            ]))
        }
        "ClearAllPoints" => r.points.clear(),
        "GetNumPoints" => out!(r.points.len()),
        "GetPoint" => {
            let p = r.points.get(n.max(1.0) as usize - 1).cloned();
            if let Some(p) = p {
                out!((
                    p.point,
                    a.regions.get(p.relative).map(|r| r.table.get()),
                    p.anchor,
                    p.x,
                    p.y
                ));
            }
            out!(());
        }
        "SetAllPoints" => {
            let relative = match v.first() {
                Some(Value::Table(p)) => id(p)?,
                _ => r.parent.unwrap_or(0),
            };
            r.points = vec![
                Point {
                    point: "BOTTOMLEFT".into(),
                    relative,
                    anchor: "BOTTOMLEFT".into(),
                    x: 0.0,
                    y: 0.0,
                },
                Point {
                    point: "TOPRIGHT".into(),
                    relative,
                    anchor: "TOPRIGHT".into(),
                    x: 0.0,
                    y: 0.0,
                },
            ];
        }
        "SetPoint" => {
            let relative = match v.get(1) {
                Some(Value::Table(p)) => id(p)?,
                Some(Value::String(name)) => {
                    let p: Option<Table> = lua.globals().raw_get(name.clone())?;
                    p.as_ref().map(id).transpose()?.unwrap_or(usize::MAX)
                }
                _ => r.parent.unwrap_or(0),
            };
            let (anchor, x, y) = if matches!(v.get(1), Some(Value::Number(_) | Value::Integer(_))) {
                (s.clone(), number(v.get(1), 0.0), number(v.get(2), 0.0))
            } else {
                (
                    string(v.get(2)).unwrap_or(s.clone()),
                    number(v.get(3), 0.0),
                    number(v.get(4), 0.0),
                )
            };
            let point = Point {
                point: s,
                relative,
                anchor,
                x,
                y,
            };
            if let Some(old) = r.points.iter_mut().find(|p| p.point == point.point) {
                *old = point;
            } else {
                r.points.push(point);
            }
        }
        "GetRect" | "GetScaledRect" | "GetLeft" | "GetRight" | "GetTop" | "GetBottom"
        | "GetCenter" => {
            if let Some(q) = r.rect {
                let z = r.effective_scale;
                match method {
                    "GetRect" => out!((q.left / z, q.bottom / z, q.width / z, q.height / z)),
                    "GetScaledRect" => out!((q.left, q.bottom, q.width, q.height)),
                    "GetLeft" => out!(q.left / z),
                    "GetRight" => out!((q.left + q.width) / z),
                    "GetTop" => out!((q.bottom + q.height) / z),
                    "GetBottom" => out!(q.bottom / z),
                    _ => out!((
                        (q.left + q.width / 2.0) / z,
                        (q.bottom + q.height / 2.0) / z
                    )),
                }
            }
            out!(());
        }
        "SetValue" => r.value = n.clamp(r.min, r.max),
        "GetValue" => out!(r.value),
        "SetMinMaxValues" => {
            r.min = n;
            r.max = number(v.get(1), 1.0).max(n);
            r.value = r.value.clamp(r.min, r.max);
        }
        "GetMinMaxValues" => out!((r.min, r.max)),
        "SetMovable" => r.movable = boolean(v.first()),
        "IsMovable" => out!(r.movable),
        "SetClampedToScreen" => r.clamped = boolean(v.first()),
        "IsClampedToScreen" => out!(r.clamped),
        "SetDesaturated" => r.visual.desaturated = boolean(v.first()),
        "IsDesaturated" => out!(r.visual.desaturated),
        "RegisterForClicks" => r.click_buttons = v.iter().filter_map(|v| string(Some(v))).collect(),
        "RegisterForDrag" => r.drag_buttons = v.iter().filter_map(|v| string(Some(v))).collect(),
        "SetMotionScriptsWhileDisabled" => r.motion_disabled = boolean(v.first()),
        "SetAutoFocus" => r.auto_focus = boolean(v.first()),
        "IsAutoFocus" => out!(r.auto_focus),
        "SetCursorPosition" => {
            let text = r.text.as_deref().unwrap_or("");
            let mut pos = (n.max(0.0) as usize).min(text.len());
            while !text.is_char_boundary(pos) {
                pos -= 1;
            }
            r.cursor_position = pos;
            r.selection = (pos, pos);
        }
        "GetCursorPosition" => out!(r.cursor_position),
        "HighlightText" => {
            let len = r.text.as_deref().unwrap_or("").len();
            let start = (n.max(0.0) as usize).min(len);
            let end = number(v.get(1), -1.0);
            let end = if end < 0.0 {
                len
            } else {
                (end as usize).min(len)
            };
            r.selection = (start.min(end), start.max(end));
        }
        "StartMoving" if r.movable => {
            r.moving = true;
            a.moving = Some(i);
        }
        "StartMoving" => {}
        "StopMovingOrSizing" => {
            r.moving = false;
            if a.moving == Some(i) {
                a.moving = None;
            }
        }
        "SetFocus" => focus(&mut a, ctx, Some(i)),
        "ClearFocus" => {
            if a.focus == Some(i) {
                focus(&mut a, ctx, None);
            }
        }
        "HasFocus" => out!(a.focus == Some(i)),
        _ => {
            ctx.not_simulated(&format!("{}:{method}", r.kind));
            out!(());
        }
    }
    let r = &a.regions[i];
    let geometry_changed = old.parent != r.parent
        || old.width != r.width
        || old.height != r.height
        || old.scale != r.scale
        || old.alpha != r.alpha
        || old.shown != r.shown
        || old.strata != r.strata
        || old.level != r.level
        || old.points != r.points
        || old.font_path != r.font_path
        || old.font_size != r.font_size
        || old.font_flags != r.font_flags
        || old.text != r.text
        || old.text_value != r.text_value
        || old.visual != r.visual
        || old.font_overrides != r.font_overrides
        || old.scroll != r.scroll
        || old.value != r.value
        || old.min != r.min
        || old.max != r.max
        || old.orientation != r.orientation
        || old.scroll_child != r.scroll_child;
    if geometry_changed
        && ((method == "SetAtlas" && boolean(v.get(1)))
            || !matches!(
                method,
                "SetTexture"
                    | "SetAtlas"
                    | "SetColorTexture"
                    | "SetGradient"
                    | "SetGradientAlpha"
                    | "SetTexCoord"
                    | "SetVertexColor"
                    | "SetBlendMode"
            ))
    {
        a.dirty = true;
        a.dirty_regions.insert(i);
        layout::resolve(&mut a, ctx);
    }
    let batched = a.layout_batches > 0;
    let new = a.regions[i].clone();
    drop(a);
    if !batched && new.rect.is_none() && (old.width, old.height) != (new.width, new.height) {
        callback(
            ctx,
            t.clone(),
            "OnSizeChanged",
            vec![Value::Number(new.width), Value::Number(new.height)],
        );
    }
    if old.value != new.value && new.kind == "Slider" {
        callback(
            ctx,
            t.clone(),
            "OnValueChanged",
            vec![
                Value::Number(new.value),
                Value::Boolean(method == "SetValue" && boolean(v.get(1))),
            ],
        );
    }
    for (changed, name, value) in [
        (
            old.scroll.0 != new.scroll.0,
            "OnHorizontalScroll",
            new.scroll.0,
        ),
        (
            old.scroll.1 != new.scroll.1,
            "OnVerticalScroll",
            new.scroll.1,
        ),
    ] {
        if changed {
            callback(ctx, t.clone(), name, vec![Value::Number(value)]);
        }
    }
    if old.enabled != new.enabled {
        sync_button_textures(lua, ctx, &t)?;
        callback(
            ctx,
            t.clone(),
            if new.enabled { "OnEnable" } else { "OnDisable" },
            vec![],
        );
    }
    if method == "SetMinMaxValues" && (old.min, old.max) != (new.min, new.max) {
        callback(
            ctx,
            t.clone(),
            "OnMinMaxChanged",
            vec![Value::Number(new.min), Value::Number(new.max)],
        );
    }
    if method == "SetText" && new.kind == "EditBox" {
        callback(ctx, t.clone(), "OnTextSet", vec![]);
    }
    if old.text != new.text {
        callback(ctx, t.clone(), "OnTextChanged", vec![Value::Boolean(false)]);
    }
    if matches!(method, "SetFont" | "SetTexture") {
        return true.into_lua_multi(lua);
    }
    Ok(MultiValue::new())
}

fn has_script(kind: &str, name: &str) -> bool {
    if matches!(name, "OnShow" | "OnHide" | "OnSizeChanged") {
        return true;
    }
    if is_region(kind) {
        return false;
    }
    if matches!(
        name,
        "OnEvent"
            | "OnUpdate"
            | "OnEnter"
            | "OnLeave"
            | "OnMouseDown"
            | "OnMouseUp"
            | "OnMouseWheel"
            | "OnDragStart"
            | "OnDragStop"
            | "OnReceiveDrag"
            | "OnAttributeChanged"
            | "OnKeyDown"
            | "OnKeyUp"
    ) {
        return true;
    }
    match kind {
        "Button" | "CheckButton" => matches!(
            name,
            "PreClick" | "OnClick" | "PostClick" | "OnDoubleClick" | "OnEnable" | "OnDisable"
        ),
        "EditBox" => matches!(
            name,
            "OnEnterPressed"
                | "OnEscapePressed"
                | "OnTabPressed"
                | "OnSpacePressed"
                | "OnTextChanged"
                | "OnTextSet"
                | "OnChar"
                | "OnCursorChanged"
                | "OnEditFocusGained"
                | "OnEditFocusLost"
                | "OnInputLanguageChanged"
        ),
        "Slider" | "StatusBar" => matches!(name, "OnValueChanged" | "OnMinMaxChanged"),
        "ScrollFrame" => matches!(
            name,
            "OnHorizontalScroll" | "OnVerticalScroll" | "OnScrollRangeChanged"
        ),
        "Cooldown" => matches!(name, "OnCooldownDone"),
        _ => false,
    }
}
