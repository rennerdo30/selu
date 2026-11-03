local __selu_setmetatable = setmetatable
local C = {}
C.methods = {}
C.methods.__index = C.methods
function C.new()
  local self = __selu_setmetatable({}, C.methods)
  self.total = 0
  return self
end
function C.methods:add(amount)
  self.total = self.total + amount
end
local c = C.new()
c:add(3)
print(c.total)
local obj = { n = 2 }
local x = 1
x = x + obj.n
print(x)
