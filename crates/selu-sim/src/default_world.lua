-- Synthetic startup data only. Never reads a player account or SavedVariables.
local version,build,interface=...
if __selu_synthetic_strings then
if rawget(_G,'GENERAL')==nil then GENERAL='General' end
if rawget(_G,'COMBAT_LOG')==nil then COMBAT_LOG='Combat Log' end
if rawget(_G,'VOICE')==nil then VOICE='Voice' end
if rawget(_G,'PLAYER_LEVEL')==nil then PLAYER_LEVEL='Level %s' end
if rawget(_G,'PLAYER_LEVEL_NO_SPEC')==nil then PLAYER_LEVEL_NO_SPEC='Level %s' end
if rawget(_G,'NUM_FREE_SLOTS')==nil then NUM_FREE_SLOTS='%s free slots' end
if rawget(_G,'CHARACTER_BUTTON')==nil then CHARACTER_BUTTON='Character' end
if rawget(_G,'BACKPACK_TOOLTIP')==nil then BACKPACK_TOOLTIP='Backpack' end
if rawget(_G,'CHAT_NAME_TEMPLATE')==nil then CHAT_NAME_TEMPLATE='Chat %d' end
-- Synthetic enUS labels for the fixed raid/group tags consumed by
-- ChatFrameConstants.lua. Installed localized GlobalStrings are unavailable.
for index=1,8 do
    local name='RAID_TARGET_'..index
    if rawget(_G,name)==nil then _G[name]='Raid marker '..index end
    for alias=1,2 do
        local key='GROUP'..index..'_CHAT_TAG'..alias
        if rawget(_G,key)==nil then _G[key]='Group '..index..' alias '..alias end
    end
end
end
-- Scenario/world implementations take precedence over bootstrap stand-ins.
local standins=rawget(_G,'__selu_native_standins') or {}
local originals={}
local function remember(owner,key,name)
    if not owner then return end
    local value=rawget(owner,key)
    if value~=nil and standins[name]~=value then originals[#originals+1]={owner,key,value} end
end
for _,name in ipairs({'GetBuildInfo','Mixin','CreateFromMixins','CopyTable','GetCurrentEnvironment','GetGlobalEnvironment','issecure','GetCVar','GetCVarBool','GetCVarNumber','SetCVar',
    'UnitRace','UnitGUID','UnitIsUnit','UnitIsPlayer','UnitPlayerControlled','UnitIsConnected','UnitIsVisible','UnitSex',
    'UnitFactionGroup','HasFullControl','GetUnitName','GetSpecialization','UnitInParty','UnitInRaid','UnitPowerType',
    'GetPowerDisplayMod','GetZoneText','GetRealZoneText','GetSubZoneText','IsLoggedIn','IsPlayerInWorld',
    'securecall','securecallfunction','secureexecuterange','CreateSecureDelegate','UnitHealthPercent','UnitPowerPercent',
    'GetChatWindowInfo','GetChatWindowSavedPosition','GetChatWindowSavedDimensions','GetTasksTable','GetNumAutoQuestPopUps','GetNumShapeshiftForms'}) do remember(_G,name,name) end
for namespace,keys in pairs({C_UnitAuras={'GetAuraSlots','GetAuraDataByIndex','GetAuraDataBySlot','GetAuraDataByAuraInstanceID','GetPlayerAuraBySpellID','GetUnitAuras'},
    C_CVar={'GetCVar','GetCVarBool','GetCVarNumber','SetCVar','AreCVarsLoaded','GetCVarDefault'},
    C_EditMode={'GetLayouts'},
    C_GameRules={'GetActiveGameMode','IsStandard','IsWoWHack','IsPlunderstorm'},
    C_NamePlate={'GetNamePlateForUnit','GetNamePlates'},
    C_InputInterfaceStyle={'GetCurrentStyle'}}) do
    for _,key in ipairs(keys) do remember(rawget(_G,namespace),key,namespace..'.'..key) end
end
-- Native helpers for Blizzard's own Lua, not declarations available to Selu.
function Mixin(object,...)
    for i=1,select('#',...) do for key,value in pairs((select(i,...))) do object[key]=value end end
    return object
end
function CreateFromMixins(...) return Mixin({},...) end
function wipe(value) for key in pairs(value) do value[key]=nil end; return value end
_G.table.wipe=wipe
function CopyTable(source)
    local seen={}
    local function copy(value)
        if type(value)~='table' then return value end
        if seen[value] then return seen[value] end
        local result={}; seen[value]=result
        for key,child in pairs(value) do result[key]=copy(child) end
        local mt=getmetatable(value); if type(mt)=='table' then setmetatable(result,mt) end
        return result
    end
    return copy(source)
end
-- Native callback containers used by secure aura/event code (SharedXMLBase/
-- Event.lua and AuraContainer.lua). Empty-world aura subscriptions retain
-- callbacks but have no aura updates to emit.
C_FunctionContainers={CreateCallback=function(callback) return {callback=callback} end}
C_UnitAurasPrivate={}
local privateAuraCallbacks={}
for _,key in ipairs({'SetPrivateAuraAnchorAddedCallback','SetPrivateAuraAnchorRemovedCallback',
    'SetPrivateWarningTextFrame','SetPrivateRaidBossMessageCallback','AnchorPrivateAura'}) do
    C_UnitAurasPrivate[key]=function(...) privateAuraCallbacks[key]={...} end
end
local privateAuraUpdates={}
function C_UnitAurasPrivate.AddPrivateAuraUpdateCallback(unit,callback)
    privateAuraUpdates[unit]=privateAuraUpdates[unit] or {};privateAuraUpdates[unit][callback]=true
end
function C_UnitAurasPrivate.RemovePrivateAuraUpdateCallback(unit,callback)
    if privateAuraUpdates[unit] then privateAuraUpdates[unit][callback]=nil end
end
for _,key in ipairs({'GetPrivateAuraAnchors','GetAllPrivateAuraInstanceIDs','GetAllPrivateAuras'}) do C_UnitAurasPrivate[key]=function() return {} end end
function C_UnitAurasPrivate.GetAuraDataByAuraInstanceIDPrivate() return nil end
function C_UnitAurasPrivate.GetPrivateAuraCasterGUID() return nil end
function C_UnitAurasPrivate.IsPrivateAuraFilteredOutByInstanceID() return true end
-- Native quest task list is empty in the outdoor baseline.
function GetTasksTable() return {} end
function GetNumAutoQuestPopUps() return 0 end
function GetNumShapeshiftForms() return 0 end
function GetCursorMoney() return 0 end
function GetPlayerTradeMoney() return 0 end
function GetMerchantNumItems() return 0 end
C_CurrencyInfo=C_CurrencyInfo or {}
function C_CurrencyInfo.GetBackpackCurrencyInfo() return nil end
C_NamePlate=C_NamePlate or {}
function C_NamePlate.GetNamePlateForUnit() return nil end
function C_NamePlate.GetNamePlates() return {} end
-- Scenario API's empty-world values. The outdoor baseline has no stages.
C_Scenario=C_Scenario or {}
function C_Scenario.ShouldShowCriteria() return false end
function C_Scenario.IsInScenario() return false end
function C_Scenario.GetInfo() return nil,0,0,0,false,false,false,0,0,0 end
function C_Scenario.GetStepInfo() return nil,nil,0,nil,nil,nil,nil,0,{},0 end
function C_Scenario.GetProvingGroundsInfo() return 0,0,0,0 end
-- FrameScriptDocumentation.lua: preserve cycles and shared references.
-- Native taint propagation remains outside the simulator's fake world.
function securecopy(value,options)
    options=options or {}
    local seen={}
    local function copy(item,depth)
        if type(item)~='table' then return item end
        if rawget(item,'__kind') or rawget(item,'GetObjectType') then
            assert(options.permitScriptObjects,'securecopy: script objects are not permitted')
            return item
        end
        if seen[item] then return seen[item] end
        assert(depth<=(options.maxTraversalDepth or 100),'securecopy: maximum traversal depth exceeded')
        local result={};seen[item]=result
        for key,child in pairs(item) do result[copy(key,depth+1)]=copy(child,depth+1) end
        return result
    end
    return copy(value,0)
end
function GetCurrentEnvironment() return _G end
function GetGlobalEnvironment() return _G end
function debuglocals() return '' end
function issecure() return true end
-- Native taint is not modeled; print handlers still need this host transition.
function forceinsecure() end
if version then
    function GetBuildInfo() return version,tostring(build),'',interface end
end
local cvars={uiScale='1',useUiScale='0',scriptErrors='0',cameraFov='90',statusText='1',statusTextDisplay='BOTH',
    ActionButtonUseKeyDown='1',chatStyle='classic',chatMouseScroll='1',nameplateShowEnemies='0',
    minimapZoom='0',rotateMinimap='0',showTutorials='0',textLocale=GetLocale()}
function GetCVar(name) if cvars[name]==nil then __selu_unknown_cvar(name) end; return cvars[name] or '0' end
function GetCVarBool(name) return GetCVar(name)=='1' end
function GetCVarNumber(name) return tonumber(GetCVar(name)) or 0 end
function SetCVar(name,value) cvars[name]=tostring(value) end
C_CVar=C_CVar or {}
C_CVar.GetCVar=GetCVar; C_CVar.GetCVarBool=GetCVarBool
C_CVar.GetCVarNumber=GetCVarNumber; C_CVar.SetCVar=SetCVar
function C_CVar.AreCVarsLoaded() return true end
function C_CVar.GetCVarDefault(name) return GetCVar(name) end
function UnitRace(unit) if unit=='player' then return 'Human','Human',1 end end
function UnitEffectiveLevel(unit) return UnitLevel(unit) end
-- UnitDocumentation marks this MayReturnNothing. The fake world has no
-- alternate-power timers; a non-nil bar ID makes Blizzard enumerate forever.
function UnitPowerBarTimerInfo() return nil end
-- MirrorTimer.lua checks this native absence sentinel before creating bars.
-- The baseline player is neither drowning nor exhausted nor a ghost.
function GetMirrorTimerInfo() return 'UNKNOWN',0,0,0,0,'' end
C_ActionBar=C_ActionBar or {}
function C_ActionBar.GetActionBarPage() return 1 end
function GetActionBarPage() return 1 end
function GetNetStats() return 0,0,0,0 end
function GetNumLanguages() return 1 end
function GetLanguageByIndex(index) if index==1 then return 'Common',7 end end
Kiosk=Kiosk or {}
function Kiosk.IsEnabled() return false end
C_AdventureJournal=C_AdventureJournal or {}
function C_AdventureJournal.CanBeShown() return false end
function UnitGUID(unit) if unit=='player' then return 'Player-1-00000001' end end
function UnitIsUnit(a,b) return a==b and UnitExists(a) end
function UnitIsPlayer(unit) return unit=='player' end
function UnitPlayerControlled(unit) return UnitIsPlayer(unit) end
function UnitIsHumanPlayer(unit) return UnitIsPlayer(unit) end
-- The simulator has no 3D portrait renderer; expose that in the snapshot.
function SetPortraitTexture(texture,unit)
    local _,class=UnitClass(unit)
    local atlas=class and GetClassAtlas and GetClassAtlas(class)
    if atlas and C_Texture.GetAtlasInfo(atlas) then texture:SetAtlas(atlas,false,false,true)
    else texture:SetTexture('Interface/Icons/INV_Misc_QuestionMark') end
end

function UnitIsConnected(unit) return UnitExists(unit) end
function UnitIsVisible(unit) return UnitExists(unit) end
function UnitSex(unit) return unit=='player' and 2 or 0 end
function UnitFactionGroup(unit) if unit=='player' then return 'Alliance','Alliance' end end
function HasFullControl() return true end
function GetUnitName(unit) return UnitName(unit) end
function GetSpecialization() return nil end
function UnitInParty() return nil end
function UnitInRaid() return nil end
function UnitPowerType(unit)
    local data=__selu_world_units[unit]
    if data and data.class=='WARRIOR' then return 1,'RAGE' end
    return 0,'MANA'
end
function GetPowerDisplayMod() return 1 end
function GetZoneText() return 'Elwynn Forest' end
function GetRealZoneText() return GetZoneText() end
function GetSubZoneText() return 'Northshire Valley' end
function IsLoggedIn() return true end
function IsPlayerInWorld() return true end
-- A new synthetic profile has one visible General window. Returning nil for
-- unsaved geometry lets Blizzard's edit-mode presets choose its position.
function GetChatWindowInfo(id)
    return id==1 and 'General' or '',14,0,0,0,0.25,id==1,true,id==1,false
end
function GetChatWindowSavedPosition() return nil end
function GetChatWindowSavedDimensions() return nil end
-- No native navigation frame exists without a simulated navigation target.
-- SuperTrackedFrame.lua InitializeNavigationFrame explicitly tests for nil.
C_Navigation=C_Navigation or {}
function C_Navigation.GetFrame() return nil end
C_EditMode=C_EditMode or {}
function C_EditMode.GetAccountSettings()
    local settings={}
    for name,setting in pairs(Enum.EditModeAccountSetting or {}) do
        if type(setting)=='number' then settings[#settings+1]={setting=setting,value=name=='GridSpacing' and 32 or 0} end
    end
    return settings
end
function securecallfunction(fn,...) return __selu_secure_invoke(fn,...) end
-- FrameScriptDocumentation.lua: delegates preserve argument/return values
-- and execute in a protected context. Native taint itself remains unmodeled.
function CreateSecureDelegate(fn,options)
    assert(type(fn)=='function','secure delegate requires a function')
    return function(...) return __selu_secure_invoke(fn,...) end
end
C_GameRules=C_GameRules or {}
function C_GameRules.GetActiveGameMode()
    -- Some clients omit this enumeration from their generated documentation.
    -- Preserve the unknown result (and Enum's once-only note) rather than
    -- throwing inside the native or inventing an undocumented numeric value.
    local modes=Enum.GameMode
    return modes and modes.Standard
end
function C_GameRules.IsStandard() return true end
function C_GameRules.IsWoWHack() return false end
function C_GameRules.IsPlunderstorm() return false end
C_InputInterfaceStyle=C_InputInterfaceStyle or {}
function C_InputInterfaceStyle.GetCurrentStyle() return Enum.InputDeviceInterfaceType.Mkb end
function securecall(fn,...)
    if type(fn)=='string' then fn=_G[fn] end
    return __selu_secure_invoke(fn,...)
end
function secureexecuterange(values,callback,...)
    for key,value in pairs(values) do __selu_secure_invoke(callback,key,value,...) end
end
-- The current world model stores aura names. Empty lists must terminate
-- native pagination; a numeric typed stand-in is not an absence token.
C_UnitAuras=C_UnitAuras or {}
local function auraNames(unit,filter)
    if filter and string.find(filter,'HARMFUL',1,true) then return {} end
    local data=__selu_world_units[unit]
    return data and data.auras or {}
end
local function auraData(unit,index,filter)
    local name=auraNames(unit,filter)[index]
    if not name then return nil end
    return {name=name,auraInstanceID=index,spellId=0,icon=0,applications=1,duration=0,
        expirationTime=0,isHelpful=true,isHarmful=false,sourceUnit=unit,points={},
        isStealable=false,isBossAura=false,isFromPlayerOrPlayerPet=unit=='player'}
end
function C_UnitAuras.GetAuraSlots(unit,filter,batchSize,continuation)
    local names=auraNames(unit,filter)
    local first=(continuation or 0)+1
    local last=math.min(#names,first+math.max(1,batchSize or #names)-1)
    local slots={}
    for i=first,last do slots[#slots+1]=i end
    return last<#names and last or nil,unpack(slots)
end
C_UnitAuras.GetAuraDataByIndex=auraData
C_UnitAuras.GetAuraDataBySlot=auraData
C_UnitAuras.GetAuraDataByAuraInstanceID=auraData
function C_UnitAuras.GetPlayerAuraBySpellID() return nil end
function C_UnitAuras.GetUnitAuras(unit,filter)
    local result={}
    for i=1,#auraNames(unit,filter) do result[i]=auraData(unit,i,filter) end
    return result
end
local function percent(current,maximum,curve)
    assert(canaccessvalue(current) and canaccessvalue(maximum),'secret unit ratios are not simulated')
    local ratio=maximum>0 and current/maximum or 0
    return curve and curve:Evaluate(ratio) or ratio
end
function UnitHealthPercent(unit,predicted,curve)
    return percent(UnitHealth(unit) or 0,UnitHealthMax(unit) or 0,curve)
end
function UnitPowerPercent(unit,powerType,unmodified,curve)
    return percent(UnitPower(unit,powerType) or 0,UnitPowerMax(unit,powerType) or 0,curve)
end
-- Native Lua host extensions consumed by AuraUtil and action-bar layout.
-- LuaTableExtensionsDocumentation.lua specifies value equality across all keys.
function table.contains(values, needle)
    for _,value in pairs(values) do if value==needle then return true end end
    return false
end
_G.table.contains=table.contains
function string.join(separator,...)
    return table.concat({...},separator)
end
_G.string.join=string.join
_G.strjoin=string.join
-- SecondsFormatterAPIDocumentation.lua: native formatter configuration must
-- survive even in an empty aura world. Localized duration rendering is absent.
C_StringUtil=C_StringUtil or {}
function C_StringUtil.CreateSecondsFormatter()
    local formatter={}
    for _,key in ipairs({'ApproximationSeconds','CanRoundUpIntervals','CanRoundUpLastUnit','ConvertToLower',
        'DefaultAbbreviation','DesiredUnitCount','DesiredUnitCountCurve','MaxInterval','MaxIntervalCurve',
        'MillisecondsThreshold','MinInterval','MinIntervalCurve','Rounding','StripIntervalWhitespace'}) do
        formatter['Set'..key]=function(self,value) self[key]=value end
        formatter['Get'..key]=function(self) return self[key] end
    end
    return formatter
end
function GetNumGroupMembers() return 0 end
function GetNumSubgroupMembers() return 0 end
function GetNumPartyMembers() return 0 end
function UnitGroupRolesAssigned() return 'NONE' end
function GetChatWindowMessages(id)
    if id==1 then return 'SYSTEM','SAY','YELL','WHISPER','PARTY','GUILD','CHANNEL' end
end
-- LuaTableExtensionsDocumentation.lua: these operate over all keys.
function table.count(values) local count=0; for _ in pairs(values) do count=count+1 end; return count end
function table.isempty(values) return next(values)==nil end
function table.keys(values) local result={}; for key in pairs(values) do result[#result+1]=key end; return result end
function table.values(values) local result={}; for _,value in pairs(values) do result[#result+1]=value end; return result end
function table.indexof(values,needle) for key,value in pairs(values) do if value==needle then return key end end end
for _,name in ipairs({'count','isempty','keys','values','indexof'}) do _G.table[name]=table[name] end
-- Native numeric extensions documented by LuaMathExtensionsDocumentation.lua.
function math.clamp(value,minimum,maximum) return math.max(minimum,math.min(maximum,value)) end
function math.saturate(value) return math.clamp(value,0,1) end
function math.normalize(value,minimum,maximum) return (value-minimum)/(maximum-minimum) end
function math.lerp(first,last,amount) return first+(last-first)*amount end
function math.round(value,places)
    local factor=10^(places or 0)
    local scaled=value*factor
    return (scaled<0 and math.ceil(scaled-.5) or math.floor(scaled+.5))/factor
end
for _,name in ipairs({'clamp','saturate','normalize','lerp','round'}) do _G.math[name]=math[name] end
C_ColorUtil=C_ColorUtil or {}
function C_ColorUtil.GenerateTextColorCode(color)
    local function byte(value) return math.floor(math.saturate(value)*255+.5) end
    return string.format('ff%02x%02x%02x',byte(color.r),byte(color.g),byte(color.b))
end
function C_ColorUtil.WrapTextInColorCode(text,code) return '|c'..code..text..'|r' end
function C_ColorUtil.WrapTextInColor(text,color) return C_ColorUtil.WrapTextInColorCode(text,C_ColorUtil.GenerateTextColorCode(color)) end
for _,entry in ipairs(originals) do entry[1][entry[2]]=entry[3] end
if not rawget(_G,'WorldFrame') then
    local world=CreateFrame('Frame','WorldFrame',UIParent)
    world:SetAllPoints(UIParent); world:SetFrameStrata('BACKGROUND')
end
