local m = {}
local k = "a"
mutate = function()
  k = "b"
  return 2
end
local obj = UIParent
local __selu_t2 = k
local __selu_t1 = m[k]
if __selu_t1 == nil then __selu_t1 = 0 end
local __selu_t3 = m
local __selu_t4 = __selu_t2
local __selu_t5 = __selu_t1 + obj.n
__selu_t3[__selu_t4] = __selu_t5
print(m["a"])
print(m["b"])
