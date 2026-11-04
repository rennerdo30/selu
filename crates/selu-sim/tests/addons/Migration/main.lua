local addon = ...
local f = CreateFrame('Frame')
f:RegisterEvent('ADDON_LOADED')
f:RegisterEvent('UNIT_HEALTH')
f:SetScript('OnEvent', function(_, event, unit)
 if event == 'ADDON_LOADED' and unit == addon then
  MigrationDB = MigrationDB or {version=0, health=100}
  if MigrationDB.version < 1 then MigrationDB.version=1; MigrationDB.migrated=true end
  print('version', MigrationDB.version)
 elseif event == 'UNIT_HEALTH' and unit == 'player' then
  MigrationDB.health = UnitHealth(unit); print('health', MigrationDB.health)
 end
end)
