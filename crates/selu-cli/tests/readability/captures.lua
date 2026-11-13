local __selu_addon, __selu_ns = ...
local __selu_setmetatable = setmetatable
local __selu_saved__G = _G
local __selu_saved_type = type
local __selu_saved_pairs = pairs
local __selu_saved_next = next
local __selu_saved_ipairs = ipairs
local __selu_saved_CreateFrame = CreateFrame
local __selu_saved_rawget = rawget
local __selu_saved_pcall = pcall
local __selu_saved_print = print
local __selu_saved_tostring = tostring
local __selu_saved_rootRawget = rawget
local __selu_saved_rootEnvironment = _G
local __selu_saved_root_settings = function()
  return __selu_saved_rootRawget(__selu_saved_rootEnvironment, "Addon_settings")
end
local __selu_saved_publish_settings = function(__selu_saved_value)
  Addon_settings = __selu_saved_value
end
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
if __selu_saved_runtime.adopt == nil then
  local __selu_saved_type = type
  local __selu_saved_pairs = pairs
  local __selu_saved_next = next
  local __selu_saved_ipairs = ipairs
  local __selu_saved_pcall = pcall
  local __selu_saved_tostring = tostring
  local __selu_saved_tonumber = tonumber
  local function __selu_saved_opaque(__selu_value, __selu_seen, __selu_done)
    local __selu_kind = __selu_saved_type(__selu_value)
    if __selu_kind == "string" or __selu_kind == "boolean" then return true end
    if __selu_kind == "number" then return __selu_value - __selu_value == 0 end
    if __selu_kind ~= "table" or __selu_seen[__selu_value] then return false end
    if __selu_done[__selu_value] then return true end
    __selu_seen[__selu_value] = true
    for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
      local __selu_key_kind = __selu_saved_type(__selu_key)
      if (__selu_key_kind ~= "string" and __selu_key_kind ~= "boolean" and __selu_key_kind ~= "number") or not __selu_saved_opaque(__selu_key, __selu_seen, __selu_done) or not __selu_saved_opaque(__selu_item, __selu_seen, __selu_done) then return false end
    end
    __selu_seen[__selu_value] = nil
    __selu_done[__selu_value] = true
    return true
  end
  -- Closed persistence boundary. Unknown fields stay in the table but are never projected.
  -- Cache only complete successes, never an in-progress or failed validation.
  local function __selu_saved_validated(__selu_value, __selu_schema, __selu_missing, __selu_done)
    if __selu_saved_type(__selu_value) == "table" then
      local __selu_schemas = __selu_done[__selu_value] or {}
      local __selu_modes = __selu_schemas[__selu_schema] or {}
      __selu_done[__selu_value] = __selu_schemas
      __selu_schemas[__selu_schema] = __selu_modes
      __selu_modes[__selu_missing] = true
    end
    return true
  end
  local function __selu_saved_validate(__selu_value, __selu_schema, __selu_missing, __selu_seen, __selu_path, __selu_done)
    __selu_path = __selu_path or "$"
    __selu_done = __selu_done or {}
    if __selu_saved_type(__selu_schema) == "string" then
      if (__selu_missing and __selu_value == nil) or __selu_saved_type(__selu_value) == __selu_schema then return true end
      return false, __selu_path .. ": expected " .. __selu_schema .. ", got " .. __selu_saved_type(__selu_value)
    end
    -- Completed results are pass-local and schema/mode specific; active ancestors
    -- still refuse cycles even when this triple succeeded on a different path.
    if __selu_saved_type(__selu_value) == "table" then
      if __selu_seen[__selu_value] then return false, __selu_path .. ": cyclic record" end
      local __selu_schemas = __selu_done[__selu_value]
      local __selu_modes = __selu_schemas and __selu_schemas[__selu_schema]
      if __selu_modes and __selu_modes[__selu_missing] then return true end
    end
    if __selu_schema.literal ~= nil then
      if (__selu_missing and __selu_value == nil) or __selu_value == __selu_schema.literal then return true end
      return false, __selu_path .. ": wrong literal value"
    end
    if __selu_schema.union then
      for __selu_index = 1, #__selu_schema.union do
        -- A failed arm may leave active-path marks. Each alternative owns its path.
        local __selu_branch = {}
        for __selu_table, __selu_active in __selu_saved_pairs(__selu_seen) do
          __selu_branch[__selu_table] = __selu_active
        end
        if __selu_saved_validate(__selu_value, __selu_schema.union[__selu_index], __selu_missing, __selu_branch, __selu_path, __selu_done) then
          return __selu_saved_validated(__selu_value, __selu_schema, __selu_missing, __selu_done)
        end
      end
      return false, __selu_path .. ": value matches no union member"
    end
    if __selu_schema.optional then
      if __selu_value == nil then return true end
      local __selu_valid, __selu_reason = __selu_saved_validate(__selu_value, __selu_schema.optional, false, __selu_seen, __selu_path, __selu_done)
      if not __selu_valid then return false, __selu_reason end
      return __selu_saved_validated(__selu_value, __selu_schema, __selu_missing, __selu_done)
    end
    if __selu_schema.repair then
      local __selu_kind = __selu_saved_type(__selu_value)
      if __selu_kind == "string" or __selu_kind == "number" or __selu_kind == "boolean" then return true end
      return __selu_saved_validate(__selu_value, __selu_schema.repair, __selu_missing, __selu_seen, __selu_path, __selu_done)
    end
    if __selu_missing and __selu_value == nil then return true end
    if __selu_saved_type(__selu_value) ~= "table" then return false, __selu_path .. ": expected table, got " .. __selu_saved_type(__selu_value) end
    __selu_seen[__selu_value] = true
    if __selu_schema.fields and __selu_saved_next(__selu_schema.fields) == nil then
      __selu_seen[__selu_value] = nil
      if not __selu_saved_opaque(__selu_value, __selu_seen, {}) then return false, __selu_path .. ": expected serializable table" end
    elseif __selu_schema.fields then
      for __selu_key, __selu_field in __selu_saved_pairs(__selu_schema.fields) do
        local __selu_valid, __selu_reason = __selu_saved_validate(__selu_value[__selu_key], __selu_field, __selu_missing, __selu_seen, __selu_path .. "." .. __selu_key, __selu_done)
        if not __selu_valid then return false, __selu_reason end
      end
    elseif __selu_schema.array then
      local __selu_count, __selu_max = 0, 0
      for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
        if __selu_saved_type(__selu_key) ~= "number" or __selu_key < 1 or __selu_key % 1 ~= 0 then return false, __selu_path .. ": invalid array index" end
        __selu_count = __selu_count + 1
        if __selu_key > __selu_max then __selu_max = __selu_key end
        local __selu_valid, __selu_reason = __selu_saved_validate(__selu_item, __selu_schema.array, false, __selu_seen, __selu_path .. "[" .. __selu_saved_tostring(__selu_key) .. "]", __selu_done)
        if not __selu_valid then return false, __selu_reason end
      end
      if __selu_count ~= __selu_max then return false, __selu_path .. ": array has holes" end
    elseif __selu_schema.key then
      for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
        local __selu_valid, __selu_reason = __selu_saved_validate(__selu_key, __selu_schema.key, false, __selu_seen, __selu_path .. "[key]", __selu_done)
        if not __selu_valid then return false, __selu_reason end
        __selu_valid, __selu_reason = __selu_saved_validate(__selu_item, __selu_schema.value, false, __selu_seen, __selu_path .. "[value]", __selu_done)
        if not __selu_valid then return false, __selu_reason end
      end
    else return false, __selu_path .. ": invalid schema" end
    __selu_seen[__selu_value] = nil
    return __selu_saved_validated(__selu_value, __selu_schema, __selu_missing, __selu_done)
  end
  -- Copy with provenance so successful commits keep existing table identities and aliases.
  local function __selu_saved_clone(__selu_value, __selu_forward, __selu_reverse)
    if __selu_saved_type(__selu_value) ~= "table" then return __selu_value end
    if __selu_forward[__selu_value] then return __selu_forward[__selu_value] end
    local __selu_copy = {}
    __selu_forward[__selu_value], __selu_reverse[__selu_copy] = __selu_copy, __selu_value
    for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
      __selu_copy[__selu_key] = __selu_saved_clone(__selu_item, __selu_forward, __selu_reverse)
    end
    return __selu_copy
  end
  local function __selu_saved_commit(__selu_value, __selu_reverse, __selu_seen)
    if __selu_saved_type(__selu_value) ~= "table" then return __selu_value end
    if __selu_seen[__selu_value] then return __selu_seen[__selu_value] end
    local __selu_target = __selu_reverse[__selu_value] or {}
    __selu_seen[__selu_value] = __selu_target
    for __selu_key in __selu_saved_pairs(__selu_target) do
      if __selu_value[__selu_key] == nil then __selu_target[__selu_key] = nil end
    end
    for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
      __selu_target[__selu_key] = __selu_saved_commit(__selu_item, __selu_reverse, __selu_seen)
    end
    return __selu_target
  end
  local function __selu_saved_equal(__selu_left, __selu_right, __selu_seen)
    if __selu_left == __selu_right then return true end
    if __selu_saved_type(__selu_left) ~= "table" or __selu_saved_type(__selu_right) ~= "table" then return false end
    if __selu_seen[__selu_left] then return __selu_seen[__selu_left] == __selu_right end
    __selu_seen[__selu_left] = __selu_right
    for __selu_key, __selu_value in __selu_saved_pairs(__selu_left) do
      if not __selu_saved_equal(__selu_value, __selu_right[__selu_key], __selu_seen) then return false end
    end
    for __selu_key in __selu_saved_pairs(__selu_right) do
      if __selu_left[__selu_key] == nil then return false end
    end
    return true
  end
  -- A future context reader owns its input until that reader commits.
  local function __selu_saved_protect(__selu_before, __selu_after, __selu_path, __selu_deletions, __selu_forward, __selu_reverse, __selu_seen)
    if __selu_saved_type(__selu_before) ~= "table" or __selu_saved_type(__selu_after) ~= "table" or __selu_seen[__selu_before] then return end
    __selu_seen[__selu_before] = true
    for __selu_key, __selu_value in __selu_saved_pairs(__selu_before) do
      local __selu_child_path = __selu_saved_clone(__selu_path, {}, {})
      __selu_child_path[#__selu_child_path + 1] = __selu_key
      if __selu_after[__selu_key] == nil then
        __selu_deletions[#__selu_deletions + 1] = __selu_child_path
        __selu_after[__selu_key] = __selu_saved_clone(__selu_value, __selu_forward, __selu_reverse)
      end
      __selu_saved_protect(__selu_value, __selu_after[__selu_key], __selu_child_path, __selu_deletions, __selu_forward, __selu_reverse, __selu_seen)
    end
  end
  local function __selu_saved_valid_paths(__selu_paths)
    if __selu_saved_type(__selu_paths) ~= "table" then return false end
    local __selu_count = 0
    for __selu_index, __selu_path in __selu_saved_pairs(__selu_paths) do
      if __selu_saved_type(__selu_index) ~= "number" or __selu_index < 1 or __selu_index % 1 ~= 0 or __selu_index > #__selu_paths then return false end
      if __selu_saved_type(__selu_path) ~= "table" or #__selu_path == 0 then return false end
      local __selu_length = 0
      for __selu_position, __selu_key in __selu_saved_pairs(__selu_path) do
        if __selu_saved_type(__selu_position) ~= "number" or __selu_position < 1 or __selu_position % 1 ~= 0 or __selu_position > #__selu_path then return false end
        local __selu_kind = __selu_saved_type(__selu_key)
        if __selu_kind ~= "string" and __selu_kind ~= "boolean" and (__selu_kind ~= "number" or __selu_key - __selu_key ~= 0) then return false end
        __selu_length = __selu_length + 1
      end
      if __selu_length ~= #__selu_path then return false end
      __selu_count = __selu_count + 1
    end
    return __selu_count == #__selu_paths
  end
  -- A scalar where the defaults hold a table cannot be used by any code. It is
  -- dropped so the defaults fill the place; each repair is reported once the
  -- load has committed. Values the schema accepts (repair unions) are kept.
  local function __selu_saved_repair(__selu_data, __selu_defaults, __selu_schema, __selu_objects, __selu_path, __selu_repairs)
    local __selu_fields = __selu_schema.fields
    if not __selu_fields then return end
    for __selu_key, __selu_default in __selu_saved_pairs(__selu_defaults) do
      local __selu_field = __selu_fields[__selu_key]
      if __selu_field ~= nil and __selu_saved_type(__selu_default) == "table" then
        local __selu_saved = __selu_data[__selu_key]
        local __selu_kind = __selu_saved_type(__selu_saved)
        local __selu_child = __selu_path .. "." .. __selu_saved_tostring(__selu_key)
        if __selu_kind == "table" then
          local __selu_inner = __selu_objects and __selu_objects[__selu_key]
          if __selu_inner ~= nil then
            if __selu_saved_type(__selu_field) == "table" and __selu_field.optional then __selu_field = __selu_field.optional end
            if __selu_saved_type(__selu_field) == "table" then
              __selu_saved_repair(__selu_saved, __selu_default, __selu_field, __selu_inner, __selu_child, __selu_repairs)
            end
          end
        elseif (__selu_kind == "string" or __selu_kind == "number" or __selu_kind == "boolean") and not __selu_saved_validate(__selu_saved, __selu_field, false, {}) then
          __selu_data[__selu_key] = nil
          __selu_repairs[#__selu_repairs + 1] = __selu_child .. " held a " .. __selu_kind .. " where a table belongs; replaced by the defaults"
        end
      end
    end
  end
  -- A hand-edited stamp such as "12" is read as its number; anything else that
  -- is no number stays malformed.
  local function __selu_saved_stamp(__selu_value)
    if __selu_saved_type(__selu_value) == "string" then return __selu_saved_tonumber(__selu_value) or __selu_value end
    return __selu_value
  end
  __selu_saved_runtime.adopt = function(__selu_database, __selu_current, __selu_legacy, __selu_version_key, __selu_latest, __selu_defaults, __selu_schema, __selu_default_objects, __selu_steps, __selu_readers, __selu_contexts, __selu_publish)
    local __selu_root = __selu_current
    -- An empty current table holds nothing, so it never hides a filled legacy one.
    if __selu_saved_type(__selu_root) == "table" and __selu_saved_next(__selu_root) == nil
      and __selu_saved_type(__selu_legacy) == "table" and __selu_saved_next(__selu_legacy) ~= nil then
      __selu_root = __selu_legacy
    end
    if __selu_root == nil then __selu_root = __selu_legacy end
    if __selu_root == nil then __selu_root = {} end
    local __selu_phase = "load"
    local function __selu_diagnostic(__selu_status, __selu_reason)
      __selu_saved_runtime.warn("[saved] " .. __selu_database .. ": " .. __selu_phase .. ": " .. __selu_status .. ": " .. __selu_reason)
    end
    local function __selu_refuse(__selu_status, __selu_reason)
      __selu_diagnostic(__selu_status, __selu_reason)
      __selu_publish({ status = __selu_status }, false)
    end
    if __selu_saved_type(__selu_root) ~= "table" then __selu_refuse("corrupt", "saved root expected table, got " .. __selu_saved_type(__selu_root)); return end
    local __selu_version = __selu_saved_stamp(__selu_root[__selu_version_key])
    if __selu_version == nil then
      local __selu_empty = true
      for __selu_key in __selu_saved_pairs(__selu_root) do __selu_empty = false; break end
      __selu_version = __selu_empty and __selu_latest or 1
    end
    if __selu_saved_type(__selu_version) ~= "number" or __selu_version < 1 or __selu_version % 1 ~= 0 then __selu_refuse("corrupt", "invalid " .. __selu_version_key .. ": expected a positive integer"); return end
    if __selu_version > __selu_latest then __selu_refuse("newer", "saved version " .. __selu_version .. " is newer than supported version " .. __selu_latest); return end
    -- This is context, never a competing database version. Keep it only on failure.
    local __selu_context_key = "__selu_saved_adoption_context"
    local __selu_pending = __selu_root[__selu_context_key]
    if __selu_pending ~= nil and (__selu_saved_type(__selu_pending) ~= "table" or __selu_pending.format ~= 1 or __selu_saved_type(__selu_pending.original) ~= "table" or not __selu_saved_valid_paths(__selu_pending.removals)) then
      __selu_refuse("corrupt", "invalid retry context"); return
    end
    local __selu_original = __selu_pending and __selu_pending.original or __selu_saved_clone(__selu_root, {}, {})
    if __selu_original[__selu_context_key] ~= nil then __selu_refuse("corrupt", "invalid retry context"); return end
    local __selu_deletions = __selu_saved_clone(__selu_pending and __selu_pending.removals or {}, {}, {})
    local __selu_from = __selu_saved_stamp(__selu_original[__selu_version_key])
    if __selu_from == nil then __selu_from = 1 end
    if __selu_saved_type(__selu_from) ~= "number" or __selu_from < 1 or __selu_from % 1 ~= 0 or __selu_from > __selu_version then __selu_refuse("corrupt", "invalid retry context"); return end
    local __selu_has_context = false
    for __selu_key in __selu_saved_pairs(__selu_contexts) do __selu_has_context = true; break end
    while __selu_version < __selu_latest do
      __selu_phase = "migration " .. __selu_version .. " -> " .. (__selu_version + 1)
      local __selu_reader = __selu_readers[__selu_version]
      if not __selu_reader then __selu_refuse("corrupt", "missing historical reader"); return end
      local __selu_valid, __selu_reason = __selu_saved_validate(__selu_root, __selu_reader, false, {})
      if not __selu_valid then __selu_refuse("corrupt", "migration input " .. __selu_reason); return end
      local __selu_context = __selu_contexts[__selu_version]
      if __selu_context then
        __selu_valid, __selu_reason = __selu_saved_validate(__selu_original, __selu_context, false, {})
        if not __selu_valid then __selu_refuse("corrupt", "retry context " .. __selu_reason); return end
      end
      local __selu_reverse = {}
      local __selu_forward = {}
      local __selu_candidate = __selu_saved_clone(__selu_root, __selu_forward, __selu_reverse)
      local __selu_ok, __selu_error = __selu_saved_pcall(__selu_steps[__selu_version], __selu_candidate, __selu_saved_clone(__selu_original, {}, {}))
      if not __selu_ok then __selu_refuse("failed", __selu_saved_tostring(__selu_error)); return end
      local __selu_protected = {}
      for __selu_step, __selu_future in __selu_saved_pairs(__selu_contexts) do
        if __selu_step > __selu_version then
          for __selu_field in __selu_saved_pairs(__selu_future.fields) do __selu_protected[__selu_field] = true end
        end
      end
      -- Protect complete input subtrees, including opaque extension fields.
      for __selu_field in __selu_saved_pairs(__selu_protected) do
        if __selu_candidate[__selu_field] == nil and __selu_root[__selu_field] ~= nil then
          __selu_deletions[#__selu_deletions + 1] = { __selu_field }
          __selu_candidate[__selu_field] = __selu_saved_clone(__selu_root[__selu_field], __selu_forward, __selu_reverse)
        end
        __selu_saved_protect(__selu_root[__selu_field], __selu_candidate[__selu_field], { __selu_field }, __selu_deletions, __selu_forward, __selu_reverse, {})
      end
      local __selu_remaining = {}
      for __selu_index, __selu_path in __selu_saved_ipairs(__selu_deletions) do
        if __selu_protected[__selu_path[1]] then __selu_remaining[#__selu_remaining + 1] = __selu_path
        else
          local __selu_parent, __selu_previous = __selu_candidate, __selu_root
          for __selu_position = 1, #__selu_path - 1 do
            if __selu_saved_type(__selu_parent) == "table" then __selu_parent = __selu_parent[__selu_path[__selu_position]] end
            if __selu_saved_type(__selu_previous) == "table" then __selu_previous = __selu_previous[__selu_path[__selu_position]] end
          end
          if __selu_saved_type(__selu_parent) == "table" then
            local __selu_leaf = __selu_path[#__selu_path]
            local __selu_old
            if __selu_saved_type(__selu_previous) == "table" then __selu_old = __selu_previous[__selu_leaf] end
            if __selu_parent[__selu_leaf] ~= nil and not __selu_saved_equal(__selu_parent[__selu_leaf], __selu_old, {}) then __selu_refuse("failed", "conflicting write to a deferred input"); return end
            __selu_parent[__selu_leaf] = nil
          end
        end
      end
      __selu_deletions = __selu_remaining
      __selu_candidate[__selu_version_key] = __selu_version + 1
      local __selu_next = __selu_readers[__selu_version + 1] or __selu_schema
      __selu_valid, __selu_reason = __selu_saved_validate(__selu_candidate, __selu_next, __selu_version + 1 == __selu_latest, {})
      if not __selu_valid then __selu_refuse("corrupt", "migration result " .. __selu_reason); return end
      if __selu_has_context then
        __selu_candidate[__selu_context_key] = { format = 1, original = __selu_saved_clone(__selu_original, {}, {}), removals = __selu_saved_clone(__selu_deletions, {}, {}) }
      end
      __selu_saved_commit(__selu_candidate, __selu_reverse, {})
      __selu_version = __selu_version + 1
    end
    -- Defaults are a graph transaction too: filling one record can change an aliased map.
    local __selu_reverse = {}
    local __selu_candidate = __selu_saved_clone(__selu_root, {}, __selu_reverse)
    local __selu_repairs = {}
    __selu_saved_repair(__selu_candidate, __selu_defaults, __selu_schema, __selu_default_objects, "$", __selu_repairs)
    __selu_phase = "current validation"
    local __selu_valid, __selu_reason = __selu_saved_validate(__selu_candidate, __selu_schema, true, {})
    if not __selu_valid then __selu_refuse("corrupt", __selu_reason); return end
    __selu_saved_runtime.fill(__selu_candidate, __selu_defaults, __selu_default_objects)
    __selu_candidate[__selu_context_key] = nil
    __selu_candidate[__selu_version_key] = __selu_latest
    __selu_phase = "defaults validation"
    __selu_valid, __selu_reason = __selu_saved_validate(__selu_candidate, __selu_schema, false, {})
    if not __selu_valid then __selu_refuse("corrupt", __selu_reason); return end
    __selu_saved_commit(__selu_candidate, __selu_reverse, {})
    __selu_phase = "defaults"
    for __selu_index = 1, #__selu_repairs do __selu_diagnostic("repaired", __selu_repairs[__selu_index]) end
    local __selu_collision = __selu_legacy ~= nil and __selu_legacy ~= __selu_root
    __selu_phase = "publication"
    if __selu_collision then __selu_diagnostic("collision", "distinct current and legacy roots; legacy data retained") end
    __selu_publish({ status = __selu_collision and "collision" or "ready", data = __selu_root }, not __selu_collision)
  end
  __selu_saved_runtime.adoptionObjects = function(__selu_schema)
    local __selu_objects = {}
    for __selu_key, __selu_field in __selu_saved_pairs(__selu_schema.fields or {}) do
      if __selu_saved_type(__selu_field) == "table" and __selu_field.fields then
        __selu_objects[__selu_key] = __selu_saved_runtime.adoptionObjects(__selu_field)
      end
    end
    return __selu_objects
  end

  __selu_saved_runtime.pending = function()
    local __selu_result = { status = "loading", callbacks = {} }
    __selu_result.whenReady = function(__selu_callback)
      if __selu_result.data then __selu_callback(__selu_result.data)
      elseif __selu_result.status == "loading" then
        __selu_result.callbacks[#__selu_result.callbacks + 1] = __selu_callback
      end
    end
    return __selu_result
  end
  __selu_saved_runtime.publishAdoption = function(__selu_pending, __selu_result)
    __selu_pending.status, __selu_pending.data = __selu_result.status, __selu_result.data
    local __selu_callbacks = __selu_pending.callbacks
    __selu_pending.callbacks = nil
    if __selu_result.data then
      for __selu_index, __selu_callback in __selu_saved_ipairs(__selu_callbacks) do
        local __selu_ok, __selu_message = __selu_saved_pcall(__selu_callback, __selu_result.data)
        if not __selu_ok then __selu_saved_runtime.warn(__selu_saved_tostring(__selu_message)) end
      end
    end
  end

  -- Ordinary declarations stage the complete attempt on a provenance-preserving
  -- copy. Refusals never commit any changes to the client's restored graph.
  __selu_saved_runtime.ordinary = function(__selu_database, __selu_current, __selu_legacy, __selu_version_key, __selu_latest, __selu_defaults, __selu_schema, __selu_default_objects, __selu_steps, __selu_readers, __selu_contexts, __selu_publish)
    local __selu_reverse = {}
    local __selu_candidate = __selu_saved_clone(__selu_current, {}, __selu_reverse)
    __selu_saved_runtime.adopt(__selu_database, __selu_candidate, nil, __selu_version_key, __selu_latest, __selu_defaults, __selu_schema, __selu_default_objects, __selu_steps, __selu_readers, __selu_contexts, function(__selu_result)
      if __selu_result.data then
        __selu_result.data = __selu_saved_commit(__selu_result.data, __selu_reverse, {})
      end
      __selu_publish(__selu_result, false)
    end)
  end
  __selu_saved_runtime.contexts = {}
  __selu_saved_runtime.readers = {}
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
settings = __selu_saved_runtime.pending()
settings.defaults = function()
  return { enabled = false }
end
__selu_saved_runtime.entries["Addon_settings"] = {}
__selu_saved_runtime.readers["Addon_settings"] = {}
__selu_saved_runtime.contexts["Addon_settings"] = {}
__selu_saved_runtime.loaders[#__selu_saved_runtime.loaders + 1] = function()
  __selu_saved_runtime.ordinary("Addon_settings", __selu_saved_root_settings(), nil, "__selu_version", 1, { enabled = false }, { fields = { ["enabled"] = "boolean" } }, {}, __selu_saved_runtime.entries["Addon_settings"], __selu_saved_runtime.readers["Addon_settings"], __selu_saved_runtime.contexts["Addon_settings"], function(__selu_saved_result, __selu_saved_retire)
    if __selu_saved_result.data ~= nil then
      __selu_saved_publish_settings(__selu_saved_result.data)
    end
    __selu_saved_runtime.publishAdoption(settings, __selu_saved_result)
  end)
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
  settings.whenReady(function(data)
    print(data.enabled)
  end)
end)
