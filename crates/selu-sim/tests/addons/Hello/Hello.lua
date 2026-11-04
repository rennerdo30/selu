-- A plain Lua addon, written the way a real one is: no Selu output.
local addonName, ns = ...

local frame = CreateFrame("Frame")
frame:RegisterEvent("ADDON_LOADED")
frame:RegisterEvent("PLAYER_LOGIN")
frame:RegisterEvent("PLAYER_ENTERING_WORLD")
frame:SetScript("OnEvent", function(self, event, ...)
  if event == "ADDON_LOADED" then
    local loaded = ...
    if loaded == addonName then
      print(addonName .. " loaded")
      self:UnregisterEvent("ADDON_LOADED")
    end
  elseif event == "PLAYER_LOGIN" then
    print(addonName .. ": login")
  elseif event == "PLAYER_ENTERING_WORLD" then
    local isInitialLogin = ...
    print(addonName .. ": entering world " .. tostring(isInitialLogin))
  end
end)
ns.frame = frame
