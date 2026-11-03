local obj = { n = 9 }
local x = 1
mutate = function()
  x = 100
  return 2
end
local function use(obj)
  do
    local __selu_t1 = x
    local __selu_t2 = obj.n
    x = __selu_t1 + __selu_t2
  end
  print(x)
end
use(UIParent)
