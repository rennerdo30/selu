local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local cooldowns = {
  { spell = "Hearthstone", seconds = 754 },
  { spell = "Feign Death", seconds = 0 },
  { spell = "Bestial Wrath", seconds = 42 },
  { spell = "Aspect of the Cheetah", seconds = 0 },
  { spell = "Misdirection", seconds = 61 },
}
local function split(seconds)
  return math.floor(seconds / 60), seconds % 60
end
local function readable(seconds)
  local minutes, rest = split(seconds)
  return minutes > 0 and string.format("%sm %ss", minutes, rest) or string.format("%ss", rest)
end
local function counter()
  local n = 0
  return function()
    n = n + 1
    return n
  end
end
__selu_on("PLAYER_LOGIN", function()
  local __selu_t1 = {}
  for __selu_t2 = 1, #cooldowns do
    local __selu_t3 = cooldowns[__selu_t2]
    if __selu_t3 == nil then break end
    local c = __selu_t3
    if c.seconds > 0 then __selu_t1[#__selu_t1 + 1] = __selu_t3 end
  end
  local waiting = __selu_t1
  do
    local __selu_t4 = function(a, b)
      return a.seconds - b.seconds
    end
    table.sort(waiting, function(__selu_t5, __selu_t6)
      return __selu_t4(__selu_t5, __selu_t6) < 0
    end)
  end
  local line = counter()
  do
    local __selu_t7 = waiting
    for __selu_t8 = 1, #__selu_t7 do
      local c = __selu_t7[__selu_t8]
      if c == nil then break end
      print(string.format("%s. %s: %s", line(), c.spell, readable(c.seconds)))
    end
  end
  local ready = 0
  do
    local __selu_t9 = cooldowns
    for __selu_t10 = 1, #__selu_t9 do
      local c = __selu_t9[__selu_t10]
      if c == nil then break end
      repeat
        if c.seconds > 0 then
          break
        end
        ready = ready + 1
      until true
    end
  end
  print(string.format("%s of %s spells are ready", ready, #cooldowns))
end)
