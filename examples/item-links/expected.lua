local __selu_addon, __selu_ns = ...
__selu_ns = __selu_ns or {}
__selu_ns["main"] = {}
if not __selu_ns.__selu_events then
  -- Selu: shared event frame and dispatch table
  local __selu_handlers = {}
  local __selu_frame = CreateFrame("Frame")
  __selu_frame:SetScript("OnEvent", function(self, event, ...)
    local handlers = __selu_handlers[event]
    if not handlers then return end
    for i = 1, #handlers do handlers[i](...) end
  end)
  local function __selu_on(event, fn)
    local handlers = __selu_handlers[event]
    if not handlers then
      handlers = {}
      __selu_handlers[event] = handlers
      __selu_frame:RegisterEvent(event)
    end
    handlers[#handlers + 1] = fn
  end
  __selu_ns.__selu_events = __selu_on
end
local __selu_on = __selu_ns.__selu_events
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
