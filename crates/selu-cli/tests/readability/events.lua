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
__selu_on("PLAYER_LOGIN", function()
  print(1)
end)
__selu_on("PLAYER_LOGIN", function()
  print(2)
end)
