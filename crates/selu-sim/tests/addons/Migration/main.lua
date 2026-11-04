local addon = ...
local f = CreateFrame('Frame', 'MigrationPanel', UIParent)
f:SetSize(200, 80)
f:SetPoint('CENTER', UIParent, 'CENTER')
local fill = f:CreateTexture('MigrationFill', 'BACKGROUND')
fill:SetAllPoints(f)
fill:SetColorTexture(0.1, 0.2, 0.3, 1)
local label = f:CreateFontString('MigrationLabel', 'OVERLAY')
label:SetPoint('CENTER', f, 'CENTER')
local function refresh() label:SetText('health: ' .. MigrationDB.health) end
f:RegisterEvent('ADDON_LOADED')
f:RegisterEvent('UNIT_HEALTH')
f:SetScript('OnEvent', function(_, event, unit)
 if event == 'ADDON_LOADED' and unit == addon then
  MigrationDB = MigrationDB or {version=0, health=100}
  if MigrationDB.version < 1 then MigrationDB.version=1; MigrationDB.migrated=true end
  refresh(); print('version', MigrationDB.version)
 elseif event == 'UNIT_HEALTH' and unit == 'player' then
  MigrationDB.health = UnitHealth(unit); refresh(); print('health', MigrationDB.health)
 end
end)
