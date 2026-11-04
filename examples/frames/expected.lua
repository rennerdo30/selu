local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local panel = CreateFrame("Frame", nil, UIParent)
panel:SetSize(160, 60)
panel:SetPoint("CENTER", UIParent, "CENTER", 0, 0)
local texture = panel:CreateTexture()
texture:SetSize(160, 60)
local label = panel:CreateFontString()
label:SetText("Selu frames")
panel:HookScript("OnShow", function()
  print("Panel shown")
end)
panel:Hide()
__selu_on("PLAYER_LOGIN", function()
  panel:Show()
  print(panel:GetWidth(), panel:GetHeight(), texture:GetWidth(), (texture:GetHeight()))
  print((label:GetText()))
end)
