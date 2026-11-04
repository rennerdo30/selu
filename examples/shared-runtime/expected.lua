local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local ready = __selu_ns["worker"].ready
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  print(ready())
end)
