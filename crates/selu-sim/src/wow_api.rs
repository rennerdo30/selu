use mlua::Lua;
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.load(
        r##"
SlashCmdList = {}
function GetLocale() return "enUS" end
function hooksecurefunc(target, name, hook)
  if type(target) == "string" then hook, name, target = name, target, _G end
  local original = target[name]
  target[name] = function(...)
    local args, count = {...}, select("#", ...)
    local function after(...) __selu_trace_hook(); hook(unpack(args, 1, count)); return ... end
    return after(original(...))
  end
end
function __selu_sim_slash(line)
  line = line:match("^%s*(.-)%s*$")
  local alias, args = line:match("^(%S+)%s*(.*)$")
  if not alias then error("unknown slash command: " .. line) end
  args = args:match("^%s*(.-)%s*$")
  for key, handler in pairs(SlashCmdList) do
    local i = 1
    while rawget(_G, "SLASH_" .. key .. i) do
      if _G["SLASH_" .. key .. i]:lower() == alias:lower() then handler(args, {}); return end
      i = i + 1
    end
  end
  error("unknown slash command: " .. line)
end
"##,
    )
    .exec()
}
