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
local function rewardPoints(value)
  if __selu_type(value) == "number" then
    return value
  end
  return 0
end
local route = {
  { label = "Supply delivery", points = rewardPoints(7) },
  { label = "Unconfirmed reward", points = rewardPoints("pending") },
}
__selu_on("PLAYER_LOGIN", function()
  local totalPoints = 0
  do
    local __selu_t1 = route
    for __selu_t4 = 1, #__selu_t1 do
      local quest = __selu_t1[__selu_t4]
      if quest == nil then break end
      print(string.format("%s: %s", quest.label, quest.points))
      do
        local __selu_t2 = totalPoints
        local __selu_t3 = quest.points
        totalPoints = __selu_t2 + __selu_t3
      end
    end
  end
  print(string.format("Route reward: %s points", totalPoints))
end)
