local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local function greeting(playerName)
  if playerName == nil then playerName = "adventurer" end
  return string.format("Welcome, %s!", playerName)
end
__selu_on("PLAYER_LOGIN", function()
  print(greeting())
  print("Before your next quest: check your bags and repair your gear.")
end)
