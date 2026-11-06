-- Native container state, shared with Scenario. No frame/template knowledge.
-- ContainerDocumentation.lua: missing slots return no item info. Scenario
-- stores item IDs only: counts/quality are synthetic and art is a generic
-- client question mark until item metadata is available.
local function item(bag,slot)
    local id=(__selu_world_bags[bag] or {})[slot]
    if id and id>0 then return id end
end
function C_Container.GetContainerItemInfo(bag,slot)
    local id=item(bag,slot);if not id then return nil end
    return {itemID=id,iconFileID=134400,stackCount=1,isLocked=false,quality=1,isReadable=false,
        hyperlink='item:'..id,isFiltered=false,hasNoValue=false,isBound=false,hasLoot=false,itemName='Simulated item '..id}
end
function C_Container.GetContainerItemID(bag,slot) return item(bag,slot) end
function C_Container.GetContainerItemLink(bag,slot)
    local id=item(bag,slot);return id and 'item:'..id or nil
end
function C_Container.HasContainerItem(bag,slot) return item(bag,slot)~=nil end
function C_Container.GetContainerNumFreeSlots(bag)
    local slots=__selu_world_bags[bag] or {};local free=0
    for _,id in ipairs(slots) do if id==0 then free=free+1 end end
    return free,0
end
function C_Container.GetContainerItemQuestInfo() return {isQuestItem=false,isActive=false} end
function C_Container.GetContainerItemCooldown() return 0,0,0 end
function C_Container.GetBagName(bag) return 'Simulated bag '..bag end
C_Item=rawget(_G,'C_Item') or {}
local function locationItem(location)
    if type(location)=='table' and location.GetBagAndSlot then return item(location:GetBagAndSlot()) end
end
function C_Item.GetItemID(location) return locationItem(location) end
function C_Item.DoesItemExist(location) return locationItem(location)~=nil end
function C_Item.GetItemIcon(location) if locationItem(location) then return 134400 end end
function C_Item.GetItemQuality(location) if locationItem(location) then return 1 end end
function C_Item.GetItemName(location) local id=locationItem(location);return id and 'Simulated item '..id end
local cached={}
local function known(value)
    local id=tonumber(value) or (type(value)=='string' and tonumber(string.match(value,'item:(%d+)')))
    if not id or id<=0 then return nil end
    if cached[id] then return id end
    for _,slots in pairs(__selu_world_bags) do for _,entry in ipairs(slots) do if entry==id then cached[id]=true;return id end end end
end
function C_Item.IsItemDataCachedByID(value) return known(value)~=nil end
function C_Item.IsItemDataCached(location) return locationItem(location)~=nil end
function C_Item.GetItemInfo(value)
    local id=known(value);if not id then return nil end
    return 'Simulated item '..id,'item:'..id,1,1,1,'Miscellaneous','Junk',1,'',134400,0,15,0,1,0,0,false
end
function C_Item.GetItemInfoInstant(value)
    local id=known(value);if id then return id,'Miscellaneous','Junk','',134400,15,0 end
end
function C_Item.GetItemIconByID(value) if known(value) then return 134400 end end
function C_Item.RequestLoadItemDataByID(value)
    local id=tonumber(value) or tonumber(string.match(value,'item:(%d+)'))
    -- Cached synthetic records complete immediately, through the same event
    -- subscribers as host events. Unknown records complete with failure.
    __selu_item_data_result(id,known(value)~=nil)
end
