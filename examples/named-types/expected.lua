local __selu_addon, __selu_ns = ...
local __selu_type = type
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
local function score(value)
  if __selu_type(value) == "number" then
    return value
  end
  return 0
end
__selu_on("PLAYER_LOGIN", function()
  local entry = { label = "Quest", points = score(7) }
  local scores = {}
  local __selu_t1 = scores
  local __selu_t2 = entry.points
  __selu_t1[#__selu_t1 + 1] = __selu_t2
  print(entry.label, scores[1])
end)
