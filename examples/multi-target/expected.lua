local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  if GetTime ~= nil then
    print("Client clock available")
  else
    print("Client clock unavailable")
  end
end)
