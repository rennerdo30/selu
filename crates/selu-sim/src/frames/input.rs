//! Host input in screen pixels, bottom-left origin. Session may forward these Sim methods.
use super::{arena::*, layout, widgets::callback};
use crate::session::Vm as Sim;
use crate::{RegionId, SimResult, core::dispatch};
use mlua::{Function, MultiValue, Value};
fn invoke(sim: &Sim, i: usize, name: &str, args: Vec<Value>) -> SimResult<()> {
    let t = store(sim.lua()).borrow().regions[i].table.get();
    if let Some(f) = t
        .raw_get::<mlua::Table>("__scripts")?
        .raw_get::<Option<Function>>(name)?
    {
        let mut args = MultiValue::from_vec(args);
        args.push_front(Value::Table(t));
        let result = crate::core::script_callback(sim.lua(), sim.ctx(), || {
            crate::core::call_lua::<()>(sim.lua(), f, args)
        });
        crate::core::flush(sim.lua(), sim.ctx());
        if let Err(error) = result {
            if *crate::core::lock(&sim.ctx().tolerant_ui) {
                sim.ctx().log(format!("error: {error}"));
                crate::core::check_budget(sim.ctx())?;
            } else {
                return Err(error);
            }
        }
    }
    crate::core::flush(sim.lua(), sim.ctx());
    Ok(())
}
fn eligible(sim: &Sim, i: usize) -> bool {
    let a = store(sim.lua());
    let mut a = a.borrow_mut();
    layout::resolve(&mut a, sim.ctx());
    let r = &a.regions[i];
    r.visible && r.enabled && r.mouse && a.capture == Some(i)
}
fn hit_motion(sim: &Sim, x: f64, y: f64, motion: bool, wheel: bool) -> Option<usize> {
    let a = store(sim.lua());
    let mut a = a.borrow_mut();
    layout::resolve(&mut a, sim.ctx());
    a.regions
        .iter()
        .enumerate()
        .filter(|(index, r)| {
            r.visible
                && (r.mouse || (wheel && r.wheel))
                && (r.enabled || (motion && r.motion_disabled))
                && r.rect.is_some_and(|q| {
                    x >= q.left
                        && x <= q.left + q.width
                        && y >= q.bottom
                        && y <= q.bottom + q.height
                })
                && clip(&a, *index).is_none_or(|q| {
                    x >= q.left && x < q.left + q.width && y >= q.bottom && y < q.bottom + q.height
                })
        })
        .max_by_key(|(i, r)| (r.strata, r.level, r.layer, *i))
        .map(|(i, _)| i)
}
fn hit(sim: &Sim, x: f64, y: f64) -> Option<usize> {
    hit_motion(sim, x, y, false, false)
}
impl crate::session::Vm {
    pub fn hover(&self, x: f64, y: f64) -> SimResult<Option<RegionId>> {
        dispatch(self.lua(), self.ctx(), || {
            let target = hit_motion(self, x, y, true, false);
            let a = store(self.lua());
            let mut a = a.borrow_mut();
            let old = a.hover;
            a.cursor = (x, y);
            a.hover = target;
            let old_table = old.map(|i| a.regions[i].table.get());
            let new_table = target.map(|i| a.regions[i].table.get());
            drop(a);
            if old != target {
                if let Some(table) = &old_table {
                    super::widgets::sync_button_textures(self.lua(), self.ctx(), table)?;
                }
                if let Some(table) = &new_table {
                    super::widgets::sync_button_textures(self.lua(), self.ctx(), table)?;
                }
                if let Some(t) = old_table {
                    callback(self.ctx(), t, "OnLeave", vec![Value::Boolean(true)]);
                }
                if let Some(t) = new_table {
                    callback(self.ctx(), t, "OnEnter", vec![Value::Boolean(true)]);
                }
            }
            Ok(target.map(|i| RegionId {
                index: i as u32,
                generation: 0,
            }))
        })
    }
    pub fn mouse_down(&self, x: f64, y: f64, button: &str) -> SimResult<Option<RegionId>> {
        self.button_phase(x, y, button, true)
    }
    pub fn mouse_up(&self, x: f64, y: f64, button: &str) -> SimResult<Option<RegionId>> {
        self.button_phase(x, y, button, false)
    }
    fn button_phase(
        &self,
        x: f64,
        y: f64,
        button: &str,
        down: bool,
    ) -> SimResult<Option<RegionId>> {
        dispatch(self.lua(), self.ctx(), || {
            self.hover(x, y)?;
            crate::core::flush(self.lua(), self.ctx());
            let a = store(self.lua());
            let target = if down {
                let target = hit(self, x, y);
                a.borrow_mut().capture = target;
                target
            } else {
                a.borrow().capture
            };
            if let Some(i) = target {
                let table = a.borrow().regions[i].table.get();
                {
                    let mut a = a.borrow_mut();
                    let r = &mut a.regions[i];
                    if !r.button_state_locked {
                        r.button_state = if down { "PUSHED" } else { "NORMAL" };
                    }
                }
                super::widgets::sync_button_textures(self.lua(), self.ctx(), &table)?;
                if down && a.borrow().regions[i].kind == "EditBox" {
                    focus(&mut a.borrow_mut(), self.ctx(), Some(i));
                    crate::core::flush(self.lua(), self.ctx());
                }
                let value = Value::String(self.lua().create_string(button)?);
                if down || eligible(self, i) {
                    invoke(
                        self,
                        i,
                        if down { "OnMouseDown" } else { "OnMouseUp" },
                        vec![value.clone()],
                    )?;
                }
                let suffix = if down { "Down" } else { "Up" };
                let registered = a.borrow().regions[i]
                    .click_buttons
                    .iter()
                    .any(|b| b == &format!("{button}{suffix}") || b == &format!("Any{suffix}"));
                if eligible(self, i) && registered && (down || hit(self, x, y) == Some(i)) {
                    {
                        let mut a = a.borrow_mut();
                        if a.regions[i].kind == "CheckButton" {
                            a.regions[i].checked = !a.regions[i].checked;
                        }
                    }
                    for name in ["PreClick", "OnClick", "PostClick"] {
                        if !eligible(self, i) {
                            break;
                        }
                        invoke(self, i, name, vec![value.clone(), Value::Boolean(down)])?;
                    }
                }
            }
            if !down {
                a.borrow_mut().capture = None;
            }
            Ok(target.map(|i| RegionId {
                index: i as u32,
                generation: 0,
            }))
        })
    }
    pub fn click(&self, x: f64, y: f64, button: &str) -> SimResult<Option<RegionId>> {
        dispatch(self.lua(), self.ctx(), || {
            let target = self.mouse_down(x, y, button)?;
            self.mouse_up(x, y, button)?;
            Ok(target)
        })
    }
    pub fn drag(&self, from: (f64, f64), to: (f64, f64)) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let a = store(self.lua());
            a.borrow_mut().cursor = from;
            let target = hit(self, from.0, from.1);
            a.borrow_mut().capture = target;
            let button = Value::String(self.lua().create_string("LeftButton")?);
            if let Some(i) = target {
                let registered = a.borrow().regions[i]
                    .drag_buttons
                    .iter()
                    .any(|b| b == "LeftButton");
                if registered {
                    invoke(self, i, "OnDragStart", vec![button.clone()])?;
                }
                {
                    let mut a = a.borrow_mut();
                    a.cursor = to;
                    if let Some(moving) = a.moving {
                        let screen = a.regions[0].rect;
                        let r = &mut a.regions[moving];
                        let mut dx = to.0 - from.0;
                        let mut dy = to.1 - from.1;
                        if r.clamped
                            && let (Some(q), Some(screen)) = (r.rect, screen)
                        {
                            dx = dx.clamp(
                                screen.left - q.left,
                                (screen.left + screen.width - q.left - q.width)
                                    .max(screen.left - q.left),
                            );
                            dy = dy.clamp(
                                screen.bottom - q.bottom,
                                (screen.bottom + screen.height - q.bottom - q.height)
                                    .max(screen.bottom - q.bottom),
                            );
                        }
                        let scale = r.effective_scale;
                        for point in &mut r.points {
                            point.x += dx / scale;
                            point.y += dy / scale;
                        }
                        a.dirty = true;
                        a.full_layout = true;
                    }
                }
                if eligible(self, i) {
                    let value = {
                        let arena = a.borrow();
                        let r = &arena.regions[i];
                        r.rect.filter(|_| r.kind == "Slider").map(|q| {
                            let horizontal = r.orientation == "HORIZONTAL";
                            let thumb = r.thumb.map(|t| &arena.regions[t]);
                            let extent = thumb
                                .map(|t| if horizontal { t.width } else { t.height })
                                .unwrap_or(0.0)
                                * r.effective_scale;
                            let length = if horizontal { q.width } else { q.height };
                            let position = if horizontal {
                                to.0 - q.left
                            } else {
                                q.bottom + q.height - to.1
                            };
                            let fraction = if length > extent {
                                ((position - extent / 2.0) / (length - extent)).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let value = r.min + fraction * (r.max - r.min);
                            if r.obey_step && r.step > 0.0 {
                                ((value - r.min) / r.step + 0.5).floor() * r.step + r.min
                            } else {
                                value
                            }
                        })
                    };
                    if let Some(value) = value {
                        let t = a.borrow().regions[i].table.get();
                        let set: Function = t.get("SetValue")?;
                        set.call::<()>((t, value, true))?;
                    }
                }
                if registered {
                    invoke(self, i, "OnDragStop", vec![button])?;
                }
                {
                    let mut arena = a.borrow_mut();
                    if !arena.regions[i].button_state_locked {
                        arena.regions[i].button_state = "NORMAL";
                    }
                }
                let table = a.borrow().regions[i].table.get();
                super::widgets::sync_button_textures(self.lua(), self.ctx(), &table)?;
            } else {
                a.borrow_mut().cursor = to;
            }
            a.borrow_mut().capture = None;
            Ok(())
        })
    }
    /// Deliver wheel motion; scripts own the policy for changing slider values.
    pub fn wheel(&self, x: f64, y: f64, delta: f64) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let mut target = hit_motion(self, x, y, false, true);
            while let Some(i) = target {
                let a = store(self.lua());
                let arena = a.borrow();
                let r = &arena.regions[i];
                let enabled = r.visible && r.enabled && r.wheel;
                target = r.parent;
                drop(arena);
                if enabled {
                    invoke(self, i, "OnMouseWheel", vec![Value::Number(delta)])?;
                    break;
                }
            }
            Ok(())
        })
    }
    /// Keyboard frames receive input from top to bottom until propagation stops.
    pub fn key(&self, key: &str, down: bool) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let a = store(self.lua());
            let key = key.to_ascii_uppercase();
            let changed = if down {
                a.borrow_mut().keys.insert(key.clone())
            } else {
                a.borrow_mut().keys.remove(&key)
            };
            if changed
                && matches!(
                    key.as_str(),
                    "LSHIFT" | "RSHIFT" | "LCTRL" | "RCTRL" | "LALT" | "RALT"
                )
            {
                self.fire(
                    "MODIFIER_STATE_CHANGED",
                    vec![
                        Value::String(self.lua().create_string(&key)?),
                        Value::Integer(i64::from(down)),
                    ],
                )?;
            }
            let focus = a.borrow().focus;
            if down
                && let Some(i) = focus
                && a.borrow().regions[i].visible
            {
                let script = match key.as_str() {
                    "ESCAPE" => Some("OnEscapePressed"),
                    "ENTER" => Some("OnEnterPressed"),
                    "TAB" => Some("OnTabPressed"),
                    _ => None,
                };
                if let Some(script) = script {
                    invoke(self, i, script, vec![])?;
                    return Ok(());
                }
            }
            let mut targets = a
                .borrow()
                .regions
                .iter()
                .enumerate()
                .filter(|(i, r)| {
                    r.visible
                        && r.enabled
                        && r.keyboard
                        && (r.kind != "EditBox" || focus == Some(*i))
                })
                .map(|(i, r)| (r.strata, r.level, i))
                .collect::<Vec<_>>();
            targets.sort();
            let mut consumed = false;
            for (_, _, i) in targets.into_iter().rev() {
                invoke(
                    self,
                    i,
                    if down { "OnKeyDown" } else { "OnKeyUp" },
                    vec![Value::String(self.lua().create_string(&key)?)],
                )?;
                if !a.borrow().regions[i].propagate_keyboard {
                    consumed = true;
                    break;
                }
            }
            if !consumed {
                crate::bindings::key(self.lua(), &key, down)?;
            }
            Ok(())
        })
    }
    pub fn press_enter(&self) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let a = store(self.lua());
            let a = a.borrow();
            let t = a
                .focus
                .filter(|&i| a.regions[i].enabled && a.regions[i].keyboard && a.regions[i].visible)
                .map(|i| a.regions[i].table.get());
            drop(a);
            if let Some(t) = t {
                callback(self.ctx(), t, "OnEnterPressed", vec![]);
            }
            Ok(())
        })
    }
    pub fn type_text(&self, text: &str) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let a = store(self.lua());
            let mut a = a.borrow_mut();
            if let Some(i) = a
                .focus
                .filter(|&i| a.regions[i].enabled && a.regions[i].keyboard && a.regions[i].visible)
            {
                let r = &mut a.regions[i];
                let mut current = r.text.clone().unwrap_or_default();
                let (mut start, mut end) = r.selection;
                if start == end {
                    start = r.cursor_position;
                    end = start;
                }
                start = start.min(current.len());
                end = end.min(current.len());
                while !current.is_char_boundary(start) {
                    start -= 1;
                }
                while !current.is_char_boundary(end) {
                    end -= 1;
                }
                current.replace_range(start..end, text);
                r.cursor_position = start + text.len();
                r.selection = (r.cursor_position, r.cursor_position);
                r.text = Some(current);
                r.text_value = Value::Nil;
                let t = r.table.get();
                a.dirty = true;
                a.full_layout = true;
                drop(a);
                callback(self.ctx(), t, "OnTextChanged", vec![Value::Boolean(true)]);
            }
            Ok(())
        })
    }
}
#[cfg(test)]
mod sf_tests {
    #[test]
    fn hover_callbacks_receive_native_motion_boolean() {
        let sim = crate::session::Vm::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:SetSize(100, 100)
            f:SetPoint("BOTTOMLEFT", UIParent, "BOTTOMLEFT", 0, 0)
            f:EnableMouse(true)
            f:SetScript("OnEnter", function(self, motion) print("enter", type(motion), motion) end)
            f:SetScript("OnLeave", function(self, motion) print("leave", type(motion), motion) end)
        "#,
        )
        .unwrap();
        sim.hover(50.0, 50.0).unwrap();
        sim.hover(200.0, 200.0).unwrap();
        assert_eq!(
            sim.prints(),
            vec!["enter\tboolean\ttrue", "leave\tboolean\ttrue"]
        );
    }
}

#[cfg(test)]
mod w3_tests {
    #[test]
    fn w3_desaturation_state() {
        let sim = crate::session::Vm::new().unwrap();
        sim.eval(
            r#"
            local t = UIParent:CreateTexture()
            print(t:IsDesaturated())
            t:SetDesaturated(true)
            print(t:IsDesaturated())
            t:SetDesaturated(false)
            print(t:IsDesaturated())
        "#,
        )
        .unwrap();
        assert_eq!(sim.prints(), vec!["false", "true", "false"]);
    }

    #[test]
    fn w3_click_registration() {
        let sim = crate::session::Vm::new().unwrap();
        sim.eval(
            r#"
            b = CreateFrame("Button")
            b:SetSize(100,100)
            b:SetPoint("BOTTOMLEFT", UIParent, "BOTTOMLEFT",0,0)
            b:SetScript("OnClick", function(self, button, down) print(button, down) end)
        "#,
        )
        .unwrap();
        sim.click(50.0, 50.0, "RightButton").unwrap();
        assert!(sim.prints().is_empty());
        sim.eval(r#"b:RegisterForClicks("RightButtonDown", "RightButtonUp")"#)
            .unwrap();
        sim.click(50.0, 50.0, "RightButton").unwrap();
        assert_eq!(
            sim.prints(),
            vec!["RightButton\ttrue", "RightButton\tfalse"]
        );
        sim.eval("b:RegisterForClicks()").unwrap();
        sim.click(50.0, 50.0, "RightButton").unwrap();
        assert_eq!(sim.prints().len(), 2);
    }
    #[test]
    fn w3_wheel_ancestor() {
        let sim = crate::session::Vm::new().unwrap();
        sim.eval(
            r#"
            local p = CreateFrame("Frame")
            p:SetSize(100,100)
            p:SetPoint("BOTTOMLEFT", UIParent, "BOTTOMLEFT",0,0)
            p:EnableMouseWheel(true)
            p:SetScript("OnMouseWheel", function(self, delta) print(delta) end)
            local b = CreateFrame("Button", nil, p)
            b:SetAllPoints(p)
        "#,
        )
        .unwrap();
        sim.wheel(50.0, 50.0, 1.0).unwrap();
        assert_eq!(sim.prints(), vec!["1"]);
    }
    #[test]
    fn w3_editbox_click_focus() {
        let sim = crate::session::Vm::new().unwrap();
        sim.eval(
            r#"
            e = CreateFrame("EditBox")
            e:SetAutoFocus(false)
            e:SetSize(100,100)
            e:SetPoint("BOTTOMLEFT", UIParent, "BOTTOMLEFT",0,0)
        "#,
        )
        .unwrap();
        sim.click(50.0, 50.0, "LeftButton").unwrap();
        sim.type_text("typed").unwrap();
        sim.eval("print(e:HasFocus(), e:GetText())").unwrap();
        assert_eq!(sim.prints(), vec!["true\ttyped"]);
    }
}
