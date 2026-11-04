local __selu_addon, __selu_ns = ...
local __selu_read = __selu_ns.__selu_read
local __selu_on = __selu_ns.__selu_events
__selu_ns["main"] = {}
local panel = CreateFrame("Frame", nil, UIParent, "BackdropTemplate")
panel:SetSize(260, 100)
panel:SetPoint("CENTER", UIParent, "CENTER", 0, 0)
panel:SetBackdrop({ bgFile = "Interface/Tooltips/UI-Tooltip-Background" })
panel:SetBackdropColor(0.1, 0.2, 0.3, 1)
local label = panel:CreateFontString(nil, "OVERLAY", "GameFontNormal")
label:SetPoint("TOP", panel, "TOP", 0, -16)
label:SetText("Review grey items at a merchant.")
local button = CreateFrame("Button", nil, panel, "UIPanelButtonTemplate")
button:SetSize(120, 24)
button:SetPoint("BOTTOM", panel, "BOTTOM", 0, 12)
button:SetText("Got it")
button:HookScript("OnClick", function()
  panel:Hide()
end)
panel:Hide()
__selu_on("PLAYER_LOGIN", function()
  panel:Show()
  do
    local __selu_t1 = __selu_read((panel:GetWidth()))
    if __selu_t1 == nil then __selu_t1 = 0 end
    local __selu_t3 = __selu_t1
    local __selu_t2 = __selu_read((button:GetText()))
    if __selu_t2 == nil then __selu_t2 = "" end
    print(string.format("Merchant reminder: %spx; button: %s", __selu_t3, __selu_t2))
  end
end)
