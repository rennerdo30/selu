local __selu_addon, __selu_ns = ...
-- Selu: errors-state
do
  if not __selu_ns.__selu_errors then
    __selu_ns.__selu_errors = { depth = 0, meta = {} }
    __selu_ns.__selu_errors.meta.__tostring = function(value)
      return (value.__selu_position or "") .. value.message
    end
  end
  local __selu_errors = __selu_ns.__selu_errors
  __selu_ns.__selu_errors = __selu_errors
end
-- Selu: errors-captures
do
  local __selu_pcall, __selu_error, __selu_tostring = pcall, error, tostring
  local __selu_error_setmetatable, __selu_error_getmetatable = setmetatable, getmetatable
  __selu_ns.__selu_pcall = __selu_pcall
  __selu_ns.__selu_error = __selu_error
  __selu_ns.__selu_tostring = __selu_tostring
  __selu_ns.__selu_error_setmetatable = __selu_error_setmetatable
  __selu_ns.__selu_error_getmetatable = __selu_error_getmetatable
end
-- Selu: errors
do
  local __selu_errors = __selu_ns.__selu_errors
  local __selu_pcall = __selu_ns.__selu_pcall
  local __selu_error = __selu_ns.__selu_error
  local __selu_tostring = __selu_ns.__selu_tostring
  local __selu_error_setmetatable = __selu_ns.__selu_error_setmetatable
  local __selu_error_getmetatable = __selu_ns.__selu_error_getmetatable
  local __selu_trace = nil
  local function __selu_stack()
    if not __selu_trace then return nil end
    local ok, value = __selu_pcall(__selu_trace)
    if not ok then return nil end
    if issecretvalue then
      local checked, secret = __selu_pcall(issecretvalue, value)
      if not checked or secret then return nil end
    end
    if canaccessvalue then
      local checked, accessible = __selu_pcall(canaccessvalue, value)
      if not checked or not accessible then return nil end
    end
    if type(value) == "string" then return value end
  end
  local function __selu_error_text(value)
    return __selu_tostring(value) .. ""
  end
  local function __selu_safe_error_text(value)
    local ok, message = __selu_pcall(__selu_error_text, value)
    if ok then return message end
    return "<error object>"
  end
  local function __selu_new_error(message, position, value)
    return __selu_error_setmetatable({ message = message, value = value, __selu_position = position }, __selu_errors.meta)
  end
  local function __selu_is_error(value)
    if __selu_error_getmetatable(value) == __selu_errors.meta then return true end
    return false
  end
  local function __selu_wrap_error(value)
    local ok, isError = __selu_pcall(__selu_is_error, value)
    if ok and isError then
      if value.stack == nil then value.stack = __selu_stack() end
      return value
    end
    local wrapped = __selu_new_error(__selu_safe_error_text(value), nil, value)
    wrapped.stack = __selu_stack()
    return wrapped
  end
  local function __selu_raise(value)
    if __selu_errors.depth == 0 then
      __selu_error(__selu_safe_error_text(value), 0)
    else
      __selu_error(value, 0)
    end
  end
  __selu_ns.__selu_new_error = __selu_new_error
  __selu_ns.__selu_wrap_error = __selu_wrap_error
  __selu_ns.__selu_raise = __selu_raise
  __selu_ns.__selu_stack = __selu_stack
end
-- Selu: protect-0002
do
  local __selu_errors = __selu_ns.__selu_errors
  local __selu_pcall = __selu_ns.__selu_pcall
  local function __selu_protect(fn, ...)
    local depth = __selu_errors.depth
    __selu_errors.depth = depth + 1
    local ok, status, value1 = __selu_pcall(fn, ...)
    __selu_errors.depth = depth
    return ok, status, value1
  end
  __selu_ns.__selu_protect_2 = __selu_protect
end
-- Selu: events
do
  local __selu_errors = __selu_ns.__selu_errors
  -- Selu: shared event frame and dispatch table
  local __selu_handlers = {}
  local __selu_frame = CreateFrame("Frame")
  __selu_frame:SetScript("OnEvent", function(self, event, ...)
    local handlers = __selu_handlers[event]
    if not handlers then return end
    local depth = __selu_errors.depth
    __selu_errors.depth = 0
    for i = 1, #handlers do handlers[i](...) end
    __selu_errors.depth = depth
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
