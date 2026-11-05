-- Closed persistence boundary. Unknown fields stay in the table but are never projected.
local function __selu_saved_validate(__selu_value, __selu_schema, __selu_missing, __selu_seen)
  if __selu_saved_type(__selu_schema) == "string" then
    return (__selu_missing and __selu_value == nil) or __selu_saved_type(__selu_value) == __selu_schema
  end
  if __selu_schema.optional then
    return __selu_value == nil or __selu_saved_validate(__selu_value, __selu_schema.optional, false, __selu_seen)
  end
  if __selu_missing and __selu_value == nil then return true end
  if __selu_saved_type(__selu_value) ~= "table" then return false end
  if __selu_seen[__selu_value] then return false end
  __selu_seen[__selu_value] = true
  local __selu_valid = true
  if __selu_schema.fields then
    for __selu_key, __selu_field in __selu_saved_pairs(__selu_schema.fields) do
      if not __selu_saved_validate(__selu_value[__selu_key], __selu_field, __selu_missing, __selu_seen) then __selu_valid = false end
    end
  elseif __selu_schema.array then
    local __selu_count, __selu_max = 0, 0
    for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
      if __selu_saved_type(__selu_key) ~= "number" or __selu_key < 1 or __selu_key % 1 ~= 0 then
        __selu_valid = false
      else
        __selu_count = __selu_count + 1
        if __selu_key > __selu_max then __selu_max = __selu_key end
      end
      if not __selu_saved_validate(__selu_item, __selu_schema.array, false, __selu_seen) then __selu_valid = false end
    end
    if __selu_count ~= __selu_max then __selu_valid = false end
  elseif __selu_schema.key then
    for __selu_key, __selu_item in __selu_saved_pairs(__selu_value) do
      if not __selu_saved_validate(__selu_key, __selu_schema.key, false, __selu_seen) or not __selu_saved_validate(__selu_item, __selu_schema.value, false, __selu_seen) then __selu_valid = false end
    end
  else __selu_valid = false end
  __selu_seen[__selu_value] = nil
  return __selu_valid
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
__selu_saved_runtime.adopt = function(__selu_current, __selu_legacy, __selu_version_key, __selu_latest, __selu_defaults, __selu_schema, __selu_steps, __selu_readers, __selu_contexts, __selu_publish)
  local __selu_root = __selu_current
  if __selu_root == nil then __selu_root = __selu_legacy end
  if __selu_root == nil then __selu_root = {} end
  local function __selu_refuse(__selu_status)
    __selu_publish({ status = __selu_status }, false)
  end
  if __selu_saved_type(__selu_root) ~= "table" then __selu_refuse("corrupt"); return end
  local __selu_version = __selu_root[__selu_version_key]
  if __selu_version == nil then
    local __selu_empty = true
    for __selu_key in __selu_saved_pairs(__selu_root) do __selu_empty = false; break end
    __selu_version = __selu_empty and __selu_latest or 1
  end
  if __selu_saved_type(__selu_version) ~= "number" or __selu_version < 1 or __selu_version % 1 ~= 0 then __selu_refuse("corrupt"); return end
  if __selu_version > __selu_latest then __selu_refuse("newer"); return end
  -- This is context, never a competing database version. Keep it only on failure.
  local __selu_context_key = "__selu_saved_adoption_context"
  local __selu_pending = __selu_root[__selu_context_key]
  if __selu_pending ~= nil and (__selu_saved_type(__selu_pending) ~= "table" or __selu_pending.format ~= 1 or __selu_saved_type(__selu_pending.original) ~= "table" or not __selu_saved_valid_paths(__selu_pending.removals)) then
    __selu_refuse("corrupt"); return
  end
  local __selu_original = __selu_pending and __selu_pending.original or __selu_saved_clone(__selu_root, {}, {})
  if __selu_original[__selu_context_key] ~= nil then __selu_refuse("corrupt"); return end
  local __selu_deletions = __selu_saved_clone(__selu_pending and __selu_pending.removals or {}, {}, {})
  local __selu_from = __selu_original[__selu_version_key]
  if __selu_from == nil then __selu_from = 1 end
  if __selu_saved_type(__selu_from) ~= "number" or __selu_from < 1 or __selu_from % 1 ~= 0 or __selu_from > __selu_version then __selu_refuse("corrupt"); return end
  local __selu_has_context = false
  for __selu_key in __selu_saved_pairs(__selu_contexts) do __selu_has_context = true; break end
  while __selu_version < __selu_latest do
    local __selu_reader = __selu_readers[__selu_version]
    if not __selu_reader or not __selu_saved_validate(__selu_root, __selu_reader, false, {}) then __selu_refuse("corrupt"); return end
    local __selu_context = __selu_contexts[__selu_version]
    if __selu_context and not __selu_saved_validate(__selu_original, __selu_context, false, {}) then __selu_refuse("corrupt"); return end
    local __selu_reverse = {}
    local __selu_forward = {}
    local __selu_candidate = __selu_saved_clone(__selu_root, __selu_forward, __selu_reverse)
    local __selu_ok = __selu_saved_pcall(__selu_steps[__selu_version], __selu_candidate, __selu_saved_clone(__selu_original, {}, {}))
    if not __selu_ok then __selu_refuse("failed"); return end
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
          if __selu_parent[__selu_leaf] ~= nil and not __selu_saved_equal(__selu_parent[__selu_leaf], __selu_old, {}) then __selu_refuse("failed"); return end
          __selu_parent[__selu_leaf] = nil
        end
      end
    end
    __selu_deletions = __selu_remaining
    __selu_candidate[__selu_version_key] = __selu_version + 1
    local __selu_next = __selu_readers[__selu_version + 1] or __selu_schema
    if not __selu_saved_validate(__selu_candidate, __selu_next, __selu_version + 1 == __selu_latest, {}) then __selu_refuse("corrupt"); return end
    if __selu_has_context then
      __selu_candidate[__selu_context_key] = { format = 1, original = __selu_saved_clone(__selu_original, {}, {}), removals = __selu_saved_clone(__selu_deletions, {}, {}) }
    end
    __selu_saved_commit(__selu_candidate, __selu_reverse, {})
    __selu_version = __selu_version + 1
  end
  if not __selu_saved_validate(__selu_root, __selu_schema, true, {}) then __selu_refuse("corrupt"); return end
  __selu_saved_runtime.fill(__selu_root, __selu_defaults, __selu_saved_runtime.adoptionObjects(__selu_schema))
  __selu_root[__selu_context_key] = nil
  __selu_root[__selu_version_key] = __selu_latest
  local __selu_collision = __selu_legacy ~= nil and __selu_legacy ~= __selu_root
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
