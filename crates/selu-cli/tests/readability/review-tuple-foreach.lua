local function hit(x)
  print(x)
  return x
end
local xs = { 1 }
local __selu_t1 = function(x)
  return hit(x), (hit(x + 10))
end
for __selu_t3 = 1, #xs do
  local __selu_t4 = xs[__selu_t3]
  if __selu_t4 == nil then break end
  __selu_t1(__selu_t4, __selu_t3 - 1, xs)
end
print("done")
