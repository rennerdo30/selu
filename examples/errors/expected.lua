local __selu_addon, __selu_ns = ...
local __selu_pcall, __selu_error, __selu_tostring = pcall, error, tostring
local __selu_error_setmetatable, __selu_error_getmetatable = setmetatable, getmetatable
__selu_ns = __selu_ns or {}
if not __selu_ns.__selu_errors then
  __selu_ns.__selu_errors = { depth = 0, meta = {} }
  __selu_ns.__selu_errors.meta.__tostring = function(value)
    return (value.__selu_position or "") .. value.message
  end
end
local __selu_errors = __selu_ns.__selu_errors
local function __selu_error_text(value)
  return __selu_tostring(value) .. ""
end
local function __selu_safe_error_text(value)
  local ok, message = __selu_pcall(__selu_error_text, value)
  if ok then return message end
  return "<error object>"
end
local function __selu_new_error(message, value)
  return __selu_error_setmetatable({ message = message, value = value }, __selu_errors.meta)
end
local function __selu_is_error(value)
  if __selu_error_getmetatable(value) == __selu_errors.meta then return true end
  return false
end
local function __selu_wrap_error(value)
  local ok, isError = __selu_pcall(__selu_is_error, value)
  if ok and isError then return value end
  return __selu_new_error(__selu_safe_error_text(value), value)
end
local function __selu_raise(value)
  if __selu_errors.depth == 0 then
    __selu_error(__selu_safe_error_text(value), 0)
  else
    __selu_error(value, 0)
  end
end
local function __selu_throw(value, position)
  if value.__selu_position == nil then value.__selu_position = position end
  __selu_raise(value)
end
local function __selu_protect(fn, ...)
  __selu_errors.depth = __selu_errors.depth + 1
  local ok, status, value1 = __selu_pcall(fn, ...)
  __selu_errors.depth = __selu_errors.depth - 1
  return ok, status, value1
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
local function readBalance()
  local failure = __selu_new_error("Balance unavailable")
  do
    local __selu_t1, __selu_t2, __selu_t3 = __selu_protect(function()
      __selu_throw(failure, "main.selu:4: ")
    end)
    if not __selu_t1 then
      __selu_t1, __selu_t2, __selu_t3 = __selu_protect(function()
        local e = __selu_wrap_error(__selu_t2)
        print(e.message)
        return "return", 0
      end)
    end
    local __selu_t4, __selu_t5, __selu_t6 = __selu_protect(function()
      print("Balance check finished")
    end)
    if not __selu_t4 or __selu_t5 ~= nil then
      __selu_t1, __selu_t2, __selu_t3 = __selu_t4, __selu_t5, __selu_t6
    end
    if not __selu_t1 then __selu_raise(__selu_t2) end
    if __selu_t2 == "return" then return __selu_t3 end
  end
end
__selu_on("PLAYER_LOGIN", function()
  print(readBalance())
end)
