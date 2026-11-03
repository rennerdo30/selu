local t = { x = 0 }
local old = t
mutate = function()
  t = { x = 9 }
  return 2
end
local obj = UIParent
do
  local __selu_t1 = t
  __selu_t1.x = obj[1]
end
print(old.x)
