local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local count = __selu_ns["counter"].count
local increment = __selu_ns["counter"].increment
local greeting = __selu_ns["counter"].greeting
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  print(greeting)
  increment()
  print(count())
end)
