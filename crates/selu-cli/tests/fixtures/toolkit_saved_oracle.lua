--[[
MIT License

Copyright (c) 2026 Renner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Source: RennersAddons df793af Core.lua defaults 4-458, migrations 467-670,
ApplyDefaults 905-914, loader 1518-1555. Function bodies unchanged.
Only the addon shell/native dependencies are replaced by inert test stubs.
]]
function __selu_test_lua_master_load()
local RA = { Print = function() end, L = function(k) return k end, Log = function() end }
local self = { UnregisterEvent = function() end }
RA.defaults = {
    auraFlash = {
        enabled = true,
        threshold = 3, -- seconds of debuff remaining before we start flashing
        -- Glow for dots expiring on nearby enemies that aren't your current
        -- target. Off restores the old target-only behaviour if the extra
        -- glows turn out to be noise in practice.
        trackOtherUnits = true,
        -- Self-buffs run minutes rather than seconds, so a 3s warning would be
        -- useless for them. Capped proportionally in AuraFlash.lua so a
        -- shorter buff doesn't spend most of its life glowing.
        buffThreshold = 60,
        -- "bars": every aura you apply from a spell on your action bars.
        -- "list": only spells in `watch`.
        watchMode = "bars",
        trackDebuffs = true,
        trackBuffs = true,
        -- Self-buffs shorter than this are procs or item effects and never
        -- warn, unless you list the spell yourself.
        minBuffDuration = 30,
        -- Time an aura from your own cast when the client hides aura data.
        castTiming = true,
        showCountdown = true,
        sound = false,
        -- How hard a warning grabs attention (Modules/AuraAlert.lua):
        -- "subtle" (the original soft pulse), "strong" or "loud".
        alertStrength = "strong",
        -- Self-buff reminder card near the character: "ooc" (out of combat),
        -- "always" or "off".
        reminderCard = "ooc",
        -- Loud only: flash the screen edges when a self-buff runs out, and
        -- whether that may happen while fighting.
        screenFlash = true,
        screenFlashInCombat = false,
        -- Key into RA.AuraAlert.SOUND_KITS.
        soundChoice = "alarm",
        -- User-edited lists. Keep these defaults EMPTY: ApplyDefaults fills
        -- missing keys recursively, so any entry placed here would come back
        -- every login after the player removed it.
        -- watch[lowercase spell name] = { name, spellID, enabled, duration?, threshold? }
        watch = {},
        -- ignore[lowercase spell name] = { name, spellID }
        ignore = {},
        -- learned[spellID] = seconds, measured from readable auras
        learned = {},
        -- collapsed["<section id>"] = true | false: the Watched Spells
        -- window's section bars (UI.CollapseStore). User data, stays empty.
        collapsed = {},
    },
    minimap = {
        angle = 215,
        hide = false,
    },
    gearCompare = {
        enabled = true,
        showBags = true,
        showQuestRewards = true,
        showCrafting = true,
        showUpgrades = true,
        showDowngrades = true,
        showUnusable = true,
    },
    junkSell = {
        enabled = true,
        -- Highest item quality to sell. 0 = Poor only (the safe default),
        -- 1 = also Common. Deliberately capped below Uncommon in the UI:
        -- auto-selling greens is how people lose gear they wanted.
        maxQuality = 0,
        -- Skip anything worth more than this many copper, so a valuable grey
        -- (vendor trash worth a lot) isn't sold without you noticing.
        -- 0 disables the cap.
        maxValue = 0,
        announce = true,     -- print a summary of what was sold
        autoRepair = true,   -- repair on vendor open when affordable
        useGuildRepair = false,
    },
    levelingStats = {
        enabled = true,
    },
    -- `showHud` (boolean) became `hudVisibility` in migration 9.
    classGuide = {
        enabled = true,
        hudVisibility = "relevant", -- "always" | "relevant" | "off"
        announceTalentPoints = true,
        announceAtTrainer = true,
        -- collapsed["<section id>"] = true | false (window section bars)
        collapsed = {},
    },
    xpEta = {
        enabled = true,
        -- Per-character rolling XP samples, so the ETA isn't blank for the
        -- first half-minute after every login. Keyed "<Name>-<Realm>"; see
        -- Modules/XPBarETA.lua. Timestamps are epoch seconds, never GetTime().
        rate = {},
        -- Saved position of the standalone estimate frame, written on drag
        -- stop as { p, rp, x, y }. Empty until the user drags it once.
        fallbackPos = {},
    },
    healEta = {
        enabled = true,
        showBags = true,
        showActionBars = true,
        showTooltips = true,
    },
    -- Spell Rank Check (Modules/RankCheck.lua). ignore is user-edited, keyed
    -- by lowercased spell name -> the name as typed, and defaults empty.
    rankCheck = {
        enabled = true,
        markActionBars = true,
        announceBars = true,
        warnOnCast = true,
        ignore = {},
    },
    nameplates = {
        -- On by default. This was off because it fights Plater over the same
        -- frames, but Plater now lives in references/ and no longer loads. If
        -- it is ever re-enabled, turn this off rather than letting both skin
        -- the same plates.
        enabled = true,
        -- Tank-specific threat colors are deliberately opt-in: damage and
        -- healing characters need the normal reaction/class-colored plates,
        -- while tanks need "mine / losing / another tank / loose" at a
        -- glance.
        tankMode = false,
        questMarkers = true,
    },
    -- `showHud` (boolean) became `hudVisibility` in migration 9.
    groupQuests = {
        enabled = true,
        hudVisibility = "relevant", -- "always" | "relevant" | "off"
        nearbyYards = 300,
        -- collapsed["<section id>"] = true | false (window section bars)
        collapsed = {},
    },
    -- Modules/QuestGuide*.lua. routes/characters/captureLog/collapsed are
    -- user data and MUST stay empty here (ApplyDefaults fills nested keys
    -- every login). `showHud` (boolean) became `hudVisibility` in migration 7.
    -- autoWaypoint ("Point the arrow at the current step") drives the addon's
    -- own arrow; the native map pin is only a fallback when that is missing.
    questGuide = {
        enabled = true,
        mode = "smart",            -- "smart" | "route"
        priority = "fastest",      -- "fastest" | "zone"
        levelCutoff = 3,           -- Fastest: levels above player; 0 = same level or below
        hudVisibility = "relevant", -- "always" | "relevant" | "off"
        capture = false,           -- "Record corrections"
        autoWaypoint = true,
        lookahead = 15,
        -- World-map pins (Modules/QuestGuideMap.lua). mapShowLow /
        -- mapShowHigh also rule the Nearby list and Smart mode's pickups:
        -- trivial (grey) and very hard (red) quests stay hidden until the
        -- player turns them on. They replaced `mapLowLevel` in migration 10.
        mapPins = true,
        mapShowLow = false,
        mapShowHigh = false,
        mapNextMarks = true,
        -- Group awareness (Modules/QuestGuideGroup.lua, data from
        -- RA.GroupQuests); nothing shows outside a group. New keys, filled by
        -- ApplyDefaults: no migration.
        groupProgress = true,      -- group progress on rows, the HUD card, pin tooltips
        groupSection = true,       -- the window's "Your group" section
        groupPlan = false,         -- Smart mode: shared quests first, member pickups
        -- collapsed["<section id>"] = true | false (window section bars)
        collapsed = {},
        -- routes[i] = { id, name, schema, source, created, build, steps }
        routes = {},
        -- characters["<Name>-<Realm>"] = { route = id, overrides = { [id] = { [stepKey] = "skip" } } }
        characters = {},
        -- { version = 1, entries = { { t, q, map, x, y, lvl, time, build, obj?, npc? } } }, oldest first
        captureLog = {},
        -- Held quests first: a new pickup leads only when it is this many
        -- yards closer than the nearest held stop (0 = pure distance). New
        -- key, filled by ApplyDefaults: no migration.
        pickupPenalty = 100,
        -- Turn-ins first: ready turn-ins within this many yards (further for
        -- a cluster of several) are handed in before anything else
        -- (0 = off). New key, filled by ApplyDefaults: no migration.
        turninReach = 500,
        -- First-step hysteresis: a re-plan replaces the current first step
        -- only when another is this many yards (or 15%) better (0 = off).
        -- New key, filled by ApplyDefaults: no migration.
        stepHold = 60,
        -- Where NPCs / objects really stand, learned from their dialogs:
        -- { version = 1, npcs = { [npcID] = { m, x, y, c?, n?, w?, t } },
        -- objects = { [objectID] = ... } }, capped per kind, oldest dropped.
        -- New table (must stay empty here): no migration.
        learned = {},
        -- Character-local client taxi knowledge and observed bind point (new key).
        travel = {},
    },
    -- Modules/VersionCheck.lua. `silent` keeps the peer check running (and
    -- /rk version informed) but suppresses the chat notice.
    versionCheck = {
        enabled = true,
        silent = false,
    },
    rareAlert = {
        enabled = true,
        chat = true,
        sound = true,
        banner = true,
        portrait = true,
        focus = true,
        duration = 6,
        repeatInterval = 180,
        detectRare = true,
        detectRareElite = true,
        detectWorldBoss = true,
        positionX = 0,
        positionY = -190,
    },
    dialogKeys = {
        enabled = true,
        showBadges = true,
        -- Per window (Modules/DialogKeys.lua registry keys); false turns
        -- that window's keys off. New keys, so no migration.
        windows = {
            gossip = true, quest = true, loot = true, popups = true,
            merchant = true, trainer = true, mail = true, bank = true,
            auction = true, trade = true, stable = true, tabard = true,
            guild = true,
            professions = true, questlog = true, itemtext = true,
            macro = true, spellbook = true, character = true,
            friends = true, calendar = true,
            lootroll = true, readycheck = true, rolepoll = true,
            lfginvite = true, battlefield = true,
        },
    },
    -- Modules/Waypoint.lua. Waypoints themselves are session state, not
    -- saved; the arrow's position lives in ui.windows.waypointArrow.
    waypoint = {
        enabled = true,
        arrivalRadius = 10, -- yards
        -- Mirror the active waypoint onto Blizzard's own map pin and
        -- super-track it. Off until verified on a live client.
        mirrorBlizzard = false,
        -- The arrow card: "always" (an idle card without a waypoint),
        -- "relevant" (while a waypoint is set) or "off". Navigation stays up
        -- in combat unless hideInCombat is set (docs/DESIGN_SYSTEM.md 6).
        visibility = "relevant",
        hideInCombat = false,
    },
    cooldownText = {
        enabled = true,
        -- countdownForCooldowns is a per-character client CVar.  Keep the
        -- value we replaced per character too, so disabling after /reload can
        -- restore the right preference without affecting another character.
        characters = {},
    },
    -- Modules/SteamDeck.lua. Off until the player opts in; the component
    -- choices only take effect while `enabled` is true. `owned` is the CVar
    -- ownership ledger, owned[scope][cvarLower] = { cvar, value, restore,
    -- component }, scope "account" or "<Name>-<Realm>" for per-character
    -- CVars, so disabling restores exactly what the mode replaced.
    steamDeck = {
        enabled = false,
        applyWindowScale = true,
        windowScale = 1.25,
        applyUiScale = true,
        uiScale = 1.15,
        -- "" keeps the current graphics; "steamdeck" / "steamdeck90" reuse
        -- the Hardware Presets Deck targets. graphicsApplied is the preset
        -- this mode last applied, so it is not re-asserted every login.
        graphicsPreset = "",
        graphicsApplied = "",
        owned = {},
    },
    teamBuffs = {
        enabled = true,
        showOtherClasses = false,
        includeSolo = false,
        warnBefore = 60,
        autoOpen = true,
        sound = false,
        reminderInterval = 30,
        -- Also watch your own buffs (self families and self-scope custom
        -- spells), in a group or solo.
        watchSelf = true,
        families = {
            mark = true,
            intellect = true,
            fortitude = true,
            spirit = true,
            shout = true,
            mageArmor = true,
            innerFire = true,
            demonArmor = true,
            lightningShield = true,
            omen = true,
            aspect = true,
            trueshot = true,
        },
        -- User-edited, default empty. familyWarn: built-in key -> seconds
        -- before expiry to warn. custom: lowercased spell name ->
        -- { name, spellID, enabled, scope = "self"|"group", warnBefore }.
        familyWarn = {},
        custom = {},
        -- collapsed["<section id>"] = true | false (the Watched Buffs
        -- window's section bars). User data, stays empty.
        collapsed = {},
    },
    blizzardSkin = {
        enabled = true,
        -- One flag per Blizzard frame we restyle, so a skin that turns out to
        -- fight another addon can be dropped without losing the rest. See
        -- Modules/BlizzardSkin.lua - each entry there reads its own key here.
        gameMenu = true,
        addonList = true,
        questDialogs = true,
        lootFrame = true,
        minimap = true,
        unitFrames = true,
        objectiveTracker = true,
        merchantFrame = true,
        trainerFrame = true,
        mailFrame = true,
        -- Wave 1 windows (Modules/Skin/<Window>.lua); new keys, no migration.
        tradeFrame = true,
        itemTextFrame = true,
        taxiFrame = true,
        dressUpFrame = true,
        questPopup = true,
        inspectFrame = true,
        stableFrame = true,
        stackSplit = true,
        tabardFrame = true,
        guildRegistrar = true,
        petitionFrame = true,
        timeManager = true,
        macroFrame = true,
        -- Wave 2: HUD elements and dialogs (Modules/Skin/<Element>.lua).
        staticPopups = true,
        rollFrames = true,
        groupPrompts = true,
        castBars = true,
        auraButtons = true,
        timerBars = true,
        alerts = true,
        -- Wave 3: the big daily windows.
        characterFrame = true,
        spellbookFrame = true,
        professionsFrame = true,
        friendsFrame = true,
        groupFinder = true,
        legacyFrame = true,
        -- Wave 4: occasional windows, widgets and the native bag fallback.
        auctionHouse = true,
        communitiesFrame = true,
        collectionsJournal = true,
        calendarFrame = true,
        pvpScoreboard = true,
        chatConfig = true,
        colorPicker = true,
        deathRecap = true,
        lootHistory = true,
        uiWidgets = true,
        nativeBags = true,
        worldMap = true,
        tooltips = true,
        chat = true,
        -- Action bars, micro menu, bag bar and XP bar (Modules/ActionBarSkin.lua).
        -- End caps of the main bar: "crest" (the toolkit's crest), "gryphons"
        -- (Blizzard's own) or "none". Replaced the boolean actionBarGryphons
        -- in migration 7.
        actionBars = true,
        actionBarEndCaps = "crest",
        -- "round" keeps Blizzard's shape; "square" masks the map square and
        -- answers GetMinimapShape() == "SQUARE" for minimap-button addons.
        minimapShape = "round",
    },
    graphicsSettings = {
        enabled = true,
        -- Conservative, hardware-agnostic presets. The new solver uses only
        -- player intent and current effective render workload. An empty
        -- presetResolution means recompute the workload automatically.
        presetIntent = "stability",
        presetResolution = "",
        -- Empty mode preserves old saves: Hardware Presets resolves to
        -- Hardware Fit when a legacy hardware choice is present, otherwise
        -- Quick Presets. The first explicit mode choice is then remembered.
        presetMode = "",
        cpuTierOverride = "",
        gpuTierOverride = "",
        -- Hardware index v1 keeps evidence-backed comparison choices bound to
        -- the exact unindexed identity they were made for. Empty FPS mode is
        -- resolved as explicit so pre-v1 saved targets remain unchanged.
        hardwareIndexVersion = 1,
        cpuEstimateModel = "",
        cpuEstimateFor = "",
        gpuEstimateModel = "",
        gpuEstimateFor = "",
        targetFpsMode = "",
        -- Legacy selections are retained for SavedVariables compatibility but
        -- are also the restored Hardware Fit inputs. Empty GPU means Auto;
        -- CPU identity remains manual because WoW exposes no processor model.
        cpu = "",
        gpu = "",
        targetFps = 60,
        resolution = "",
        contentMode = "raid",
        -- Legacy self-calibration history is preserved but ignored because the
        -- samples lack a hardware/driver/resolution/render-scale fingerprint.
        calibration = { version = 1, buckets = {} },
        -- collapsed["<section id>"] = true | false (the Workbench's section
        -- bars; diagnostic sections start collapsed). User data, stays empty.
        collapsed = {},
    },
    bags = {
        -- On by default now that it claims the bag key: with the module off,
        -- `b` correctly falls through to Blizzard's bags, which looks like the
        -- takeover is broken rather than disabled.
        enabled = true,
        -- Items the user never wants sold, as { [itemID] = true }. Keyed by
        -- item rather than bag slot on purpose: slots move every time bags are
        -- sorted, and "never sell this thing" is about the item, not where it
        -- happens to sit. Account-wide, like the rest of the DB.
        sellLocked = {},
        columns = 9,
        rows = 10,
        groupByCategory = true,
        showEmpty = true,
        showItemLevel = true,
        showJunk = true,
        minQualityBorder = 2,
        showNewItems = true,
        -- The Modern Bags bank window replaces Blizzard's bank while this is on
        -- (Modules/BagsBank.lua). Added without a migration: a new key.
        useForBank = true,
    },
    gold = {
        enabled = true,
        -- Cap for the raw event/item rows. Aggregates in days/characters are
        -- never dropped, so trimming old rows changes no total.
        maxEvents = 1000,
        characters = {},
        events = {},
        items = {},
        days = {},
        -- collapsed["events:<day>" | "items:<day>"] = true | false
        collapsed = {},
    },
    ui = {
        theme = "dark", -- key into RA.UI.themes (Modules/UI.lua)
        locale = "auto", -- "auto" follows GetLocale(); or an explicit RA.L key
        hud = {
            locked = false,
            maxCards = 4,
        },
    },
    logging = {
        consoleLevel = "WARN", -- what reaches the chat frame
        storeLevel = "INFO",   -- what's kept in SavedVariables for /rk log
        entries = {},
    },
}

RA.DB_VERSION = 10

-- [version] = function(db) ... end, run in ascending order against any save
-- below that version. Append only: never edit, renumber or delete an entry,
-- because somebody's save is part-way through this chain.
-- Moves a pre-rename `debuffFlash` table onto `auraFlash`. The repair mode is
-- for saves polluted by the old loader, which applied auraFlash defaults and
-- only then stamped the save as migrated. A destination value that still
-- equals its known default cannot be distinguished from that pollution; in
-- that narrow case a non-default legacy value is the stronger evidence of a
-- real choice. Any non-default destination remains authoritative.
local function MergeDebuffFlash(db, repairDefaultPollution, legacySnapshot)
    local old = type(db.debuffFlash) == "table" and db.debuffFlash or legacySnapshot
    if type(old) ~= "table" then return end

    local new = db.auraFlash
    if type(new) ~= "table" then
        new = {}
        db.auraFlash = new
    end

    local defaults = RA.defaults.auraFlash
    for key, value in pairs(old) do
        local default = defaults[key]
        local looksDefaultFilled = repairDefaultPollution
            and default ~= nil
            and new[key] == default
            and value ~= default
        if new[key] == nil or looksDefaultFilled then
            new[key] = value
        end
    end

    db.debuffFlash = nil
end

local migrations = {
    -- The feature grew from "flash expiring debuffs on your target" to also
    -- cover your own expiring self-buffs, so `debuffFlash` became `auraFlash`
    -- (module renamed DebuffFlash.lua -> AuraFlash.lua to match). A rename is
    -- a shape change, so the saved table moves rather than being abandoned -
    -- otherwise every user silently reverts to defaults and loses their
    -- threshold.
    [2] = function(db)
        MergeDebuffFlash(db, false)
    end,

    -- Repair pass. Until this version, an unstamped save was written straight
    -- to DB_VERSION without running the chain, so every save that predated
    -- versioning skipped migration 2 and kept an orphaned `debuffFlash` while
    -- `auraFlash` silently took defaults. Those saves are already marked as
    -- version 2, so the only way to reach them is a further version. Harmless
    -- where migration 2 did run: there is no `debuffFlash` left to move.
    [3] = function(db)
        MergeDebuffFlash(db, false)
    end,

    -- Version 3 was exercised from the working tree before this repair was
    -- complete. Retry the repair for any partial/hand-restored v3 save that
    -- still has the legacy table. If the old v3 pass already deleted it, no
    -- surviving field can safely reconstruct the lost value.
    [4] = function(db, context)
        MergeDebuffFlash(db, true, context and context.debuffFlash)
    end,

    -- Cooldown Text takes temporary ownership of a per-character client CVar.
    -- Its pre-addon value must survive /reload so disabling can restore it.
    -- There was no legacy table to transform; this migration establishes and
    -- repairs the new persisted container before defaults are applied.
    [5] = function(db)
        if type(db.cooldownText) ~= "table" then db.cooldownText = {} end
        if type(db.cooldownText.characters) ~= "table" then
            db.cooldownText.characters = {}
        end
    end,

    -- Public branding changed from RennersAddons to Renner's Toolkit. The
    -- account-wide table also moved from RennersAddonsDB to RennerToolkitDB.
    -- Importing the legacy global happens before this chain because the global
    -- name lives outside the table; this version stamp records that the save
    -- completed the rebrand checkpoint without altering its contents.
    [6] = function(db)
        -- Intentionally empty: the table identity and every player value stay
        -- intact while the owning SavedVariables global changes name.
    end,

    -- Quest Guide HUD visibility became a three-way choice (Always / When
    -- relevant / Off). The old boolean `showHud` maps false -> "off" and
    -- anything else -> "relevant" (what the old card did: show only while
    -- the current step was actionable). The old key is removed so defaults
    -- cannot resurrect a second source of truth.
    [7] = function(db)
        local guide = db.questGuide
        if type(guide) ~= "table" then return end
        if guide.hudVisibility == nil then
            guide.hudVisibility = guide.showHud == false and "off" or "relevant"
        end
        guide.showHud = nil
    end,

    -- The action bar end caps became a three-way choice (the toolkit's
    -- crest, Blizzard's gryphons, none) in place of the boolean
    -- blizzardSkin.actionBarGryphons. true was an explicit "keep the
    -- gryphons" and stays exactly that. false was the default ("fade them
    -- with the bar art"), which the crest now stands in for; "none" remains
    -- one click away. A choice already written (a partial earlier run or a
    -- hand edit) is kept, so the step is idempotent.
    [8] = function(db)
        local skin = db.blizzardSkin
        if type(skin) ~= "table" then return end
        if skin.actionBarEndCaps == nil then
            skin.actionBarEndCaps = skin.actionBarGryphons == true and "gryphons" or "crest"
        end
        skin.actionBarGryphons = nil
    end,

    -- The Class Guide and Group Quests HUD cards became the same three-way
    -- choice as the Quest Guide's (migration 7): Always / When relevant /
    -- Off. The old boolean `showHud` maps false -> "off" and anything else
    -- -> "relevant" (what both cards did: show only while there was
    -- something to act on). A choice already written is kept, and the old
    -- key is removed so defaults cannot resurrect a second source of truth.
    [9] = function(db)
        for _, key in ipairs({ "classGuide", "groupQuests" }) do
            local settings = db[key]
            if type(settings) == "table" then
                if settings.hudVisibility == nil then
                    settings.hudVisibility = settings.showHud == false and "off" or "relevant"
                end
                settings.showHud = nil
            end
        end
    end,

    -- The Quest Guide's map filter `mapLowLevel` ("guide" | "show" | "hide")
    -- became two switches, `mapShowLow` and `mapShowHigh`, and low-level and
    -- very hard quests are now hidden by default. The old "show" drew both
    -- grey and too-hard pickups, so it keeps both on; "guide" and "hide"
    -- (and anything unreadable) become off. Switches already written are
    -- kept, and the old key is removed so defaults cannot resurrect a second
    -- source of truth.
    [10] = function(db)
        local guide = db.questGuide
        if type(guide) ~= "table" then return end
        local shown = guide.mapLowLevel == "show"
        if type(guide.mapShowLow) ~= "boolean" then guide.mapShowLow = shown end
        if type(guide.mapShowHigh) ~= "boolean" then guide.mapShowHigh = shown end
        guide.mapLowLevel = nil
    end,
}

local function MigrateDB(db)
    local from = db.dbVersion

    if from == nil then
        -- No version stamp means one of two things, and they need opposite
        -- handling. A genuinely empty table is a fresh install: it has the
        -- current shape by definition, so stamp it and skip the chain.
        -- Anything WITH data is a pre-versioning save - i.e. the version-1
        -- shape, and the only kind that can still be holding `debuffFlash`.
        -- Stamping those straight to DB_VERSION skipped the very migration
        -- they existed to receive, silently reverting renamed settings to
        -- defaults. They enter the chain at 1 instead.
        if next(db) == nil then
            db.dbVersion = RA.DB_VERSION
            return true
        end
        from = 1
        db.dbVersion = from
    end

    if from > RA.DB_VERSION then
        -- Addon was rolled back. Migrating "down" would mean guessing at a
        -- shape this code has never seen, so don't touch the data at all.
        RA.Print(RA.L("core.migrate.newer", from, RA.DB_VERSION))
        return false, "newer"
    end

    -- Keep migration inputs in memory while the chain runs. The append-only
    -- version 3 repair deletes debuffFlash after a conservative
    -- merge, so version 4 needs this snapshot to distinguish and repair a v2
    -- save whose auraFlash destination had first been polluted with defaults.
    -- The snapshot is never written into the SavedVariables table.
    local migrationContext = {
        debuffFlash = type(db.debuffFlash) == "table" and db.debuffFlash or nil,
    }

    for version = from + 1, RA.DB_VERSION do
        local migrate = migrations[version]
        if migrate then
            local ok, err = pcall(migrate, db, migrationContext)
            if not ok then
                -- Stop the chain instead of half-applying it. dbVersion stays
                -- at the last version that fully succeeded, so this retries on
                -- the next login once the migration is fixed.
                RA.Print(RA.L("core.migrate.failed", version, tostring(err)))
                return false, "failed"
            end
        end
        db.dbVersion = version
    end
    return true
end

local function ApplyDefaults(defaults, target)
    for key, value in pairs(defaults) do
        if type(value) == "table" then
            target[key] = target[key] or {}
            ApplyDefaults(value, target[key])
        elseif target[key] == nil then
            target[key] = value
        end
    end
end
    local currentDB = type(RennerToolkitDB) == "table" and RennerToolkitDB or nil
    local legacyDB = type(RennersAddonsDB) == "table" and RennersAddonsDB or nil
    local isFreshInstall = (currentDB == nil and legacyDB == nil)
    RennerToolkitDB = currentDB or legacyDB or {}

    local databaseReady = true
    if isFreshInstall then
        RennerToolkitDB.dbVersion = RA.DB_VERSION
    else
        -- Must run BEFORE ApplyDefaults, so migrations see the saved data
        -- exactly as it was written rather than one already back-filled with
        -- new default keys.
        databaseReady = MigrateDB(RennerToolkitDB)
    end

    if not databaseReady then
        -- In particular, never attach RA.db to a newer schema: modules write
        -- through that reference during ordinary play. Leaving it nil keeps
        -- callbacks and event handlers inert without changing the future save.
        self:UnregisterEvent("ADDON_LOADED")
        return
    end

    ApplyDefaults(RA.defaults, RennerToolkitDB)
    RA.db = RennerToolkitDB
    local svMessages = {
        client = "savedvariables: loaded by the client",
        mirror = "savedvariables: loaded through SVMirror - the client did not load them (Forever beta bug; workaround active)",
        restored = "savedvariables: the client left them empty; restored from SVMirror (Forever beta bug)",
        none = "savedvariables: none found - a fresh install, or the Forever beta did not load them and SVMirror is not set up (tools/sv_mirror.py)",
    }
    RA.Log(RA.svSource == "restored" and "WARN" or "INFO", svMessages[RA.svSource] or "savedvariables: unknown source")

    -- Retire the legacy global only after the new table is fully usable. WoW
    -- then writes RennerToolkitDB and omits the nil legacy key on logout. If a
    -- migration fails, the early return above preserves the old global so the
    -- next login can retry without data loss.
    RennersAddonsDB = nil
return RA.db
end
