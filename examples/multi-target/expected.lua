local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local function reportSessionStart()
  if GetTime ~= nil then
    print(string.format("Session clock: %s seconds since client startup", (GetTime())))
  else
    print("Session started; this client has no supported clock API.")
  end
end
__selu_on("PLAYER_LOGIN", function()
  print("Adventure journal ready on this client.")
  reportSessionStart()
end)
