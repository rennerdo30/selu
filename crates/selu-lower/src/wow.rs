use crate::Lowerer;
use crate::lua::{self, LBlock, LExpr, LStmt, LStmtKind, name};
use selu_syntax::{Expr, ExprKind};
#[derive(Default)]
pub(crate) struct WowState {
    pub keys: Vec<String>,
    pub used: bool,
}
impl Lowerer<'_> {
    pub(crate) fn wow_prologue(&self) -> Option<LBlock> {
        self.wow.used.then(|| vec![crate::raw(RUNTIME)])
    }
    pub(crate) fn tagged_template(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let ExprKind::TaggedTemplate { tag, quasis, exprs } = &e.kind else {
            unreachable!()
        };
        self.wow.used = true;
        let tag = tag.dotted_name().unwrap();
        let mut key = String::new();
        for (i, q) in quasis.iter().enumerate() {
            key.push_str(&q.replace('{', "{{").replace('}', "}}"));
            if i < exprs.len() {
                key.push_str(&format!("{{{i}}}"));
            }
        }
        let mut fmt = String::new();
        for (i, q) in quasis.iter().enumerate() {
            fmt.push_str(&q.replace('%', "%%"));
            if i < exprs.len() {
                fmt.push_str("%s");
            }
        }
        let mut body = vec![];
        let refs: Vec<_> = exprs.iter().collect();
        let vals = self
            .seq(&refs, &mut body)
            .into_iter()
            .map(|x| lua::call(name("tostring"), vec![x]))
            .collect::<Vec<_>>();
        if tag == "L" {
            if !self.wow.keys.contains(&key) {
                self.wow.keys.push(key.clone());
            }
            let fallback = if exprs.is_empty() {
                quasis[0].clone()
            } else {
                fmt
            };
            let mut args = vec![LExpr::Str(key), LExpr::Str(fallback)];
            args.extend(vals);
            pre.extend(body);
            lua::call(name("__selu_wow.text"), args)
        } else {
            let level = tag.strip_prefix("log.").unwrap();
            let mut args = vec![LExpr::Str(fmt)];
            args.extend(vals);
            body.push(LStmt::new(
                LStmtKind::Call(lua::call(
                    name("__selu_wow.write"),
                    vec![
                        LExpr::Str(level.into()),
                        LExpr::Str(key.clone()),
                        lua::call(name("string.format"), args),
                    ],
                )),
                &e.span,
            ));
            let test = lua::call(
                name("__selu_wow.enabled"),
                vec![LExpr::Str(level.into()), LExpr::Str(key)],
            );
            pre.push(LStmt::new(LStmtKind::If(vec![(test, body)], None), &e.span));
            LExpr::Nil
        }
    }
    pub(crate) fn builtin_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> Option<LExpr> {
        // Built-in callees are deliberately not typed; ordinary bound callees are.
        if self.types.get(callee.id).is_some() {
            return None;
        }
        let n = callee.dotted_name()?;
        if !matches!(n.as_str(), "command" | "hook" | "hookScript") {
            return None;
        }
        self.wow.used = true;
        let refs: Vec<_> = args.iter().collect();
        let mut vals = self.seq(&refs, pre);
        Some(match n.as_str() {
            "command" => lua::call(name("__selu_wow.command"), vals),
            "hook" => lua::call(name("hooksecurefunc"), vals),
            _ => {
                let obj = vals.remove(0);
                LExpr::Method(Box::new(obj), "HookScript".into(), vals)
            }
        })
    }
}
const RUNTIME: &str = r##"local ADDON, ns = ...
ns = ns or {}
if not ns.wow then
  local w = { buffer = {}, levels = { info = true, warn = true, error = true, warnOnce = true }, once = {}, commands = 0, locales = {}, formats = {} }
  ns.wow = w
  function w.enabled(level, key)
    return w.levels[level] and (level ~= "warnOnce" or not w.once[key])
  end
  function w.write(level, key, message)
    if level == "warnOnce" then w.once[key] = true end
    if #w.buffer == 200 then table.remove(w.buffer, 1) end
    w.buffer[#w.buffer + 1] = { level = level, message = message }
  end
  function w.command(...)
    local count = select("#", ...)
    w.commands = w.commands + 1
    local key = "SELU_" .. (ADDON or "Addon") .. "_" .. w.commands .. "_"
    for i = 1, count - 1 do _G["SLASH_" .. key .. i] = select(i, ...) end
    local callback = select(count, ...)
    SlashCmdList[key] = function(msg) callback(msg) end
  end
  local function parse(text)
    local parts, plain, order = {}, {}, {}
    local i, maximum = 1, -1
    while i <= #text do
      local ch, pair = text:sub(i, i), text:sub(i, i + 1)
      if pair == "{{" or pair == "}}" then
        parts[#parts + 1], plain[#plain + 1] = ch, ch
        i = i + 2
      elseif ch == "{" then
        local first, last, index = text:find("{(%d+)}", i)
        assert(first == i, "invalid locale placeholder")
        index = tonumber(index)
        maximum = math.max(maximum, index)
        order[#order + 1], parts[#parts + 1] = index + 1, "%s"
        i = last + 1
      else
        assert(ch ~= "}", "escape literal locale braces with {{ or }}")
        parts[#parts + 1], plain[#plain + 1] = ch == "%" and "%%" or ch, ch
        i = i + 1
      end
    end
    return { format = table.concat(parts), literal = table.concat(plain), order = order, maximum = maximum }
  end
  function w.text(key, fallback, ...)
    local locale = w.locales[GetLocale()]
    local text = locale and locale[key]
    local count = select("#", ...)
    if not text then
      if count == 0 then return fallback end
      return string.format(fallback, ...)
    end
    local cached = w.formats[text]
    if not cached then cached = parse(text); w.formats[text] = cached end
    assert(cached.maximum < count, "invalid locale argument")
    if count == 0 then return cached.literal end
    local values, args = {...}, {}
    for i = 1, #cached.order do args[i] = values[cached.order[i]] end
    return string.format(cached.format, unpack(args, 1, #cached.order))
  end
end
local __selu_wow = ns.wow"##;
