//! Host input in screen pixels, bottom-left origin. Session may forward these Sim methods.
use super::{arena::*, layout, widgets::callback};
use crate::session::Vm as Sim;
use crate::{RegionId, SimResult, core::dispatch};
use mlua::{Function, MultiValue, Value};
fn invoke(sim: &Sim, i: usize, name: &str, args: Vec<Value>) -> SimResult<()> {
    let t = store(sim.lua()).borrow().regions[i].table.clone();
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
        result?;
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
fn hit(sim: &Sim, x: f64, y: f64) -> Option<usize> {
    let a = store(sim.lua());
    let mut a = a.borrow_mut();
    layout::resolve(&mut a, sim.ctx());
    a.regions
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.visible
                && r.mouse
                && r.enabled
                && r.rect.is_some_and(|q| {
                    x >= q.left
                        && x <= q.left + q.width
                        && y >= q.bottom
                        && y <= q.bottom + q.height
                })
        })
        .max_by_key(|(i, r)| (r.strata, r.level, r.layer, *i))
        .map(|(i, _)| i)
}
impl crate::session::Vm {
    pub fn hover(&self, x: f64, y: f64) -> SimResult<Option<RegionId>> {
        dispatch(self.lua(), self.ctx(), || {
            let target = hit(self, x, y);
            let a = store(self.lua());
            let mut a = a.borrow_mut();
            let old = a.hover;
            a.cursor = (x, y);
            a.hover = target;
            let old_table = old.map(|i| a.regions[i].table.clone());
            let new_table = target.map(|i| a.regions[i].table.clone());
            drop(a);
            if old != target {
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
    pub fn click(&self, x: f64, y: f64, button: &str) -> SimResult<Option<RegionId>> {
        dispatch(self.lua(), self.ctx(), || {
            self.hover(x, y)?;
            crate::core::flush(self.lua(), self.ctx());
            let target = hit(self, x, y);
            let a = store(self.lua());
            a.borrow_mut().capture = target;
            if let Some(i) = target {
                let button = Value::String(self.lua().create_string(button)?);
                invoke(self, i, "OnMouseDown", vec![button.clone()])?;
                if eligible(self, i) {
                    invoke(self, i, "OnMouseUp", vec![button.clone()])?;
                }
                if eligible(self, i) {
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
                        invoke(self, i, name, vec![button.clone(), Value::Boolean(false)])?;
                    }
                }
            }
            a.borrow_mut().capture = None;
            Ok(target.map(|i| RegionId {
                index: i as u32,
                generation: 0,
            }))
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
                invoke(self, i, "OnDragStart", vec![button.clone()])?;
                {
                    let mut a = a.borrow_mut();
                    a.cursor = to;
                    if let Some(moving) = a.moving {
                        let r = &mut a.regions[moving];
                        let scale = r.effective_scale;
                        for point in &mut r.points {
                            point.x += (to.0 - from.0) / scale;
                            point.y += (to.1 - from.1) / scale;
                        }
                        a.dirty = true;
                    }
                }
                invoke(self, i, "OnDragStop", vec![button])?;
            } else {
                a.borrow_mut().cursor = to;
            }
            a.borrow_mut().capture = None;
            Ok(())
        })
    }
    pub fn press_enter(&self) -> SimResult<()> {
        dispatch(self.lua(), self.ctx(), || {
            let a = store(self.lua());
            let a = a.borrow();
            let t = a.focus.map(|i| a.regions[i].table.clone());
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
            if let Some(i) = a.focus {
                let r = &mut a.regions[i];
                r.text = Some(format!("{}{text}", r.text.as_deref().unwrap_or("")));
                r.text_value = Value::Nil;
                let t = r.table.clone();
                a.dirty = true;
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
