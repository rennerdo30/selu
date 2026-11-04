local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local sampleLink = "|cffffffff|Hitem:123:0|h[Silver Sword]|h|r"
local sampleMessage = "Selling a Silver Sword"
local function describeLink(link)
  local itemId, itemName = string.match(link, "%|Hitem:(%d+).-%|h%[(.-)%]%|h")
  if itemId == nil then
    print("No item link found; leave the message unchanged.")
    return
  end
  print(string.format("Trade item: %s (ID %s)", itemName == nil and "unnamed item" or itemName, itemId))
end
__selu_on("PLAYER_LOGIN", function()
  describeLink(sampleLink)
  describeLink("Anyone need supplies?")
  local wordCount = 0
  for word in string.gmatch(sampleMessage, "%S+") do
    wordCount = wordCount + 1
  end
  print(string.format("Sample trade message: %s words", wordCount))
end)
