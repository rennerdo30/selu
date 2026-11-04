-- A plain Lua addon for trying Selu Studio: a panel with a title, a
-- coloured bar that grows over time, and a counter driven by a ticker.
local addonName = ...

local panel = CreateFrame("Frame", "StudioDemoPanel", UIParent)
panel:SetSize(360, 180)
panel:SetPoint("CENTER", UIParent, "CENTER", 0, 120)

local background = panel:CreateTexture(nil, "BACKGROUND")
background:SetAllPoints()
background:SetColorTexture(0.05, 0.07, 0.12, 0.9)

local header = panel:CreateTexture(nil, "ARTWORK")
header:SetPoint("TOPLEFT", panel, "TOPLEFT", 0, 0)
header:SetPoint("TOPRIGHT", panel, "TOPRIGHT", 0, 0)
header:SetSize(360, 28)
header:SetColorTexture(0.2, 0.35, 0.7, 1)

local title = panel:CreateFontString(nil, "OVERLAY")
title:SetPoint("TOPLEFT", panel, "TOPLEFT", 10, -8)
title:SetText("Studio Demo")

local track = panel:CreateTexture(nil, "ARTWORK")
track:SetPoint("BOTTOMLEFT", panel, "BOTTOMLEFT", 20, 30)
track:SetSize(320, 16)
track:SetColorTexture(0.15, 0.15, 0.15, 1)

local bar = panel:CreateTexture(nil, "OVERLAY")
bar:SetPoint("BOTTOMLEFT", track, "BOTTOMLEFT", 0, 0)
bar:SetSize(1, 16)
bar:SetColorTexture(0.2, 0.8, 0.3, 1)

local counter = panel:CreateFontString(nil, "OVERLAY")
counter:SetPoint("LEFT", panel, "LEFT", 20, 10)
counter:SetText("ticks: 0")

local icon = panel:CreateTexture(nil, "ARTWORK")
icon:SetSize(48, 48)
icon:SetPoint("RIGHT", panel, "RIGHT", -20, 10)
icon:SetTexture("Interface\\Icons\\INV_Misc_QuestionMark")

local ticks = 0
local events = CreateFrame("Frame")
events:RegisterEvent("PLAYER_LOGIN")
events:SetScript("OnEvent", function(self, event)
  print(addonName .. ": " .. event)
  C_Timer.NewTicker(0.5, function()
    ticks = ticks + 1
    counter:SetText("ticks: " .. ticks)
    bar:SetSize(math.min(320, ticks * 8), 16)
  end)
end)

SLASH_STUDIODEMO1 = "/demo"
SlashCmdList.STUDIODEMO = function(msg)
  if msg == "hide" then panel:Hide() else panel:Show() end
  print("demo panel " .. (panel:IsShown() and "shown" or "hidden"))
end
