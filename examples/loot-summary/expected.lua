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
    for __selu_t11 = 1, #__selu_t1 do
      local item = __selu_t1[__selu_t11]
      if item == nil then break end
      local __selu_t5 = item.quality
      local __selu_t2 = countByQuality
      local __selu_t3 = item.quality
      local __selu_t4 = __selu_t2[__selu_t3]
      if __selu_t4 == nil then __selu_t4 = 0 end
      local __selu_t6 = countByQuality
      local __selu_t7 = __selu_t5
      local __selu_t8 = __selu_t4 + item.count
      __selu_t6[__selu_t7] = __selu_t8
      do
        local __selu_t9 = value
        local __selu_t10 = item.count * item.price
        value = __selu_t9 + __selu_t10
      end
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
        if count > 0 then
          print(string.format("  %s: %s", qualityNames[quality + 1], count))
        end
      end
      quality = quality + 1
    end
  end
  local __selu_t12 = {}
  for __selu_t13 = 1, #loot do
    local __selu_t14 = loot[__selu_t13]
    if __selu_t14 == nil then break end
    local item = __selu_t14
    if item.quality >= 2 then __selu_t12[#__selu_t12 + 1] = __selu_t14 end
  end
  local __selu_t15 = {}
  for __selu_t16 = 1, #__selu_t12 do
    local __selu_t17 = __selu_t12[__selu_t16]
    if __selu_t17 == nil then break end
    local item = __selu_t17
    __selu_t15[#__selu_t15 + 1] = item.name
  end
  local finds = __selu_t15
  print(string.format("Notable: %s", table.concat(finds, ", ")))
  print(string.format("Vendor value: %s", formatMoney(value)))
end)
