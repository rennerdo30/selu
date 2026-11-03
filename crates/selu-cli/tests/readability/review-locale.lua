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
local function f()
  return (UnitName("player"))
end
emit = function()
  print(__selu_wow.text("{0}", "%s", (f())))
end
print(__selu_wow.text("ready", "ready"))
