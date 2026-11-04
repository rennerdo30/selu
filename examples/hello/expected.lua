local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local function greeting()
  return "Hello from Selu!"
end
__selu_on("PLAYER_LOGIN", function()
  print(greeting())
end)
