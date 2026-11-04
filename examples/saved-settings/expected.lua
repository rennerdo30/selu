local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_saved_runtime = __selu_ns.__selu_saved_runtime
__selu_ns["main"] = {}
local settings
__selu_saved_runtime.entries["SavedSettings_settings"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  settings = __selu_saved_runtime.load("SavedSettings_settings", 2, { enabled = true, thresholdPercent = 35, panel = { scale = 1 } }, { panel = {} }, __selu_saved_runtime.entries["SavedSettings_settings"])
end
__selu_saved_runtime.entries["SavedSettings_settings"][1] = function(old)
  old.thresholdPercent = 35
end
local progress
__selu_saved_runtime.entries["SavedSettings_progress"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  progress = __selu_saved_runtime.load("SavedSettings_progress", 1, { visits = 0 }, {}, __selu_saved_runtime.entries["SavedSettings_progress"])
end
__selu_on("PLAYER_LOGIN", function()
  do
    local __selu_t1 = progress
    local __selu_t2 = __selu_t1.visits
    __selu_t1.visits = __selu_t2 + 1
  end
  print(settings.enabled)
  print(settings.thresholdPercent)
  print(progress.visits)
end)
