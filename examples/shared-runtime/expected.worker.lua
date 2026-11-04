local __selu_addon, __selu_ns = ...
local __selu_raise = __selu_ns.__selu_raise
local __selu_protect = __selu_ns.__selu_protect_2
__selu_ns["worker"] = {}
local function ready()
  do
    local __selu_ok, __selu_status, __selu_value1 = __selu_protect(function()
      return "return", "ready"
    end)
    local __selu_final_ok, __selu_final_status, __selu_final_value1 = __selu_protect(function()
      print("cleanup")
    end)
    if not __selu_final_ok or __selu_final_status ~= nil then
      __selu_ok, __selu_status, __selu_value1 = __selu_final_ok, __selu_final_status, __selu_final_value1
    end
    if not __selu_ok then __selu_raise(__selu_status) end
    if __selu_status == "return" then return __selu_value1 end
  end
end
__selu_ns["worker"].ready = ready
