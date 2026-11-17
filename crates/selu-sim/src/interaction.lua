-- Native NPC interaction state: a gossip conversation, the quest dialog
-- (greeting, detail, progress, reward) and a loot window, set by the host
-- (Session::interact, `sim.interact`). No frame or template knowledge:
-- Blizzard's own frames answer the events and read these APIs.
--
-- What the player chose (an option, a quest, accept, a reward, a loot slot)
-- is recorded in order, as the server would receive it
-- (`sim.interactions()`). The host decides what the server answers next.
local state = { kind = nil, data = {} }
local log = {}
__selu_world_interaction_log = log

local function record(entry) log[#log + 1] = entry end
local function data() return state.data or {} end
local function list(key)
    local value = data()[key]
    if type(value) == 'table' then return value end
    return {}
end
local function fire(event, ...) __selu_world_event(event, ...) end
local function open(kind) return state.kind == kind end
local function enumValue(enum, name, fallback)
    local values = Enum and Enum[enum]
    local value = values and values[name]
    if value ~= nil then return value end
    return fallback
end

---------------------------------------------------------------- gossip
-- GossipDocumentation.lua: options carry their own ID and order; the quest
-- lists carry quest IDs.
C_GossipInfo = rawget(_G, 'C_GossipInfo') or {}
local function gossipOpen() return open('gossip') end
function C_GossipInfo.GetText() return gossipOpen() and (data().text or '') or '' end
function C_GossipInfo.GetOptions()
    local out = {}
    if not gossipOpen() then return out end
    for index, option in ipairs(list('options')) do
        out[#out + 1] = {
            gossipOptionID = option.id or index, name = option.name or '', icon = option.icon or 132053,
            orderIndex = option.orderIndex or (index - 1), flags = 0, status = 0, rewards = {},
            selectOptionWhenOnlyOption = false,
        }
    end
    return out
end
local function quests(key)
    local out = {}
    if not gossipOpen() then return out end
    for _, quest in ipairs(list(key)) do
        out[#out + 1] = {
            questID = quest.questID or 0, title = quest.title or '', questLevel = quest.level or 1,
            isTrivial = quest.trivial == true, frequency = 0, repeatable = false,
            isComplete = quest.complete == true, isLegendary = false, isIgnored = false,
            isImportant = false, isMeta = false,
        }
    end
    return out
end
function C_GossipInfo.GetAvailableQuests() return quests('available') end
function C_GossipInfo.GetActiveQuests() return quests('active') end
function C_GossipInfo.GetNumOptions() return #C_GossipInfo.GetOptions() end
function C_GossipInfo.GetNumAvailableQuests() return #quests('available') end
function C_GossipInfo.GetNumActiveQuests() return #quests('active') end
function C_GossipInfo.SelectOption(id) record('SelectOption ' .. tostring(id)) end
function C_GossipInfo.SelectOptionByIndex(index) record('SelectOptionByIndex ' .. tostring(index)) end
function C_GossipInfo.SelectAvailableQuest(questID) record('SelectAvailableQuest ' .. tostring(questID)) end
function C_GossipInfo.SelectActiveQuest(questID) record('SelectActiveQuest ' .. tostring(questID)) end
function C_GossipInfo.GetFriendshipReputation() return { friendshipFactionID = 0 } end
function C_GossipInfo.CloseGossip()
    record('CloseGossip')
    if gossipOpen() then
        state.kind, state.data = nil, {}
        fire('GOSSIP_CLOSED', false)
    end
end

---------------------------------------------------------------- quest dialog
local function questOpen()
    return open('questGreeting') or open('questDetail') or open('questProgress') or open('questComplete')
end
local function section(key) return questOpen() and list(key) or {} end
function GetGreetingText() return open('questGreeting') and (data().text or '') or '' end
function GetNumActiveQuests() return #section('active') end
function GetNumAvailableQuests() return #section('available') end
function GetActiveTitle(index)
    local quest = section('active')[index]
    if quest then return quest.title or '', quest.complete == true end
end
function GetActiveQuestID(index)
    local quest = section('active')[index]
    return quest and quest.questID or 0
end
function IsActiveQuestTrivial(index)
    local quest = section('active')[index]
    return quest ~= nil and quest.trivial == true
end
function GetAvailableTitle(index)
    local quest = section('available')[index]
    return quest and quest.title or nil
end
function GetAvailableQuestInfo(index)
    local quest = section('available')[index]
    if not quest then return end
    -- isTrivial, frequency, isRepeatable, isLegendary, questID, isImportant, isMeta, questInfoID
    return quest.trivial == true, 0, false, false, quest.questID or 0, false, false, nil
end
function SelectActiveQuest(index) record('SelectActiveQuest ' .. tostring(index)) end
function SelectAvailableQuest(index) record('SelectAvailableQuest ' .. tostring(index)) end
function GetTitleText() return questOpen() and (data().title or '') or '' end
function GetQuestText() return questOpen() and (data().text or '') or '' end
function GetObjectiveText() return questOpen() and (data().objectives or '') or '' end
function GetProgressText() return open('questProgress') and (data().text or '') or '' end
function GetRewardText() return open('questComplete') and (data().text or '') or '' end
function GetQuestID() return questOpen() and (data().questID or 0) or 0 end
function IsQuestCompletable() return open('questProgress') and data().completable == true end
function QuestGetAutoAccept() return false end
-- No NPC portrait beside the dialog (displayID 0), as for most quest givers:
-- portrait, text, name, mount, model scene.
function GetQuestPortraitGiver() return 0, '', '', 0, 0 end
function GetQuestPortraitTurnIn() return 0, '', '', 0, 0 end
-- The dialog's background material; nil gives Blizzard's default parchment.
function GetQuestBackgroundMaterial() return nil end
function QuestIsFromAdventureMap() return false end
function GetCriteriaSpell() return nil end
function QuestIsFromAreaTrigger() return false end
function QuestFlagsPVP() return false end
function GetQuestItemRequired() return nil end
local function items(kind)
    if kind == 'required' then return open('questProgress') and list('required') or {} end
    if kind == 'choice' then return questOpen() and list('choices') or {} end
    if kind == 'reward' then return questOpen() and list('rewards') or {} end
    return {}
end
function GetNumQuestItems() return #items('required') end
-- Currencies and money a quest asks for or gives: none unless the host says.
function GetNumQuestCurrencies() return 0 end
function GetQuestMoneyToGet() return open('questProgress') and (data().moneyRequired or 0) or 0 end
function GetQuestCurrencyInfo() return nil end
function GetQuestCurrencyID() return nil end
function GetNumQuestChoices() return #items('choice') end
function GetNumQuestRewards() return #items('reward') end
function GetQuestItemInfo(kind, index)
    local item = items(kind)[index]
    if not item then return end
    -- name, texture, count, quality, isUsable, itemID
    return item.name or ('Simulated item ' .. tostring(item.itemID or 0)), item.icon or 134400,
        item.count or 1, item.quality or 1, true, item.itemID or 0
end
function GetQuestItemLink(kind, index)
    local item = items(kind)[index]
    if item and item.itemID then return '|Hitem:' .. item.itemID .. ':0|h[' .. (item.name or 'item') .. ']|h' end
end
-- 0 is LOOT_LIST_ITEM, 1 LOOT_LIST_CURRENCY (QuestInfo.lua); the host's
-- rewards are items.
function GetQuestItemInfoLootType(kind, index)
    if items(kind)[index] then return 0 end
end
function GetRewardMoney() return questOpen() and (data().money or 0) or 0 end
function GetRewardXP() return questOpen() and (data().xp or 0) or 0 end
function GetRewardHonor() return 0 end
function GetRewardTitle() return nil end
function GetNumRewardCurrencies() return 0 end
function GetNumRewardSpells() return 0 end
function GetRewardSkillPoints() return nil end
function GetRewardArtifactXP() return 0 end
function GetSuggestedGroupSize() return 0 end
local function finish(entry)
    record(entry)
    if questOpen() then
        state.kind, state.data = nil, {}
        fire('QUEST_FINISHED')
    end
end
function AcceptQuest() record('AcceptQuest') end
function DeclineQuest() finish('DeclineQuest') end
function CompleteQuest() record('CompleteQuest') end
function GetQuestReward(choice) finish('GetQuestReward ' .. tostring(choice or 0)) end
function CloseQuest() finish('CloseQuest') end

---------------------------------------------------------------- loot
-- LootDocumentation: slots by index; a looted slot is empty (LootSlotType None).
local function lootOpen() return open('loot') end
local function slot(index) return lootOpen() and list('slots')[index] or nil end
function GetNumLootItems() return lootOpen() and #list('slots') or 0 end
function GetLootSlotType(index)
    local entry = slot(index)
    if not entry or entry.looted then return enumValue('LootSlotType', 'None', 0) end
    if entry.money then return enumValue('LootSlotType', 'Money', 2) end
    return enumValue('LootSlotType', 'Item', 1)
end
function GetLootSlotInfo(index)
    local entry = slot(index)
    if not entry or entry.looted then return end
    -- texture, item, quantity, currencyID, quality, locked, isQuestItem, questID, isActive, isCoin
    return entry.icon or 134400, entry.name or ('Simulated item ' .. tostring(entry.itemID or 0)), entry.count or 1,
        nil, entry.quality or 1, false, entry.quest == true, nil, false, entry.money == true
end
function GetLootSlotLink(index)
    local entry = slot(index)
    if entry and not entry.looted and entry.itemID then
        return '|Hitem:' .. entry.itemID .. ':0|h[' .. (entry.name or 'item') .. ']|h'
    end
end
function LootSlotHasItem(index)
    local entry = slot(index)
    return entry ~= nil and not entry.looted
end
function IsFishingLoot() return false end
function LootSlot(index)
    record('LootSlot ' .. tostring(index))
    local entry = slot(index)
    if not entry or entry.looted then return end
    entry.looted = true
    fire('LOOT_SLOT_CLEARED', index)
    for _, other in ipairs(list('slots')) do
        if not other.looted then return end
    end
    state.kind, state.data = nil, {}
    fire('LOOT_CLOSED')
end
function CloseLoot()
    record('CloseLoot')
    if lootOpen() then
        state.kind, state.data = nil, {}
        fire('LOOT_CLOSED')
    end
end

---------------------------------------------------------------- the host
local SHOW = {
    gossip = { 'GOSSIP_SHOW', '' },
    questGreeting = { 'QUEST_GREETING' },
    questDetail = { 'QUEST_DETAIL' },
    questProgress = { 'QUEST_PROGRESS' },
    questComplete = { 'QUEST_COMPLETE' },
    loot = { 'LOOT_OPENED', false, false },
}
local CLOSE = {
    gossip = { 'GOSSIP_CLOSED', false },
    questGreeting = { 'QUEST_FINISHED' }, questDetail = { 'QUEST_FINISHED' },
    questProgress = { 'QUEST_FINISHED' }, questComplete = { 'QUEST_FINISHED' },
    loot = { 'LOOT_CLOSED' },
}

-- `close` ends the open interaction as the server would; any other kind
-- opens it with its data and fires the event that shows it.
function __selu_interact(kind, value)
    if kind == 'close' then
        local closing = CLOSE[state.kind or '']
        state.kind, state.data = nil, {}
        if closing then fire(unpack(closing)) end
        return
    end
    local show = SHOW[kind]
    if not show then error('sim.interact: unknown kind ' .. tostring(kind), 2) end
    state.kind = kind
    state.data = type(value) == 'table' and value or {}
    -- The items this interaction names exist in the client's item data.
    for _, key in ipairs({ 'choices', 'rewards', 'required', 'slots' }) do
        for _, item in ipairs(list(key)) do
            if type(item) == 'table' and type(item.itemID) == 'number' and item.itemID > 0 then
                __selu_world_known_items[item.itemID] = true
            end
        end
    end
    fire(unpack(show))
end
