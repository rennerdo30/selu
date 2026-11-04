local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  local report = function(label)
    local show = function()
      print(string.format("Quest reminder: %s", label))
    end
    show()
  end
  report("Pick up supplies before leaving town")
end)
