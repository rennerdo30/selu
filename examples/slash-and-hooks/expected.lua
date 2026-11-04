local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_wow = __selu_ns.__selu_wow
__selu_ns["main"] = {}
local panel = CreateFrame("Frame")
panel:HookScript("OnShow", function(self)
  print(__selu_wow.text("Status panel ready", "Status panel ready"))
end)
hooksecurefunc("GetTime", function()
  if __selu_wow.enabled("info", "Clock checked") then
    __selu_wow.write("info", "Clock checked", "Clock checked")
  end
end)
local function showStatus(args)
  if __selu_wow.enabled("info", "Status requested: {0}") then
    __selu_wow.write("info", "Status requested: {0}", string.format("Status requested: %s", args))
  end
  print(__selu_wow.text("{0} tasks ready", "%s tasks ready", tostring(3)))
end
__selu_wow.command("/status", "/st", showStatus)
panel:Hide()
__selu_on("PLAYER_LOGIN", function()
  panel:Show()
  showStatus("login")
  print(string.format("Clock: %s", (GetTime())))
end)
