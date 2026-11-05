local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_saved_runtime = __selu_ns.__selu_saved
local __selu_saved_rootRawget = rawget
local __selu_saved_rootEnvironment = _G
local __selu_saved_root_settings = function()
  return __selu_saved_rootRawget(__selu_saved_rootEnvironment, "SavedSettings_settings")
end
local __selu_saved_publish_settings = function(__selu_saved_value)
  SavedSettings_settings = __selu_saved_value
end
local __selu_saved_root_progress = function()
  return __selu_saved_rootRawget(__selu_saved_rootEnvironment, "SavedSettings_progress")
end
local __selu_saved_publish_progress = function(__selu_saved_value)
  SavedSettings_progress = __selu_saved_value
end
__selu_ns["main"] = {}
local settings
settings = __selu_saved_runtime.pending()
__selu_saved_runtime.entries["SavedSettings_settings"] = {}
__selu_saved_runtime.readers["SavedSettings_settings"] = {}
__selu_saved_runtime.contexts["SavedSettings_settings"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  __selu_saved_runtime.ordinary("SavedSettings_settings", __selu_saved_root_settings(), nil, "__selu_version", 2, { enabled = true, thresholdPercent = 35, panel = { scale = 1 } }, {
    fields = {
    ["enabled"] = "boolean",
    ["panel"] = { fields = { ["scale"] = "number" } },
    ["thresholdPercent"] = "number",
  },
  }, { panel = {} }, __selu_saved_runtime.entries["SavedSettings_settings"], __selu_saved_runtime.readers["SavedSettings_settings"], __selu_saved_runtime.contexts["SavedSettings_settings"], function(__selu_saved_result, __selu_saved_retire)
    if __selu_saved_result.data ~= nil then
      __selu_saved_publish_settings(__selu_saved_result.data)
    end
    __selu_saved_runtime.publishAdoption(settings, __selu_saved_result)
  end)
end
__selu_saved_runtime.readers["SavedSettings_settings"][1] = {
  fields = {
  ["enabled"] = "boolean",
  ["panel"] = { fields = { ["scale"] = "number" } },
  ["thresholdPercent"] = { optional = "number" },
},
}
__selu_saved_runtime.entries["SavedSettings_settings"][1] = function(old)
  old.thresholdPercent = 35
end
local progress
progress = __selu_saved_runtime.pending()
__selu_saved_runtime.entries["SavedSettings_progress"] = {}
__selu_saved_runtime.readers["SavedSettings_progress"] = {}
__selu_saved_runtime.contexts["SavedSettings_progress"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  __selu_saved_runtime.ordinary("SavedSettings_progress", __selu_saved_root_progress(), nil, "__selu_version", 1, { visits = 0 }, { fields = { ["visits"] = "number" } }, {}, __selu_saved_runtime.entries["SavedSettings_progress"], __selu_saved_runtime.readers["SavedSettings_progress"], __selu_saved_runtime.contexts["SavedSettings_progress"], function(__selu_saved_result, __selu_saved_retire)
    if __selu_saved_result.data ~= nil then
      __selu_saved_publish_progress(__selu_saved_result.data)
    end
    __selu_saved_runtime.publishAdoption(progress, __selu_saved_result)
  end)
end
local function initializeSaved()
  settings.whenReady(function(settings)
    progress.whenReady(function(progress)
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
    end)
  end)
end
initializeSaved()
