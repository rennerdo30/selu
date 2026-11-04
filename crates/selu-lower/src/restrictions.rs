//! Restriction runtime emission, self-contained for addon-runtime relocation.
use super::*;
use selu_check::{Type, restrictions::secret_inner};

fn semantics(types: &TypeTable, callee: &Expr) -> Option<selu_check::types::ApiSemantics> {
    match types.get(callee.id) {
        Some(Type::Function { api_semantics, .. }) => Some(api_semantics.clone()),
        _ => None,
    }
}
pub(super) fn emit_restriction_runtime(
    program: &Program,
    types: &TypeTable,
    manifest: &Manifest,
) -> LBlock {
    fn walk(c: visit::Child<'_>, types: &TypeTable, queue: &mut bool, read: &mut bool) {
        match c {
            visit::Child::Stmt(s) => {
                visit::child_stmts_and_exprs(s, &mut |c| walk(c, types, queue, read))
            }
            visit::Child::Expr(e) => {
                match &e.kind {
                    ExprKind::Call { callee, args } => {
                        if let Some(s) = semantics(types, callee) {
                            *queue |= s.protected || s.dynamic_protected;
                        }
                        if matches!(&callee.kind,ExprKind::Ident(n) if n=="readable")
                            && types.get(callee.id).is_none()
                        {
                            *read |= args
                                .first()
                                .and_then(|a| types.get(a.id))
                                .is_some_and(|t| secret_inner(t).is_some());
                        }
                    }
                    ExprKind::Binary { op, left, .. } if op == "??" => {
                        *read |= types
                            .get(left.id)
                            .is_some_and(|t| secret_inner(t).is_some())
                    }
                    _ => {}
                }
                visit::child_exprs_and_stmts(e, &mut |c| walk(c, types, queue, read));
            }
        }
    }
    let (mut queue, mut read) = (false, false);
    for s in &program.stmts {
        walk(visit::Child::Stmt(s), types, &mut queue, &mut read);
    }
    let mut out = Vec::new();
    if read
        && manifest
            .function("canaccessvalue")
            .is_some_and(|f| f.available)
    {
        out.push(raw(READ_RUNTIME));
    }
    if queue {
        let runtime = if manifest
            .function("canaccessvalue")
            .is_some_and(|f| f.available)
        {
            DEFER_RUNTIME.to_owned()
        } else {
            DEFER_RUNTIME.replace(
                "(canaccessvalue and not canaccessvalue(protected)) or protected",
                "protected",
            )
        };
        out.push(raw(&runtime));
        out.push(raw("local __selu_defer = __selu_restrictions.defer\nlocal __selu_invoke = __selu_restrictions.defer\nlocal __selu_widget_call = __selu_restrictions.widget"));
    }
    out
}
/// The same blocks serve standalone lowering and the addon registry. Keep the
/// queue algorithm here unchanged; sharing only moves its initialization.
pub(crate) fn runtime_pieces(
    program: &Program,
    types: &TypeTable,
    manifest: &Manifest,
) -> Vec<crate::runtime::Piece> {
    let mut blocks = emit_restriction_runtime(program, types, manifest);
    let mut pieces = Vec::new();
    if blocks
        .first()
        .is_some_and(|s| matches!(&s.kind, LStmtKind::Raw(text) if text == READ_RUNTIME))
    {
        pieces.push(crate::runtime::piece(
            "60-secret-read",
            vec![blocks.remove(0)],
            &["__selu_read"],
            &[],
        ));
    }
    if !blocks.is_empty() {
        // Preserve standalone ordering: drain frame before the user dispatcher.
        pieces.push(crate::runtime::piece(
            "18-protected-calls",
            blocks,
            &["__selu_defer", "__selu_invoke", "__selu_widget_call"],
            &[],
        ));
    }
    pieces
}
const READ_RUNTIME: &str = r#"local function __selu_read(value)
  if canaccessvalue and not canaccessvalue(value) then return nil end
  return value
end"#;
const DEFER_RUNTIME: &str = r##"local _, __selu_restriction_ns = ...
__selu_restriction_ns = __selu_restriction_ns or {}
if not __selu_restriction_ns.__selu_restrictions then
  local pending = {}
  local frame = CreateFrame("Frame")
  frame:RegisterEvent("PLAYER_REGEN_ENABLED")
  frame:SetScript("OnEvent", function()
    while #pending > 0 and not InCombatLockdown() do
      local item = table.remove(pending, 1)
      local ok, err = pcall(item.fn, unpack(item, 1, item.n))
      if not ok then pcall(geterrorhandler(), err) end
    end
  end)
  local function defer(fn, ...)
    if not InCombatLockdown() then return fn(...) end
    if false then print("Selu: deferred protected call") end
    local item = { ... }
    item.fn, item.n = fn, select("#", ...)
    pending[#pending + 1] = item
  end
  local function widget(fn, receiver, ...)
    if InCombatLockdown() then
      local protected = receiver:IsProtected()
      if (canaccessvalue and not canaccessvalue(protected)) or protected then return defer(fn, receiver, ...) end
    end
    return fn(receiver, ...)
  end
  __selu_restriction_ns.__selu_restrictions = { defer = defer, widget = widget }
end
local __selu_restrictions = __selu_restriction_ns.__selu_restrictions"##;
impl Lowerer<'_> {
    pub(super) fn read_if_secret(&self, expr: &Expr, value: LExpr) -> LExpr {
        if self
            .target
            .universe
            .function("canaccessvalue")
            .is_some_and(|f| f.available)
            && self
                .types
                .get(expr.id)
                .is_some_and(|t| secret_inner(t).is_some())
        {
            lua::call(name("__selu_read"), vec![value])
        } else {
            value
        }
    }

    pub(super) fn restricted_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> LExpr {
        if matches!(&callee.kind,ExprKind::Ident(n) if n=="readable")
            && self.types.get(callee.id).is_none()
        {
            let value = self.expr(&args[0], pre);
            return self.read_if_secret(&args[0], value);
        }
        let s = semantics(self.types, callee);
        let result = self.call(callee, args, pre);
        let Some(s) = s else {
            return result;
        };
        if !(s.protected || s.dynamic_protected) {
            return result;
        }
        match result {
            LExpr::Call(f, mut args) => {
                args.insert(0, *f);
                lua::call(
                    name(if s.dynamic_protected {
                        "__selu_invoke"
                    } else {
                        "__selu_defer"
                    }),
                    args,
                )
            }
            LExpr::Method(obj, method, mut args) => {
                let held = self.hold(*obj, &callee.span, pre);
                let obj = name(&held);
                args.insert(0, obj.clone());
                args.insert(0, LExpr::Field(Box::new(obj), method));
                lua::call(name("__selu_widget_call"), args)
            }
            other => other,
        }
    }
}
