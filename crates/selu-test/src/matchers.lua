-- Test assertions deliberately live outside the addon namespace.
local function entries(value)
  local meta = getmetatable(value)
  if meta and meta.__selu_test_entries then
    local rows, index = meta.__selu_test_entries(), 0
    return function()
      index = index + 1
      local row = rows[index]
      if row then return row[1], row[2] end
    end
  end
  return pairs(value)
end
local function equal(a, b, seen)
  if type(a) ~= type(b) then return false end
  if type(a) ~= "table" then return a == b end
  local ma, mb = getmetatable(a), getmetatable(b)
  if (ma and ma.__selu_test_frame) or (mb and mb.__selu_test_frame) then return rawequal(a, b) end
  seen = seen or {}
  if seen[a] and seen[a][b] then return true end
  seen[a] = seen[a] or {}
  seen[a][b] = true
  for k, v in entries(a) do if not equal(v, b[k], seen) then return false end end
  for k in entries(b) do if a[k] == nil then return false end end
  return true
end
local function render(value, seen)
  if type(value) == "string" then return string.format("%q", value) end
  if type(value) == "function" then return "<function>" end
  if type(value) == "userdata" or type(value) == "thread" then return "<" .. type(value) .. ">" end
  if type(value) ~= "table" then return tostring(value) end
  local meta = getmetatable(value)
  if meta and meta.__selu_test_frame then return "<frame>" end
  seen = seen or {}
  if seen[value] then return "<cycle>" end
  seen[value] = true
  local parts = {}
  for k, v in entries(value) do parts[#parts + 1] = render(k, seen) .. "=" .. render(v, seen) end
  table.sort(parts)
  return "{" .. table.concat(parts, ", ") .. "}"
end
local function expectation(actual, negated)
  local result = {}
  local function check(ok, name, expected, observed)
    if ok == negated then
      error("expected " .. render(observed == nil and actual or observed) .. (negated and " not " or " ") .. name .. " " .. render(expected), 3)
    end
  end
  result.toBe = function(expected) check(actual == expected, "to be", expected) end
  result.toEqual = function(expected) check(equal(actual, expected), "to equal", expected) end
  result.toBeNull = function() check(actual == nil, "to be", nil) end
  result.toThrow = function()
    if type(actual) ~= "function" then error("toThrow expects a function", 2) end
    local ok = pcall(actual)
    check(not ok, "to throw", nil)
  end
  result.toContain = function(expected)
    local found = false
    if type(actual) == "string" and type(expected) == "string" then found = string.find(actual, expected, 1, true) ~= nil
    elseif type(actual) == "table" then for _, v in entries(actual) do if equal(v, expected) then found = true end end end
    check(found, "to contain", expected)
  end
  result.toBeLessThan = function(expected) check(actual < expected, "to be less than", expected) end
  result.toBeGreaterThan = function(expected) check(actual > expected, "to be greater than", expected) end
  result.toHaveBeenCalled = function() check(#actual.calls() > 0, "to have been called", nil) end
  result.toHaveBeenCalledWith = function(...)
    local expected = {n = select("#", ...), ...}
    local found = false
    local calls = actual.calls()
    for _, call in ipairs(calls) do if equal(call, expected) then found = true end end
    check(found, "to have been called with", expected, calls)
  end
  setmetatable(result, {__index = function(_, key) if key == "not" then return expectation(actual, not negated) end end})
  return result
end
expect = function(actual) return expectation(actual, false) end
