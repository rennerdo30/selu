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
local function readBalance()
  do
    local __selu_t1, __selu_t2, __selu_t3 = pcall(function()
      error("main.selu:3: " .. ({ message = "Balance unavailable" }).message, 0)
    end)
    if not __selu_t1 then
      __selu_t1, __selu_t2, __selu_t3 = pcall(function()
        local e = { message = tostring(__selu_t2) }
        print(e.message)
        return "return", 0
      end)
    end
    local __selu_t4, __selu_t5, __selu_t6 = pcall(function()
      print("Balance check finished")
    end)
    if not __selu_t4 or __selu_t5 ~= nil then
      __selu_t1, __selu_t2, __selu_t3 = __selu_t4, __selu_t5, __selu_t6
    end
    if not __selu_t1 then error(__selu_t2, 0) end
    if __selu_t2 == "return" then return __selu_t3 end
  end
end
__selu_on("PLAYER_LOGIN", function()
  print(readBalance())
end)
