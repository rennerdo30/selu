local __selu_addon, __selu_ns = ...
local __selu_read = __selu_ns.__selu_read
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  local panel = CreateFrame("Frame", nil, nil, "BackdropTemplate")
  panel:SetSize(240, 100)
  panel:SetBackdrop({ bgFile = "Interface/Tooltips/UI-Tooltip-Background" })
  panel:SetBackdropColor(0.1, 0.2, 0.3, 1)
  local button = CreateFrame("Button", nil, panel, "UIPanelButtonTemplate")
  button:SetSize(120, 24)
  button:SetText("Sell junk")
  do
    local __selu_t1 = __selu_read((panel:GetWidth()))
    if __selu_t1 == nil then __selu_t1 = 0 end
    local __selu_t3 = __selu_t1
    local __selu_t2 = __selu_read((button:GetText()))
    if __selu_t2 == nil then __selu_t2 = "" end
    print(__selu_t3, __selu_t2)
  end
end)
