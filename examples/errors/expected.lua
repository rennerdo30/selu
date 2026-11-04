local __selu_addon, __selu_ns = ...
local __selu_new_error = __selu_ns.__selu_new_error
local __selu_wrap_error = __selu_ns.__selu_wrap_error
local __selu_raise = __selu_ns.__selu_raise
local __selu_protect = __selu_ns.__selu_protect_2
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local function readBalance()
  local failure = __selu_new_error("Balance unavailable", "main.selu:2: ")
  do
    local __selu_ok, __selu_status, __selu_value1 = __selu_protect(function()
      __selu_raise(failure)
    end)
    if not __selu_ok then
      __selu_ok, __selu_status, __selu_value1 = __selu_protect(function()
        local e = __selu_wrap_error(__selu_status)
        print(e.message)
        return "return", 0
      end)
    end
    local __selu_final_ok, __selu_final_status, __selu_final_value1 = __selu_protect(function()
      print("Balance check finished")
    end)
    if not __selu_final_ok or __selu_final_status ~= nil then
      __selu_ok, __selu_status, __selu_value1 = __selu_final_ok, __selu_final_status, __selu_final_value1
    end
    if not __selu_ok then __selu_raise(__selu_status) end
    if __selu_status == "return" then return __selu_value1 end
  end
end
__selu_on("PLAYER_LOGIN", function()
  print(readBalance())
end)
