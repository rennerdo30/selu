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
local loot = {
  { name = "Linen Cloth", quality = 1, count = 12, price = 13 },
  { name = "Broken Fang", quality = 0, count = 3, price = 5 },
  { name = "Silver Ore", quality = 1, count = 4, price = 120 },
  { name = "Ring of the Fox", quality = 2, count = 1, price = 2150 },
  { name = "Fiery War Axe", quality = 3, count = 1, price = 38455 },
  { name = "Wool Cloth", quality = 1, count = 7, price = 33 },
}
local qualityNames = { "poor", "common", "uncommon", "rare" }
local function formatMoney(copper)
  local gold = math.floor(copper / 10000)
  local silver = math.floor(copper / 100) % 100
  local rest = copper % 100
  return string.format("%sg %ss %sc", gold, silver, rest)
end
__selu_on("PLAYER_LOGIN", function()
  local countByQuality = {}
  local value = 0
  do
    local __selu_t1 = loot
    for __selu_t3 = 1, #__selu_t1 do
      local item = __selu_t1[__selu_t3]
      if item == nil then break end
      local __selu_t2 = countByQuality[item.quality]
      if __selu_t2 == nil then __selu_t2 = 0 end
      countByQuality[item.quality] = __selu_t2 + item.count
      value = value + item.count * item.price
    end
  end
  print(string.format("Loot this session: %s kinds of items", #loot))
  do
    local quality = 0
    while true do
      if not (quality < #qualityNames) then break end
      do
        local count = countByQuality[quality]
        if count == nil then count = 0 end
        if count > 0 then print(string.format("  %s: %s", qualityNames[quality + 1], count)) end
      end
      quality = quality + 1
    end
  end
  local __selu_t4 = {}
  for __selu_t5 = 1, #loot do
    local __selu_t6 = loot[__selu_t5]
    if __selu_t6 == nil then break end
    local item = __selu_t6
    if item.quality >= 2 then __selu_t4[#__selu_t4 + 1] = __selu_t6 end
  end
  local __selu_t7 = {}
  for __selu_t8 = 1, #__selu_t4 do
    local __selu_t9 = __selu_t4[__selu_t8]
    if __selu_t9 == nil then break end
    local item = __selu_t9
    __selu_t7[#__selu_t7 + 1] = item.name
  end
  local finds = __selu_t7
  print(string.format("Notable: %s", table.concat(finds, ", ")))
  print(string.format("Vendor value: %s", formatMoney(value)))
end)
