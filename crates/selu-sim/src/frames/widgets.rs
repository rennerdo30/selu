use super::{arena::*, layout};
use crate::core::{Ctx, DrawLayer, Strata, TextureFill};
use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};
use std::{cell::RefCell, rc::Rc};
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
pub fn callback(ctx: &Ctx, t: Table, name: &str, args: Vec<Value>) {
    let name = name.to_owned();
    let ctx2 = ctx.clone();
    ctx.queue(Box::new(move |lua| {
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
    }));
}
pub fn install(lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
    lua.globals().set(
        "CreateFont",
        lua.create_function(|lua, name: String| crate::sim_globals::create_font(lua, &name))?,
    )?;
    lua.set_app_data(ctx.clone());
    lua.set_app_data(Rc::new(RefCell::new(Arena::default())) as Store);
    let ctx = ctx.clone();
    lua.globals().set(
        "CreateFrame",
        lua.create_function(
            move |lua,
                  (kind, name, parent, templates): (
                Option<String>,
                Option<String>,
                Option<Table>,
                Option<String>,
            )| {
                let t = create(lua, &ctx, kind.as_deref().unwrap_or("Frame"), name, parent)?;
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
    a.borrow_mut().regions.push(Region {
        table: t.clone(),
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
        visible: true,
        effective_scale: 1.0,
        effective_alpha: 1.0,
        text: None,
        texture: None,
        visual: crate::core::VisualStyle::default(),
        font_overrides: 0,
        clips_children: false,
        scroll_child: None,
        scroll: (0.0, 0.0),
        font_size: 12.0,
        font_path: "fixed".into(),
        font_flags: String::new(),
        text_value: Value::Nil,
        strata_explicit: false,
        level_explicit: false,
        mouse: matches!(kind, "Button" | "CheckButton" | "EditBox" | "Slider"),
        enabled: true,
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
    a.borrow_mut().dirty = true;
    // Shared method table per type. Missing addon fields remain nil.
    let key = format!("selu.methods.{kind}");
    let methods = if let Ok(m) = lua.named_registry_value::<Table>(&key) {
        m
    } else {
        let m = lua.create_table()?;
        for method in METHODS.split_whitespace() {
            let method = method.to_owned();
            let context = ctx.clone();
            m.set(
                method.clone(),
                lua.create_function(move |lua, args: MultiValue| {
                    call(lua, &context, &method, args)
                })?,
            )?;
        }
        lua.set_named_registry_value(&key, m.clone())?;
        m
    };
    let mt = lua.create_table()?;
    let context = ctx.clone();
    let widget = kind.to_owned();
    mt.set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, String)| {
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
    Ok(t)
}
const METHODS: &str = "SetFontObject GetFontObject Raise Lower SetClipsChildren DoesClipChildren GetScrollChild GetHorizontalScroll GetVerticalScroll GetHorizontalScrollRange GetVerticalScrollRange UpdateScrollChildRect SetShadowColor SetShadowOffset SetWordWrap SetMaxLines SetBlendMode GetName GetObjectType IsObjectType GetParent SetParent SetSize SetWidth SetHeight GetWidth GetHeight GetSize SetPoint GetPoint GetNumPoints ClearAllPoints SetAllPoints GetRect GetLeft GetRight GetTop GetBottom GetCenter SetScale GetScale GetEffectiveScale SetAlpha GetAlpha GetEffectiveAlpha SetFrameStrata GetFrameStrata SetFrameLevel GetFrameLevel SetDrawLayer GetDrawLayer Show Hide SetShown IsShown IsVisible IsProtected SetText GetText SetFont GetFont GetStringWidth GetStringHeight SetTexture GetTexture SetAtlas SetColorTexture SetScript GetScript HookScript HasScript RegisterEvent UnregisterEvent UnregisterAllEvents CreateTexture CreateMaskTexture CreateFontString EnableMouse IsMouseEnabled Enable Disable IsEnabled SetChecked GetChecked SetValue GetValue SetMinMaxValues GetMinMaxValues SetFocus ClearFocus HasFocus StartMoving StopMovingOrSizing RegisterForClicks RegisterForDrag SetMovable SetClampedToScreen SetNormalTexture SetHighlightTexture SetPushedTexture SetAutoFocus SetMultiLine SetJustifyH SetJustifyV SetTextColor SetTexCoord SetVertexColor SetScrollChild SetHorizontalScroll SetVerticalScroll SetCooldown";
fn call(lua: &Lua, ctx: &Ctx, method: &str, args: MultiValue) -> mlua::Result<MultiValue> {
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
        match method {
            "GetScript" => out!(scripts.raw_get::<Value>(name)?),
            "HasScript" => out!(has_script(&a.borrow().regions[i].kind, &name)),
            "SetScript" => scripts.raw_set(name, v.get(1).cloned().unwrap_or(Value::Nil))?,
            "HookScript" => {
                let old = scripts.raw_get::<Option<Function>>(name.clone())?;
                let hook = match v.get(1) {
                    Some(Value::Function(f)) => f.clone(),
                    _ => return Err(mlua::Error::runtime("function expected")),
                };
                let context = ctx.clone();
                scripts.raw_set(
                    name,
                    lua.create_function(move |lua, args: MultiValue| {
                        if let Some(old) = &old {
                            crate::core::script_callback(lua, &context, || {
                                crate::core::call_lua::<()>(lua, old.clone(), args.clone())
                            })?;
                            crate::core::flush(lua, &context);
                        }
                        let result = crate::core::script_callback(lua, &context, || {
                            crate::core::call_lua::<()>(lua, hook.clone(), args)
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
        a.regions[i].font_path = path;
        a.regions[i].font_size = size;
        a.regions[i].font_flags = flags;
        a.dirty = true;
        out!(());
    }
    if method == "SetFont" {
        t.raw_set("__font_object", Value::Nil)?;
    }
    if method == "GetFontObject" {
        out!(t.raw_get::<Value>("__font_object")?);
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
        }
        a.regions[i].scroll_child = child;
    }
    if method == "GetScrollChild" {
        out!(
            a.regions[i]
                .scroll_child
                .map(|c| a.regions[c].table.clone())
        );
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
    let r = &mut a.regions[i];
    r.font_overrides |= match method {
        "SetTextColor" => 1,
        "SetShadowColor" => 2,
        "SetShadowOffset" => 4,
        "SetJustifyH" => 8,
        "SetJustifyV" => 16,
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
        "SetWordWrap" => r.visual.wrap = boolean(v.first()),
        "SetMaxLines" => r.visual.max_lines = n.max(0.0) as usize,
        "SetBlendMode" => r.visual.blend = s,
        "SetShadowOffset" => r.visual.shadow_offset = (n, number(v.get(1), 0.0)),
        "SetTextColor" | "SetVertexColor" | "SetShadowColor" => {
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
        "GetName" => out!(r.name.clone()),
        "GetObjectType" => out!(r.kind.clone()),
        "IsObjectType" => out!(super::manifest::inherits(&r.kind, &s)),
        "GetParent" => {
            let p = r.parent;
            out!(p.map(|p| a.regions[p].table.clone()));
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
        "EnableMouse" => r.mouse = boolean(v.first()),
        "IsMouseEnabled" => out!(r.mouse),
        "Enable" => r.enabled = true,
        "Disable" => r.enabled = false,
        "IsEnabled" => out!(r.enabled),
        "SetChecked" => r.checked = boolean(v.first()),
        "GetChecked" => out!(r.checked),
        "SetText" => {
            r.text_value = v.first().cloned().unwrap_or(Value::Nil);
            r.text = if matches!(v.first(), None | Some(Value::Nil | Value::LightUserData(_))) {
                None
            } else {
                Some(crate::value_string(&v[0]))
            };
            if !matches!(r.text_value, Value::LightUserData(_)) {
                r.text_value = match &r.text {
                    Some(text) => Value::String(lua.create_string(text)?),
                    None => Value::Nil,
                };
            }
        }
        "GetText" => out!(r.text_value.clone()),
        "SetFont" => {
            r.font_size = number(v.get(1), 12.0);
            r.font_path = s;
            r.font_flags = string(v.get(2)).unwrap_or_default();
        }
        "GetFont" => out!((r.font_path.clone(), r.font_size, r.font_flags.clone())),
        "GetStringWidth" => {
            out!(crate::core::measure_text(r.display_text().unwrap_or(""), r.font_size).width)
        }
        "GetStringHeight" => out!(layout::text_height(
            r,
            r.rect
                .map(|q| q.width / r.effective_scale)
                .unwrap_or(r.width)
        )),
        "SetTexture" => {
            r.texture = if matches!(v.first(), None | Some(Value::Nil)) {
                None
            } else {
                Some(TextureFill::File(if s.is_empty() {
                    n.to_string()
                } else {
                    s
                }))
            };
        }
        "GetTexture" => out!(match &r.texture {
            Some(TextureFill::File(s)) => Some(s.clone()),
            _ => None,
        }),
        "SetAtlas" => r.texture = Some(TextureFill::Atlas(s)),
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
                    a.regions.get(p.relative).map(|r| r.table.clone()),
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
        "GetRect" | "GetLeft" | "GetRight" | "GetTop" | "GetBottom" | "GetCenter" => {
            if let Some(q) = r.rect {
                let z = r.effective_scale;
                match method {
                    "GetRect" => out!((q.left / z, q.bottom / z, q.width / z, q.height / z)),
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
        }
        "GetMinMaxValues" => out!((r.min, r.max)),
        "StartMoving" => {
            r.moving = true;
            a.moving = Some(i);
        }
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
        _ => ctx.not_simulated(&format!("{}:{method}", r.kind)),
    }
    let direct_visibility = matches!(method, "Show" | "Hide" | "SetShown");
    if direct_visibility {
        let parent_visible = a.regions[i]
            .parent
            .map(|p| a.regions[p].visible)
            .unwrap_or(true);
        a.regions[i].visible = a.regions[i].shown && parent_visible;
    }
    a.dirty = true;
    layout::resolve(&mut a, ctx);
    let new = a.regions[i].clone();
    drop(a);
    if direct_visibility
        && old.visible != new.visible
        && let Some(f) = t
            .raw_get::<Table>("__scripts")?
            .raw_get::<Option<Function>>(if new.visible { "OnShow" } else { "OnHide" })?
    {
        let result = crate::core::script_callback(lua, ctx, || {
            crate::core::call_lua::<()>(lua, f, t.clone())
        });
        crate::core::flush(lua, ctx);
        result?;
    }
    if new.rect.is_none() && (old.width, old.height) != (new.width, new.height) {
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
            vec![Value::Number(new.value), Value::Boolean(false)],
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
