local __selu_addon, __selu_ns = ...
local __selu_read = __selu_ns.__selu_read
local __selu_widget_call = __selu_ns.__selu_widget_call
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local panel = CreateFrame("Frame", nil, UIParent, "SecureHandlerBaseTemplate")
local label = panel:CreateFontString()
__selu_on("PLAYER_LOGIN", function()
  do
    local __selu_t1 = panel
    __selu_widget_call(__selu_t1.Hide, __selu_t1)
  end
  local name = UnitName("player")
  label:SetText(name)
  do
    local __selu_t2 = __selu_read(name)
    if __selu_t2 == nil then __selu_t2 = "Player" end
    print("Hello " .. __selu_t2)
  end
  print("Panel hidden or queued")
end)
