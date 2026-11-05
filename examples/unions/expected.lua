local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_type = type
__selu_ns["main"] = {}
local function describe(value)
  if __selu_type(value) == "string" then
    return value
  end
  return "number"
end
__selu_on("PLAYER_LOGIN", function()
  print(describe("union"), describe(7))
end)
