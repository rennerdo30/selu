local __selu_addon, __selu_ns = ...
local __selu_saved__G = _G
local __selu_saved_type = type
local __selu_saved_pairs = pairs
local __selu_saved_ipairs = ipairs
local __selu_saved_CreateFrame = CreateFrame
local __selu_saved_rawget = rawget
local __selu_saved_pcall = pcall
local __selu_saved_print = print
local __selu_saved_tostring = tostring
__selu_ns = __selu_ns or {}
if not __selu_ns.__selu_wow then
  local w = { buffer = {}, levels = { info = true, warn = true, error = true, warnOnce = true }, once = {}, commands = 0, locales = {}, formats = {} }
  __selu_ns.__selu_wow = w
  function w.enabled(level, key)
    return w.levels[level] and (level ~= "warnOnce" or not w.once[key])
  end
  function w.write(level, key, message)
    if level == "warnOnce" then w.once[key] = true end
    if #w.buffer == 200 then table.remove(w.buffer, 1) end
    w.buffer[#w.buffer + 1] = { level = level, message = message }
  end
  function w.command(...)
    local count = select("#", ...)
    w.commands = w.commands + 1
    local key = "SELU_" .. (__selu_addon or "Addon") .. "_" .. w.commands .. "_"
    for i = 1, count - 1 do _G["SLASH_" .. key .. i] = select(i, ...) end
    local callback = select(count, ...)
    SlashCmdList[key] = function(msg) callback(msg) end
  end
  local function parse(text)
    local parts, plain, order = {}, {}, {}
    local i, maximum = 1, -1
    while i <= #text do
      local ch, pair = text:sub(i, i), text:sub(i, i + 1)
      if pair == "{{" or pair == "}}" then
        parts[#parts + 1], plain[#plain + 1] = ch, ch
        i = i + 2
      elseif ch == "{" then
        local first, last, index = text:find("{(%d+)}", i)
        assert(first == i, "invalid locale placeholder")
        index = tonumber(index)
        maximum = math.max(maximum, index)
        order[#order + 1], parts[#parts + 1] = index + 1, "%s"
        i = last + 1
      else
        assert(ch ~= "}", "escape literal locale braces with {{ or }}")
        parts[#parts + 1], plain[#plain + 1] = ch == "%" and "%%" or ch, ch
        i = i + 1
      end
    end
    return { format = table.concat(parts), literal = table.concat(plain), order = order, maximum = maximum }
  end
  function w.text(key, fallback, ...)
    local locale = w.locales[GetLocale()]
    local text = locale and locale[key]
    local count = select("#", ...)
    if not text then
      if count == 0 then return fallback end
      return string.format(fallback, ...)
    end
    local cached = w.formats[text]
    if not cached then cached = parse(text); w.formats[text] = cached end
    assert(cached.maximum < count, "invalid locale argument")
    if count == 0 then return cached.literal end
    local values, args = {...}, {}
    for i = 1, #cached.order do args[i] = values[cached.order[i]] end
    return string.format(cached.format, unpack(args, 1, #cached.order))
  end
end
local __selu_wow = __selu_ns.__selu_wow
local __selu_saved_runtime = __selu_ns.__selu_saved
if __selu_saved_runtime == nil then
  __selu_saved_runtime = {}
  __selu_ns.__selu_saved = __selu_saved_runtime
  __selu_saved_runtime.copy = function(__selu_saved_value)
    if __selu_saved_type(__selu_saved_value) ~= "table" then return __selu_saved_value end
    local __selu_saved_result = {}
    for __selu_saved_key, __selu_saved_value in __selu_saved_pairs(__selu_saved_value) do
      __selu_saved_result[__selu_saved_key] = __selu_saved_runtime.copy(__selu_saved_value)
    end
    return __selu_saved_result
  end
  __selu_saved_runtime.fill = function(__selu_saved_data, __selu_saved_defaults, __selu_saved_objects)
    for __selu_saved_key, __selu_saved_value in __selu_saved_pairs(__selu_saved_defaults) do
      if __selu_saved_data[__selu_saved_key] == nil then
        __selu_saved_data[__selu_saved_key] = __selu_saved_runtime.copy(__selu_saved_value)
      elseif __selu_saved_objects[__selu_saved_key] ~= nil and __selu_saved_type(__selu_saved_data[__selu_saved_key]) == "table" then
        __selu_saved_runtime.fill(__selu_saved_data[__selu_saved_key], __selu_saved_value, __selu_saved_objects[__selu_saved_key])
      end
    end
  end
  __selu_saved_runtime.warn = function(__selu_saved_message)
    local __selu_saved_wow = __selu_ns.__selu_wow
    if __selu_saved_wow ~= nil then
      if __selu_saved_wow.enabled("warn", __selu_saved_message) then
        __selu_saved_wow.write("warn", __selu_saved_message, __selu_saved_message)
      end
    else
      __selu_saved_print(__selu_saved_message)
    end
  end
  __selu_saved_runtime.load = function(__selu_saved_key, __selu_saved_current, __selu_saved_defaults, __selu_saved_objects, __selu_saved_migrations)
    local __selu_saved_data = __selu_saved_rawget(__selu_saved__G, __selu_saved_key)
    if __selu_saved_data == nil then
      __selu_saved_data = {}
      __selu_saved_data.__selu_version = __selu_saved_current
    end
    if __selu_saved_type(__selu_saved_data) ~= "table" then
      __selu_saved_runtime.warn(__selu_saved_key .. ": saved root is not a table")
      return __selu_saved_data
    end
    local __selu_saved_version = __selu_saved_rawget(__selu_saved_data, "__selu_version")
    if __selu_saved_version == nil then __selu_saved_version = 1 end
    if __selu_saved_type(__selu_saved_version) ~= "number" or (__selu_saved_version < 1 or __selu_saved_version % 1 ~= 0) then
      __selu_saved_runtime.warn(__selu_saved_key .. ": invalid saved version")
      return __selu_saved_data
    end
    if __selu_saved_version > __selu_saved_current then
      __selu_saved_runtime.warn(__selu_saved_key .. ": saved data is newer than this addon")
      return __selu_saved_data
    end
    while __selu_saved_version < __selu_saved_current do
      local __selu_saved_migration = __selu_saved_migrations[__selu_saved_version]
      if __selu_saved_migration == nil then
        __selu_saved_runtime.warn(__selu_saved_key .. ": missing saved migration")
        return __selu_saved_data
      end
      local __selu_saved_candidate = __selu_saved_runtime.copy(__selu_saved_data)
      local __selu_saved_ok, __selu_saved_message = __selu_saved_pcall(__selu_saved_migration, __selu_saved_candidate)
      if not __selu_saved_ok then
        __selu_saved_runtime.warn(__selu_saved_key .. ": migration failed: " .. __selu_saved_tostring(__selu_saved_message))
        return __selu_saved_data
      end
      __selu_saved_version = __selu_saved_version + 1
      __selu_saved_data = __selu_saved_candidate
      __selu_saved_data.__selu_version = __selu_saved_version
      __selu_saved__G[__selu_saved_key] = __selu_saved_data
    end
    __selu_saved_runtime.fill(__selu_saved_data, __selu_saved_defaults, __selu_saved_objects)
    __selu_saved_data.__selu_version = __selu_saved_current
    __selu_saved__G[__selu_saved_key] = __selu_saved_data
    return __selu_saved_data
  end
  __selu_saved_runtime.entries = {}
  __selu_saved_runtime.loaders = {}
  local __selu_saved_frame = __selu_saved_CreateFrame("Frame")
  __selu_saved_frame:RegisterEvent("ADDON_LOADED")
  __selu_saved_frame:SetScript("OnEvent", function(__selu_saved_self, __selu_saved_event, __selu_saved_addon)
    if __selu_saved_addon ~= __selu_addon then return end
    for __selu_saved_key, __selu_saved_value in __selu_saved_ipairs(__selu_saved_runtime.loaders) do
      local __selu_saved_ok, __selu_saved_message = __selu_saved_pcall(__selu_saved_value)
      if not __selu_saved_ok then
        __selu_saved_runtime.warn(__selu_saved_tostring(__selu_saved_message))
      end
    end
    __selu_saved_runtime.entries = nil
    __selu_saved_runtime.loaders = nil
    __selu_saved_self:UnregisterEvent("ADDON_LOADED")
    __selu_saved_self:SetScript("OnEvent", nil)
    __selu_ns.__selu_saved = nil
  end)
end
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
local settings
__selu_saved_runtime.entries["JunkSellProof_settings"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  settings = __selu_saved_runtime.load("JunkSellProof_settings", 1, {
    enabled = true,
    maxQuality = 0,
    maxValue = 0,
    announce = true,
    autoRepair = true,
    useGuildRepair = false,
    locks = {},
  }, {}, __selu_saved_runtime.entries["JunkSellProof_settings"])
end
local open = false
local generation = 0
local baseline = {}
local function locked(id)
  do
    local __selu_t1 = settings.locks
    local __selu_t2 = __selu_t1[id]
    if __selu_t2 == nil then __selu_t2 = false end
    return __selu_t2
  end
end
local function itemID(link)
  local id = C_Item.GetItemInfoInstant(link)
  return id == nil and 0 or id
end
local function saySold(count, copper)
  if not settings.announce then return end
  if GetLocale() == "deDE" then
    print(string.format("%s Schrottgegenstand/-gegenstände für %sc verkauft.", count, copper))
  else
    print(string.format("Sold %s junk item(s) for %sc.", count, copper))
  end
end
local function repairUnchecked()
  if not settings.autoRepair then return end
  if CanMerchantRepair ~= nil and RepairAllItems ~= nil then
    if not CanMerchantRepair() then return end
    local cost = GetRepairAllCost()
    if cost <= 0 then return end
    local guild = false
    if CanGuildBankRepair ~= nil then guild = settings.useGuildRepair and CanGuildBankRepair() end
    RepairAllItems(guild)
    if settings.announce then
      if GetLocale() == "deDE" then
        local payer = guild and "Gildenmitteln" or "eigenem Gold"
        print(string.format("Für %sc repariert, bezahlt mit %s.", cost, payer))
      else
        local payer = guild and "guild funds" or "your own gold"
        print(string.format("Repaired for %sc using %s.", cost, payer))
      end
    end
  end
end
local function eligibleUnchecked(bag, slot)
  local info = C_Container.GetContainerItemInfo(bag, slot)
  if info == nil then return 0 end
  if info.hyperlink == nil then return 0 end
  local quest = C_Container.GetContainerItemQuestInfo(bag, slot)
  if quest ~= nil then
    do
      local __selu_t4 = quest.isQuestItem or quest.isActive
      if not __selu_t4 then
        local __selu_t3 = quest.questID
        if __selu_t3 == nil then __selu_t3 = 0 end
        __selu_t4 = __selu_t3 > 0
      end
      if __selu_t4 then return 0 end
    end
  end
  local __selu_t5, __selu_t6, __selu_t7, __selu_t8, __selu_t9, cls = C_Item.GetItemInfoInstant(info.hyperlink)
  if cls == 12 then return 0 end
  do
    local __selu_t10 = info.quality
    if __selu_t10 == nil then __selu_t10 = 0 end
    if __selu_t10 > settings.maxQuality or locked(info.itemID) then return 0 end
  end
  local __selu_t11, __selu_t12, __selu_t13, __selu_t14, __selu_t15, __selu_t16, __selu_t17, __selu_t18, __selu_t19, __selu_t20, price = C_Item.GetItemInfo(info.hyperlink)
  local value = (price == nil and 0 or price) * info.stackCount
  if value <= 0 or settings.maxValue > 0 and value > settings.maxValue then return 0 end
  return value
end
local function repair()
  local ok, err = pcall(repairUnchecked)
  if not ok then
    if __selu_wow.enabled("warn", "Repair failed") then
      __selu_wow.write("warn", "Repair failed", "Repair failed")
    end
  end
end
local function eligible(bag, slot)
  local ok, value = pcall(eligibleUnchecked, bag, slot)
  if not ok then return 0 end
  return value
end
local function snapshot()
  local counts = {}
  if GetNumBuybackItems ~= nil and GetBuybackItemLink ~= nil then
    do
      local i = 1
      while true do
        if not (i <= GetNumBuybackItems()) then break end
        do
          local link = GetBuybackItemLink(i)
          if link ~= nil then
            local id = itemID(link)
            local __selu_t21 = counts[id]
            if __selu_t21 == nil then __selu_t21 = 0 end
            counts[id] = __selu_t21 + 1
          end
        end
        i = i + 1
      end
    end
  end
  return counts
end
local function recover()
  if not open then return end
  local current = snapshot()
  local added = {}
  do
    local __selu_t22 = current
    for id, count in pairs(__selu_t22) do
      local __selu_t23 = baseline[id]
      if __selu_t23 == nil then __selu_t23 = 0 end
      local __selu_t24 = added
      local __selu_t25 = id
      local __selu_t26 = math.max(0, count - __selu_t23)
      __selu_t24[__selu_t25] = __selu_t26
    end
  end
  if GetNumBuybackItems ~= nil and GetBuybackItemLink ~= nil then
    do
      local i = GetNumBuybackItems()
      while true do
        if not (i >= 1) then break end
        do
          repeat
            local link = GetBuybackItemLink(i)
            if link == nil then break end
            local id = itemID(link)
            local pending = added[id]
            if pending == nil then pending = 0 end
            if pending <= 0 then break end
            added[id] = pending - 1
            if not locked(id) then break end
            if BuybackItem ~= nil then
              BuybackItem(i)
              if GetLocale() == "deDE" then
                print(string.format("%s zurückgekauft - der Gegenstand ist geschützt.", link))
              else
                print(string.format("Bought back %s - it's protected.", link))
              end
            elseif GetLocale() == "deDE" then
              print(string.format("%s ist geschützt und wurde gerade verkauft, aber dieser Client bietet keine Rückkauf-API. Kaufe den Gegenstand im Rückkauf-Tab des Händlers zurück.", link))
            else
              print(string.format("%s is protected and was just sold, but this client has no buyback API. Buy it back from the vendor's Buyback tab.", link))
            end
          until true
        end
        i = i - 1
      end
    end
  end
  baseline = current
end
__selu_on("MERCHANT_SHOW", function()
  open = true
  generation = generation + 1
  baseline = snapshot()
  if not settings.enabled then return end
  repair()
  local session = generation
  local queue = {}
  for bag = 0, 4 do
    do
      local slot = 1
      while true do
        if not (slot <= C_Container.GetContainerNumSlots(bag)) then break end
        do
          if eligible(bag, slot) > 0 then
            local info = C_Container.GetContainerItemInfo(bag, slot)
            if info ~= nil then
              local __selu_t27 = queue
              local __selu_t28 = { bag = bag, slot = slot, id = info.itemID }
              __selu_t27[#__selu_t27 + 1] = __selu_t28
            end
          end
        end
        slot = slot + 1
      end
    end
  end
  local count = 0
  local total = 0
  do
    local __selu_t29 = queue
    for __selu_t32 = 1, #__selu_t29 do
      local entry = __selu_t29[__selu_t32]
      if entry == nil then break end
      local __selu_t30 = -1
      for __selu_t31 = 1, #queue do
        if queue[__selu_t31] == entry then
          __selu_t30 = __selu_t31 - 1
          break
        end
      end
      local index = __selu_t30
      C_Timer.After(index * 0.2, function()
        if not open or generation ~= session or not settings.enabled then return end
        local info = C_Container.GetContainerItemInfo(entry.bag, entry.slot)
        if info ~= nil and info.itemID == entry.id then
          local value = eligible(entry.bag, entry.slot)
          if value > 0 then
            local ok, result = pcall(C_Container.UseContainerItem, entry.bag, entry.slot)
            if not ok or result == false then return end
            count = count + 1
            total = total + value
          end
        end
        if index == #queue - 1 then saySold(count, total) end
      end)
    end
  end
end)
__selu_on("MERCHANT_CLOSED", function()
  open = false
  generation = generation + 1
end)
local function scans()
  if not open then return end
  do
    local __selu_t33 = { 0.05, 0.20, 0.60 }
    for __selu_t34 = 1, #__selu_t33 do
      local delay = __selu_t33[__selu_t34]
      if delay == nil then break end
      C_Timer.After(delay, function()
        recover()
      end)
    end
  end
end
__selu_on("MERCHANT_UPDATE", function()
  scans()
end)
__selu_on("BAG_UPDATE_DELAYED", function()
  scans()
end)
__selu_on("PLAYER_MONEY", function()
  scans()
end)
