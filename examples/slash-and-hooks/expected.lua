local __selu_addon, __selu_ns = ...
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
local panel = CreateFrame("Frame")
panel:HookScript("OnShow", function(self)
  print(__selu_wow.text("Status panel ready", "Status panel ready"))
end)
hooksecurefunc("GetTime", function()
  if __selu_wow.enabled("info", "Clock checked") then
    __selu_wow.write("info", "Clock checked", "Clock checked")
  end
end)
local function showStatus(args)
  if __selu_wow.enabled("info", "Status requested: {0}") then
    __selu_wow.write("info", "Status requested: {0}", string.format("Status requested: %s", args))
  end
  print(__selu_wow.text("{0} tasks ready", "%s tasks ready", tostring(3)))
end
__selu_wow.command("/status", "/st", showStatus)
panel:Hide()
__selu_on("PLAYER_LOGIN", function()
  panel:Show()
  showStatus("login")
  print(string.format("Clock: %s", GetTime()))
end)
