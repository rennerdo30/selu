local __selu_addon, __selu_ns = ...
local __selu_setmetatable = setmetatable
local __selu_saved__G = _G
local __selu_saved_type = type
local __selu_saved_pairs = pairs
local __selu_saved_ipairs = ipairs
local __selu_saved_CreateFrame = CreateFrame
local __selu_saved_rawget = rawget
local __selu_saved_pcall = pcall
local __selu_saved_print = print
local __selu_saved_tostring = tostring
__selu_ns = __selu_ns or {}
local __selu_saved_runtime = __selu_ns.__selu_saved
if __selu_saved_runtime == nil then
  __selu_saved_runtime = {}
  __selu_ns.__selu_saved = __selu_saved_runtime
  __selu_saved_runtime.copy = function(__selu_saved_value)
    if __selu_saved_type(__selu_saved_value) ~= "table" then return __selu_saved_value end
    local __selu_saved_result = {}
    for __selu_saved_key, __selu_saved_value in __selu_saved_pairs(__selu_saved_value) do
      __selu_saved_result[__selu_saved_key] = __selu_saved_runtime.copy(__selu_saved_value)
    end
    return __selu_saved_result
  end
  __selu_saved_runtime.fill = function(__selu_saved_data, __selu_saved_defaults, __selu_saved_objects)
    for __selu_saved_key, __selu_saved_value in __selu_saved_pairs(__selu_saved_defaults) do
      if __selu_saved_data[__selu_saved_key] == nil then
        __selu_saved_data[__selu_saved_key] = __selu_saved_runtime.copy(__selu_saved_value)
      elseif __selu_saved_objects[__selu_saved_key] ~= nil and __selu_saved_type(__selu_saved_data[__selu_saved_key]) == "table" then
        __selu_saved_runtime.fill(__selu_saved_data[__selu_saved_key], __selu_saved_value, __selu_saved_objects[__selu_saved_key])
      end
    end
  end
  __selu_saved_runtime.warn = function(__selu_saved_message)
    local __selu_saved_wow = __selu_ns.__selu_wow
    if __selu_saved_wow ~= nil then
      if __selu_saved_wow.enabled("warn", __selu_saved_message) then
        __selu_saved_wow.write("warn", __selu_saved_message, __selu_saved_message)
      end
    else
      __selu_saved_print(__selu_saved_message)
    end
  end
  __selu_saved_runtime.load = function(__selu_saved_key, __selu_saved_current, __selu_saved_defaults, __selu_saved_objects, __selu_saved_migrations)
    local __selu_saved_data = __selu_saved_rawget(__selu_saved__G, __selu_saved_key)
    if __selu_saved_data == nil then
      __selu_saved_data = {}
      __selu_saved_data.__selu_version = __selu_saved_current
    end
    if __selu_saved_type(__selu_saved_data) ~= "table" then
      __selu_saved_runtime.warn(__selu_saved_key .. ": saved root is not a table")
      return __selu_saved_data
    end
    local __selu_saved_version = __selu_saved_rawget(__selu_saved_data, "__selu_version")
    if __selu_saved_version == nil then __selu_saved_version = 1 end
    if __selu_saved_type(__selu_saved_version) ~= "number" or (__selu_saved_version < 1 or __selu_saved_version % 1 ~= 0) then
      __selu_saved_runtime.warn(__selu_saved_key .. ": invalid saved version")
      return __selu_saved_data
    end
    if __selu_saved_version > __selu_saved_current then
      __selu_saved_runtime.warn(__selu_saved_key .. ": saved data is newer than this addon")
      return __selu_saved_data
    end
    while __selu_saved_version < __selu_saved_current do
      local __selu_saved_migration = __selu_saved_migrations[__selu_saved_version]
      if __selu_saved_migration == nil then
        __selu_saved_runtime.warn(__selu_saved_key .. ": missing saved migration")
        return __selu_saved_data
      end
      local __selu_saved_candidate = __selu_saved_runtime.copy(__selu_saved_data)
      local __selu_saved_ok, __selu_saved_message = __selu_saved_pcall(__selu_saved_migration, __selu_saved_candidate)
      if not __selu_saved_ok then
        __selu_saved_runtime.warn(__selu_saved_key .. ": migration failed: " .. __selu_saved_tostring(__selu_saved_message))
        return __selu_saved_data
      end
      __selu_saved_version = __selu_saved_version + 1
      __selu_saved_data = __selu_saved_candidate
      __selu_saved_data.__selu_version = __selu_saved_version
      __selu_saved__G[__selu_saved_key] = __selu_saved_data
    end
    __selu_saved_runtime.fill(__selu_saved_data, __selu_saved_defaults, __selu_saved_objects)
    __selu_saved_data.__selu_version = __selu_saved_current
    __selu_saved__G[__selu_saved_key] = __selu_saved_data
    return __selu_saved_data
  end
  __selu_saved_runtime.entries = {}
  __selu_saved_runtime.loaders = {}
  local __selu_saved_frame = __selu_saved_CreateFrame("Frame")
  __selu_saved_frame:RegisterEvent("ADDON_LOADED")
  __selu_saved_frame:SetScript("OnEvent", function(__selu_saved_self, __selu_saved_event, __selu_saved_addon)
    if __selu_saved_addon ~= __selu_addon then return end
    for __selu_saved_key, __selu_saved_value in __selu_saved_ipairs(__selu_saved_runtime.loaders) do
      local __selu_saved_ok, __selu_saved_message = __selu_saved_pcall(__selu_saved_value)
      if not __selu_saved_ok then
        __selu_saved_runtime.warn(__selu_saved_tostring(__selu_saved_message))
      end
    end
    __selu_saved_runtime.entries = nil
    __selu_saved_runtime.loaders = nil
    __selu_saved_self:UnregisterEvent("ADDON_LOADED")
    __selu_saved_self:SetScript("OnEvent", nil)
    __selu_ns.__selu_saved = nil
  end)
end
-- Selu: shared event frame and dispatch table
local __selu_handlers = {}
local __selu_frame = CreateFrame("Frame")
__selu_frame:SetScript("OnEvent", function(self, event, ...)
  local handlers = __selu_handlers[event]
  if not handlers then return end
  for i = 1, #handlers do handlers[i](...) end
end)
local function __selu_on(event, fn)
  local handlers = __selu_handlers[event]
  if not handlers then
    handlers = {}
    __selu_handlers[event] = handlers
    __selu_frame:RegisterEvent(event)
  end
  handlers[#handlers + 1] = fn
end
local settings
__selu_saved_runtime.entries["Addon_settings"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  settings = __selu_saved_runtime.load("Addon_settings", 1, { enabled = false }, {}, __selu_saved_runtime.entries["Addon_settings"])
end
local C = {}
C.methods = {}
C.methods.__index = C.methods
function C.new()
  local self = __selu_setmetatable({}, C.methods)
  self.value = 1
  return self
end
__selu_on("PLAYER_LOGIN", function()
  print(settings.enabled)
end)
