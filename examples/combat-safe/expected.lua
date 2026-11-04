local __selu_addon, __selu_ns = ...
__selu_ns = __selu_ns or {}
local __selu_pending = {}
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
end
__selu_ns["main"] = {}
if not __selu_ns.__selu_events then
  -- Selu: shared event frame and dispatch table
  local __selu_handlers = {}
  local __selu_frame = CreateFrame("Frame")
  __selu_frame:SetScript("OnEvent", function(self, event, ...)
    local handlers = __selu_handlers[event]
    if not handlers then return end
    for i = 1, #handlers do handlers[i](...) end
  end)
  local function __selu_on(event, fn)
    local handlers = __selu_handlers[event]
    if not handlers then
      handlers = {}
      __selu_handlers[event] = handlers
      __selu_frame:RegisterEvent(event)
    end
    handlers[#handlers + 1] = fn
  end
  __selu_ns.__selu_events = __selu_on
end
local __selu_on = __selu_ns.__selu_events
local panel = CreateFrame("Frame")
__selu_on("PLAYER_LOGIN", function()
  do
    local __selu_t1 = panel
    __selu_defer(__selu_t1.Hide, __selu_t1)
  end
  print("Panel hidden or queued")
end)
