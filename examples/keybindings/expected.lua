local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_wow = __selu_ns.__selu_wow
__selu_ns["main"] = {}
local function togglePanel()
  print("Panel toggled")
end
BINDING_NAME_Keybindings_TOGGLE_PANEL = __selu_wow.text("Toggle the panel", "Toggle the panel")
Keybindings_TOGGLE_PANEL_Binding = togglePanel
BINDING_HEADER_Keybindings_TOGGLE_PANEL = __selu_wow.text("Panel tools", "Panel tools")
__selu_on("PLAYER_LOGIN", function()
  togglePanel()
end)
