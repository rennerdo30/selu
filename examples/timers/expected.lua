local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local reminderPending = false
local function queueReminder()
  if reminderPending then
    return
  end
  reminderPending = true
  print("Bag check queued; no items will be sold.")
  C_Timer.After(0.2, function()
    reminderPending = false
    print("Bag check ready: visit a merchant to review your grey items.")
  end)
end
__selu_on("PLAYER_LOGIN", function()
  queueReminder()
end)
