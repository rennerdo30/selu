//! Self-contained runtime emission, relocatable by the addon runtime pass.
use super::*;
use selu_check::Type;

fn policy(types: &TypeTable, e: &Expr) -> Option<(bool, Vec<bool>)> {
    match types.get(e.id) {
        Some(Type::Function {
            api_semantics: s, ..
        }) if s.protected || s.secret_results.iter().any(|x| *x) => {
            Some((s.protected, s.secret_results.clone()))
        }
        _ => None,
    }
}
fn guard_name(mask: &[bool]) -> String {
    format!(
        "__selu_read_{}",
        mask.iter()
            .map(|x| if *x { 's' } else { 'v' })
            .collect::<String>()
    )
}
pub(super) fn emit_restriction_runtime(
    program: &Program,
    types: &TypeTable,
    manifest: &Manifest,
) -> LBlock {
    fn walk(
        c: visit::Child<'_>,
        types: &TypeTable,
        protected: &mut bool,
        masks: &mut std::collections::BTreeSet<Vec<bool>>,
    ) {
        let mut next = |c| walk(c, types, protected, masks);
        match c {
            visit::Child::Stmt(s) => visit::child_stmts_and_exprs(s, &mut next),
            visit::Child::Expr(e) => {
                if let ExprKind::Call { callee, .. } = &e.kind
                    && let Some((p, mask)) = policy(types, callee)
                {
                    *protected |= p;
                    if mask.iter().any(|x| *x) {
                        masks.insert(mask);
                    }
                }
                visit::child_exprs_and_stmts(e, &mut |c| walk(c, types, protected, masks));
            }
        }
    }
    let mut protected = false;
    let mut masks = Default::default();
    for s in &program.stmts {
        walk(visit::Child::Stmt(s), types, &mut protected, &mut masks);
    }
    let mut out = Vec::new();
    if !manifest
        .function("canaccessvalue")
        .is_some_and(|f| f.available)
    {
        masks.clear();
    }
    for mask in masks {
        let names: Vec<_> = (0..mask.len()).map(|i| format!("v{i}")).collect();
        let mut code = format!(
            "local function {}({})\n",
            guard_name(&mask),
            names.join(", ")
        );
        for (i, secret) in mask.iter().enumerate() {
            if *secret {
                code += &format!(
                    "  if canaccessvalue and not canaccessvalue(v{i}) then v{i} = nil end\n"
                );
            }
        }
        code += &format!("  return {}\nend", names.join(", "));
        out.push(raw(&code));
    }
    if protected {
        out.push(raw(DEFER_RUNTIME));
    }
    out
}
const DEFER_RUNTIME: &str = r##"local __selu_pending = {}
local __selu_defer_frame = CreateFrame("Frame")
__selu_defer_frame:RegisterEvent("PLAYER_REGEN_ENABLED")
__selu_defer_frame:SetScript("OnEvent", function()
  while #__selu_pending > 0 and not InCombatLockdown() do
    local item = table.remove(__selu_pending, 1)
    item.fn(unpack(item, 1, item.n))
  end
end)
local function __selu_defer(fn, ...)
  if not InCombatLockdown() then return fn(...) end
  if false then print("Selu: deferred protected call") end
  local item = { ... }
  item.fn, item.n = fn, select("#", ...)
  __selu_pending[#__selu_pending + 1] = item
end"##;
impl Lowerer<'_> {
    pub(super) fn restricted_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> LExpr {
        let Some((protected, mask)) = policy(self.types, callee) else {
            return self.call(callee, args, pre);
        };
        let mut result = self.call(callee, args, pre);
        if protected {
            result = match result {
                LExpr::Call(f, mut args) => {
                    args.insert(0, *f);
                    lua::call(name("__selu_defer"), args)
                }
                LExpr::Method(obj, method, mut args) => {
                    let held = self.hold(*obj, &callee.span, pre);
                    let obj = name(&held);
                    args.insert(0, obj.clone());
                    args.insert(0, LExpr::Field(Box::new(obj), method));
                    lua::call(name("__selu_defer"), args)
                }
                other => other,
            };
        }
        if mask.iter().any(|x| *x)
            && self
                .target
                .universe
                .function("canaccessvalue")
                .is_some_and(|f| f.available)
        {
            lua::call(name(&guard_name(&mask)), vec![result])
        } else {
            result
        }
    }
}
