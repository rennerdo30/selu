-- Native world state the host sets through world calls (Session::world_call,
-- `__selu_host_<name>`): temporary weapon enchants and addon messages. No
-- frame or template knowledge.

---------------------------------------------------------------- weapon enchants
-- ItemDocumentation.lua: C_Item.GetWeaponEnchantInfo(weaponSlot) returns the
-- slot's enchants as WeaponEnchantInfo; timeLeft is in milliseconds and counts
-- down with the client clock. Temporary enchants (imbues, poisons, oils) only.
local enchants = {}
local function enumValue(enum, name, fallback)
    local values = Enum and Enum[enum]
    local value = values and values[name]
    if value ~= nil then return value end
    return fallback
end
local function remaining(slot)
    local enchant = enchants[slot]
    if not enchant then return nil end
    local left = enchant.expires - GetTime()
    if left <= 0 then return nil end
    return left
end
C_Item = rawget(_G, 'C_Item') or {}
function C_Item.GetWeaponEnchantInfo(slot)
    local enchant = enchants[slot]
    local left = remaining(slot)
    if not enchant or not left then
        return { { hasEnchant = false, enchantType = enumValue('ItemEnchantType', 'Temporary', 1), timeLeft = 0, charges = 0, enchantID = 0, enchantIconID = 0 } }
    end
    return { {
        hasEnchant = true, enchantType = enumValue('ItemEnchantType', 'Temporary', 1),
        timeLeft = math.floor(left * 1000), charges = enchant.charges or 0,
        enchantID = enchant.enchantID or 0, enchantIconID = enchant.icon or 0,
    } }
end
-- The legacy reading: main hand, off hand, ranged, each (has, ms left,
-- charges, enchant ID).
function GetWeaponEnchantInfo()
    local out = {}
    for _, slot in ipairs({ 0, 1, 2 }) do
        local enchant, left = enchants[slot], remaining(slot)
        if enchant and left then
            out[#out + 1], out[#out + 2], out[#out + 3], out[#out + 4] = true, math.floor(left * 1000), enchant.charges or 0, enchant.enchantID or 0
        else
            out[#out + 1], out[#out + 2], out[#out + 3], out[#out + 4] = false, 0, 0, 0
        end
    end
    return unpack(out, 1, 12)
end
-- slot: Enum.WeaponSlot (0 main hand, 1 off hand, 2 ranged); info nil
-- removes the enchant. Fires what the client fires when it changes.
function __selu_host_setWeaponEnchant(slot, info)
    if type(info) == 'table' then
        enchants[slot] = {
            enchantID = info.enchantID or 0, charges = info.charges or 0, icon = info.icon or 0,
            expires = GetTime() + (info.seconds or 1800),
        }
    else
        enchants[slot] = nil
    end
    __selu_world_event('WEAPON_ENCHANT_CHANGED')
    __selu_world_event('UNIT_INVENTORY_CHANGED', 'player')
end

---------------------------------------------------------------- addon messages
-- ChatInfoDocumentation.lua: a client receives CHAT_MSG_ADDON only for a
-- prefix it registered. What the addon sends is recorded in order.
C_ChatInfo = rawget(_G, 'C_ChatInfo') or {}
local prefixes = {}
local sent = {}
__selu_world_addon_messages = sent
function C_ChatInfo.RegisterAddonMessagePrefix(prefix)
    if type(prefix) ~= 'string' or prefix == '' or #prefix > 16 then return false end
    prefixes[prefix] = true
    return true
end
function C_ChatInfo.IsAddonMessagePrefixRegistered(prefix) return prefixes[prefix] == true end
function C_ChatInfo.GetRegisteredAddonMessagePrefixes()
    local out = {}
    for prefix in pairs(prefixes) do out[#out + 1] = prefix end
    table.sort(out)
    return out
end
local function send(prefix, message, chatType, target)
    sent[#sent + 1] = table.concat({ tostring(prefix), tostring(chatType or 'PARTY'), tostring(target or ''), tostring(message) }, '|')
    return enumValue('SendAddonMessageResult', 'Success', 0)
end
C_ChatInfo.SendAddonMessage = send
C_ChatInfo.SendAddonMessageLogged = send
-- Another player's message arriving: only a registered prefix is delivered.
function __selu_host_receiveAddonMessage(prefix, text, channel, sender)
    if not prefixes[prefix] then return end
    __selu_world_event('CHAT_MSG_ADDON', prefix, text, channel or 'PARTY', sender or 'Friend-Realm',
        UnitName and UnitName('player') or '', 0, 0, '', 0)
end
