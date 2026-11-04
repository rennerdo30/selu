local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
__selu_on("PLAYER_LOGIN", function()
  print("Sale queued")
  C_Timer.After(0.2, function()
    print("Sale ready")
  end)
end)
