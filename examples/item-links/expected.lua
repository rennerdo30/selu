local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  local link = "|cffffffff|Hitem:123:0|h[Silver Sword]|h|r"
  local itemId, itemName = string.match(link, "%|Hitem:(%d+).-%|h%[(.-)%]%|h")
  print(itemId == nil and "missing" or itemId, itemName == nil and "missing" or itemName)
  local words = 0
  for word in string.gmatch("Selling a Silver Sword", "%S+") do
    words = words + 1
  end
  print(words)
end)
