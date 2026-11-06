-- Synthetic startup data only. Never reads a player account or SavedVariables.
local version,build,interface=...
if version then
    function GetBuildInfo() return version,tostring(build),'',interface end
end
local cvars={uiScale='1',useUiScale='0',scriptErrors='1',cameraFov='90',
    ActionButtonUseKeyDown='1',chatStyle='classic',nameplateShowEnemies='0',
    minimapZoom='0',rotateMinimap='0',showTutorials='0'}
function GetCVar(name) return cvars[name] or '0' end
function GetCVarBool(name) return GetCVar(name)=='1' end
function GetCVarNumber(name) return tonumber(GetCVar(name)) or 0 end
function SetCVar(name,value) cvars[name]=tostring(value) end
C_CVar=C_CVar or {}
C_CVar.GetCVar=GetCVar; C_CVar.GetCVarBool=GetCVarBool
C_CVar.GetCVarNumber=GetCVarNumber; C_CVar.SetCVar=SetCVar
function UnitRace(unit) if unit=='player' then return 'Human','Human',1 end end
function GetZoneText() return 'Elwynn Forest' end
function GetRealZoneText() return GetZoneText() end
function GetSubZoneText() return 'Northshire Valley' end
function IsLoggedIn() return true end
function IsPlayerInWorld() return true end
