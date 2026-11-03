local xs = { 1, 2, 3 }
local __selu_t1 = {}
for __selu_t2 = 1, #xs do
  local __selu_t3 = xs[__selu_t2]
  if __selu_t3 == nil then break end
  local x = __selu_t3
  __selu_t1[#__selu_t1 + 1] = x * 2
end
local mapped = __selu_t1
local __selu_t4 = {}
for __selu_t5 = 1, #xs do
  local __selu_t6 = xs[__selu_t5]
  if __selu_t6 == nil then break end
  local x = __selu_t6
  if x > 1 then __selu_t4[#__selu_t4 + 1] = __selu_t6 end
end
local filtered = __selu_t4
local __selu_t7 = nil
for __selu_t8 = 1, #xs do
  local __selu_t9 = xs[__selu_t8]
  if __selu_t9 == nil then break end
  local x = __selu_t9
  if x > 1 then
    __selu_t7 = __selu_t9
    break
  end
end
local found = __selu_t7
local __selu_t10 = false
for __selu_t11 = 1, #xs do
  local __selu_t12 = xs[__selu_t11]
  if __selu_t12 == nil then break end
  local x = __selu_t12
  if x > 1 then
    __selu_t10 = true
    break
  end
end
local some = __selu_t10
local __selu_t13 = true
for __selu_t14 = 1, #xs do
  local __selu_t15 = xs[__selu_t14]
  if __selu_t15 == nil then break end
  local x = __selu_t15
  if not (x > 0) then
    __selu_t13 = false
    break
  end
end
local every = __selu_t13
local __selu_t16 = 0
for __selu_t17 = 1, #xs do
  local __selu_t18 = xs[__selu_t17]
  if __selu_t18 == nil then break end
  local sum, x = __selu_t16, __selu_t18
  __selu_t16 = sum + x
end
local total = __selu_t16
for __selu_t20 = 1, #xs do
  local __selu_t21 = xs[__selu_t20]
  if __selu_t21 == nil then break end
  local x = __selu_t21
  print(x)
end
