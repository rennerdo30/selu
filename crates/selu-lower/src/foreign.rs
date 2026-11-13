//! Declared other addons (docs/lang/addons.md, checker `foreign`): the live,
//! checked globals of `addons/<Name>.selu`, checked field reads, and
//! protected calls whose results and callback arguments are checked.
use crate::{
    Lowerer,
    lua::{self, LExpr, LStmt, LStmtKind, name},
};
use selu_check::foreign::ForeignCall;
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind};

/// Shared by every module that crosses the boundary. A value that does not
/// match its declaration, or a call that raises, gives nil and one warning
/// per place.
pub(crate) const RUNTIME: &str = r##"local __selu_fx_type, __selu_fx_pcall, __selu_fx_print, __selu_fx_tostring, __selu_fx_unpack, __selu_fx_tonumber = type, pcall, print, tostring, unpack, tonumber
local __selu_fx_warned, __selu_fx_parsed = {}, {}
local function __selu_fwarn(path, what)
  if __selu_fx_warned[path] then return end
  __selu_fx_warned[path] = true
  __selu_fx_print("|cffff8800" .. __selu_fx_tostring(__selu_addon) .. "|r: " .. path .. " " .. what .. "; it is ignored.")
end
local function __selu_fparse(d, i)
  local node = {}
  local c = d:sub(i, i)
  if c == "?" then node.opt = true; i = i + 1; c = d:sub(i, i) end
  if c == "a" then
    local e
    e, i = __selu_fparse(d, i + 1)
    node.k, node.e = "a", e
    return node, i
  end
  if c == "(" then
    node.k, node.alts = "|", {}
    i = i + 1
    while true do
      local alt
      alt, i = __selu_fparse(d, i)
      node.alts[#node.alts + 1] = alt
      local sep = d:sub(i, i)
      i = i + 1
      if sep ~= "|" then break end
    end
    return node, i
  end
  if c == "'" then
    local j, text = i + 1, ""
    while j <= #d do
      local ch = d:sub(j, j)
      if ch == "\\" then text = text .. d:sub(j + 1, j + 1); j = j + 2
      elseif ch == "'" then break
      else text = text .. ch; j = j + 1 end
    end
    node.k, node.v = "=", text
    return node, j + 1
  end
  if c == "#" then
    local j = d:find(";", i, true)
    node.k, node.v = "=", __selu_fx_tonumber(d:sub(i + 1, j - 1))
    return node, j + 1
  end
  node.k = c
  return node, i + 1
end
local function __selu_fok(v, n)
  if v == nil then return n.opt == true end
  local k = n.k
  if k == "u" then return true end
  local t = __selu_fx_type(v)
  if k == "n" then return t == "number" end
  if k == "s" then return t == "string" end
  if k == "b" then return t == "boolean" end
  if k == "t" then return t == "table" end
  if k == "f" then return t == "function" end
  if k == "=" then return v == n.v end
  if k == "a" then
    if t ~= "table" then return false end
    for i = 1, #v do
      if not __selu_fok(v[i], n.e) then return false end
    end
    return true
  end
  if k == "|" then
    for i = 1, #n.alts do
      if __selu_fok(v, n.alts[i]) then return true end
    end
  end
  return false
end
local function __selu_fcheck(v, d, path)
  local n = __selu_fx_parsed[d]
  if n == nil then
    n = __selu_fparse(d, 1)
    __selu_fx_parsed[d] = n
  end
  if __selu_fok(v, n) then return v end
  __selu_fwarn(path, "does not match its declaration")
  return nil
end
local function __selu_fidx(o, k) return o[k] end
local function __selu_fread(o, k, d, path)
  if __selu_fx_type(o) ~= "table" then return nil end
  local ok, v = __selu_fx_pcall(__selu_fidx, o, k)
  if not ok then
    __selu_fwarn(path, "raised an error when read")
    return nil
  end
  return __selu_fcheck(v, d, path)
end
local function __selu_fresult(path, d, ok, v)
  if not ok then
    __selu_fwarn(path, "raised an error")
    return nil
  end
  return __selu_fcheck(v, d, path)
end
local function __selu_fcall(f, d, path, ...)
  if __selu_fx_type(f) ~= "function" then
    __selu_fwarn(path, "is not a function")
    return nil
  end
  return __selu_fresult(path, d, __selu_fx_pcall(f, ...))
end
local function __selu_fcallfield(o, k, d, path, ...)
  if __selu_fx_type(o) ~= "table" then return nil end
  local ok, f = __selu_fx_pcall(__selu_fidx, o, k)
  if not ok or __selu_fx_type(f) ~= "function" then
    __selu_fwarn(path, "is not a function")
    return nil
  end
  return __selu_fresult(path, d, __selu_fx_pcall(f, ...))
end
local function __selu_fcallback(fn, ds, path)
  return function(...)
    local args = { ... }
    for i = 1, #ds do args[i] = __selu_fcheck(args[i], ds[i], path) end
    return fn(__selu_fx_unpack(args, 1, #ds))
  end
end"##;

impl Lowerer<'_> {
    /// `addon "Name" { global X: T; }`: each global is a getter the
    /// importing modules call on every read.
    pub(crate) fn addon_decl(&mut self, s: &Stmt, out: &mut Vec<LStmt>) {
        let StmtKind::AddonDecl { .. } = &s.kind else {
            return;
        };
        for g in self.types.foreign_globals() {
            let path = format!("{} {}", g.addon, g.global);
            out.push(LStmt::new(
                LStmtKind::Raw(format!(
                    "{table}.{global} = function()\n  local ok, loaded = __selu_fx_pcall({loaded}, {addon:?})\n  if not (ok and loaded == true) then return nil end\n  return __selu_fcheck({global}, {desc:?}, {path:?})\nend",
                    table = lua::print_expr(&crate::modules::module_table(&self.module.key)),
                    global = g.global,
                    loaded = g.loaded,
                    addon = g.addon,
                    desc = g.check.descriptor(),
                )),
                &s.span,
            ));
        }
    }

    /// A checked field read of another addon's table.
    pub(crate) fn foreign_read(&mut self, e: &Expr, pre: &mut Vec<LStmt>) -> Option<LExpr> {
        let read = self.types.foreign_read(e.id)?.clone();
        let ExprKind::Member { object, name: field } = &e.kind else {
            return None;
        };
        let o = self.expr(object, pre);
        Some(lua::call(
            name("__selu_fread"),
            vec![
                o,
                LExpr::Str(field.clone()),
                LExpr::Str(read.check.descriptor()),
                LExpr::Str(read.path),
            ],
        ))
    }

    /// A protected call into another addon.
    pub(crate) fn foreign_call(&mut self, e: &Expr, pre: &mut Vec<LStmt>) -> Option<LExpr> {
        let call: ForeignCall = self.types.foreign_call(e.id)?.clone();
        let ExprKind::Call { callee, args } = &e.kind else {
            return None;
        };
        let mut lowered = Vec::new();
        let (function, head) = match (&call.field, &callee.kind) {
            (Some(field), ExprKind::Member { object, .. }) => {
                let o = self.expr(object, pre);
                ("__selu_fcallfield", vec![o, LExpr::Str(field.clone())])
            }
            _ => {
                let f = self.expr(callee, pre);
                ("__selu_fcall", vec![f])
            }
        };
        lowered.extend(head);
        lowered.push(LExpr::Str(call.result.descriptor()));
        lowered.push(LExpr::Str(call.path.clone()));
        for (i, arg) in args.iter().enumerate() {
            let mut v = self.expr(arg, pre);
            if let Some((_, checks)) = call.callbacks.iter().find(|(at, _)| *at == i) {
                v = lua::call(
                    name("__selu_fcallback"),
                    vec![
                        v,
                        LExpr::Table(
                            checks
                                .iter()
                                .map(|c| lua::LField::Pos(LExpr::Str(c.descriptor())))
                                .collect(),
                        ),
                        LExpr::Str(format!("{} callback", call.path)),
                    ],
                );
            }
            lowered.push(v);
        }
        Some(lua::call(name(function), lowered))
    }
}
