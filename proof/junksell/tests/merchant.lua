sold, bought, buyback = {}, {}, {}
repaired = 0
local items = {
 {hyperlink='item:101', itemID=101, quality=0, stackCount=2},
 {hyperlink='item:102', itemID=102, quality=0, stackCount=1},
 {hyperlink='item:103', itemID=103, quality=0, stackCount=1},
 {hyperlink='item:104', itemID=104, quality=2, stackCount=1},
}
C_Container = {
 GetContainerNumSlots=function(bag) return bag == 0 and #items or 0 end,
 GetContainerItemInfo=function(bag,slot) return items[slot] end,
 GetContainerItemQuestInfo=function(bag,slot) return {isQuestItem=slot==2, isActive=false} end,
 UseContainerItem=function(bag,slot) sold[#sold+1]=items[slot].itemID end,
}
C_Item = {
 GetItemInfoInstant=function(link) return tonumber(link:match('item:(%d+)')),nil,nil,nil,nil,0 end,
 GetItemInfo=function(link) return nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,10 end,
}
function CanMerchantRepair() return true end
function GetRepairAllCost() return 50,true end
function CanGuildBankRepair() return true end
function RepairAllItems(guild) repaired=repaired+1; guildUsed=guild end
function GetNumBuybackItems() return #buyback end
function GetBuybackItemLink(i) return buyback[i] end
function BuybackItem(i) bought[#bought+1]=i; table.remove(buyback,i) end
