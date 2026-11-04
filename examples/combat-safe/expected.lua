local __selu_addon, __selu_ns = ...
local __selu_read = __selu_ns.__selu_read
local __selu_widget_call = __selu_ns.__selu_widget_call
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
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
