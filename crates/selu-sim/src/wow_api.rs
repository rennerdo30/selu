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
    local function after(...) hook(unpack(args, 1, count)); return ... end
    return after(original(...))
  end
end
function __selu_sim_slash(line)
  local alias, args = line:match("^(%S+)%s*(.*)$")
  for key, handler in pairs(SlashCmdList) do
    local i = 1
    while rawget(_G, "SLASH_" .. key .. i) do
      if _G["SLASH_" .. key .. i]:lower() == alias:lower() then handler(args); return end
      i = i + 1
    end
  end
  error("unknown slash command: " .. line)
end
"##,
    )
    .exec()
}
