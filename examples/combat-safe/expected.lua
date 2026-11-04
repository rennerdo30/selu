local __selu_addon, __selu_ns = ...
__selu_ns = __selu_ns or {}
local function __selu_read(value)
  if canaccessvalue and not canaccessvalue(value) then return nil end
  return value
end
local _, __selu_restriction_ns = ...
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
local __selu_restrictions = __selu_restriction_ns.__selu_restrictions
local __selu_defer = __selu_restrictions.defer
local __selu_invoke = __selu_restrictions.defer
local __selu_widget_call = __selu_restrictions.widget
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
local panel = CreateFrame("Frame", nil, UIParent, "SecureHandlerBaseTemplate")
do
  local __selu_t1 = panel
  __selu_widget_call(__selu_t1.SetSize, __selu_t1, 240, 60)
end
do
  local __selu_t2 = panel
  __selu_widget_call(__selu_t2.SetPoint, __selu_t2, "CENTER", UIParent, "CENTER", 0, 100)
end
local label = panel:CreateFontString(nil, "OVERLAY", "GameFontNormal")
do
  local __selu_t3 = label
  __selu_widget_call(__selu_t3.SetPoint, __selu_t3, "CENTER", panel, "CENTER", 0, 0)
end
__selu_on("PLAYER_LOGIN", function()
  local playerName = __selu_read((UnitName("player")))
  if playerName == nil then playerName = "adventurer" end
  label:SetText(string.format("Safe travels, %s", playerName))
  do
    local __selu_t4 = panel
    __selu_widget_call(__selu_t4.Hide, __selu_t4)
  end
  print(string.format("Travel reminder prepared for %s", playerName))
  print("Protected panel hide requested; combat may defer it.")
end)
