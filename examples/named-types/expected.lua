local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_type = type
__selu_ns["main"] = {}
local function score(value)
  if __selu_type(value) == "number" then return value end
  return 0
end
__selu_on("PLAYER_LOGIN", function()
  local entry = { label = "Quest", points = score(7) }
  local scores = {}
  local __selu_t1 = scores
  local __selu_t2 = entry.points
  __selu_t1[#__selu_t1 + 1] = __selu_t2
  print(entry.label, scores[1])
end)
