//! Host input in screen pixels, bottom-left origin. Session may forward these Sim methods.
use super::{arena::*, layout, widgets::callback};
use crate::{RegionId, Sim, SimResult, core::dispatch};
use mlua::Value;
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
impl Sim {
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
                    callback(self.ctx(), t, "OnLeave", vec![]);
                }
                if let Some(t) = new_table {
                    callback(self.ctx(), t, "OnEnter", vec![]);
                }
            }
            Ok(target.map(|i| RegionId {
                index: i as u32,
                generation: 0,
            }))
        })
    }
    pub fn click(&self, x: f64, y: f64, button: &str) -> SimResult<Option<RegionId>> {
        self.hover(x, y)?;
        dispatch(self.lua(), self.ctx(), || {
            let target = hit(self, x, y);
            if let Some(i) = target {
                let a = store(self.lua());
                let mut a = a.borrow_mut();
                let r = &mut a.regions[i];
                if r.kind == "CheckButton" {
                    r.checked = !r.checked;
                }
                let t = r.table.clone();
                drop(a);
                let b = Value::String(self.lua().create_string(button)?);
                for name in ["OnMouseDown", "OnMouseUp", "OnClick"] {
                    callback(self.ctx(), t.clone(), name, vec![b.clone()]);
                }
            }
            Ok(target.map(|i| RegionId {
                index: i as u32,
                generation: 0,
            }))
        })
    }
    pub fn drag(&self, from: (f64, f64), to: (f64, f64)) -> SimResult<()> {
        let target = hit(self, from.0, from.1);
        dispatch(self.lua(), self.ctx(), || {
            if let Some(i) = target {
                let t = store(self.lua()).borrow().regions[i].table.clone();
                callback(self.ctx(), t, "OnDragStart", vec![]);
            }
            Ok(())
        })?;
        dispatch(self.lua(), self.ctx(), || {
            if let Some(i) = target {
                let a = store(self.lua());
                let mut a = a.borrow_mut();
                let r = &mut a.regions[i];
                if r.moving {
                    let scale = r.effective_scale;
                    for p in &mut r.points {
                        p.x += (to.0 - from.0) / scale;
                        p.y += (to.1 - from.1) / scale;
                    }
                }
                let t = r.table.clone();
                a.cursor = to;
                a.dirty = true;
                drop(a);
                callback(self.ctx(), t, "OnDragStop", vec![]);
            }
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
                let t = r.table.clone();
                drop(a);
                callback(self.ctx(), t, "OnTextChanged", vec![Value::Boolean(true)]);
            }
            Ok(())
        })
    }
}
