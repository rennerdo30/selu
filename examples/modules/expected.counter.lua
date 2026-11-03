local __selu_addon, __selu_ns = ...
__selu_ns = __selu_ns or {}
__selu_ns["counter"] = {}
local count = 0
__selu_ns["counter"].count = function()
  return count
end
local greeting = "Hello from modules!"
__selu_ns["counter"].greeting = greeting
local function increment()
  count = count + 1
end
__selu_ns["counter"].increment = increment
