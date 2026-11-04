local __selu_addon, __selu_ns = ...
local __selu_read = __selu_ns.__selu_read
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
  local width = __selu_read((panel:GetWidth()))
  if width == nil then width = 0 end
  local height = __selu_read((panel:GetHeight()))
  if height == nil then height = 0 end
  local textureWidth = __selu_read((texture:GetWidth()))
  if textureWidth == nil then textureWidth = 0 end
  local textureHeight = __selu_read((texture:GetHeight()))
  if textureHeight == nil then textureHeight = 0 end
  print(width, height, textureWidth, textureHeight)
  do
    local __selu_t1 = __selu_read((label:GetText()))
    if __selu_t1 == nil then __selu_t1 = "" end
    print(__selu_t1)
  end
end)
