local __selu_addon, __selu_ns = ...
__selu_ns = __selu_ns or {}
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
__selu_on("PLAYER_LOGIN", function()
  local panel = CreateFrame("Frame", nil, nil, "BackdropTemplate")
  panel:SetSize(240, 100)
  panel:SetBackdrop({ bgFile = "Interface/Tooltips/UI-Tooltip-Background" })
  panel:SetBackdropColor(0.1, 0.2, 0.3, 1)
  local button = CreateFrame("Button", nil, panel, "UIPanelButtonTemplate")
  button:SetSize(120, 24)
  button:SetText("Sell junk")
  print(panel:GetWidth(), (button:GetText()))
end)
