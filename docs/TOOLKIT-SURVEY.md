# What Renner's Toolkit uses (survey, 2026-11-04)

Goal (owner, 2026-11-04): Renner's Toolkit (`E:/Development/RennersAddons`)
should in the end be written in Selu. This survey lists what the addon
uses today, so Selu supports that first. Source: every `.lua` file the
`RennersAddons.toc` loads (111 files, about 78,500 lines, without tests and
the QuestData addon), counted by a script on master `df793af`; comments and
strings were removed before counting. The counts are approximate: they find
names in the source text and don't resolve them.

## Targets

- One `.toc` declares seven interfaces: `11509` (Classic Era), `16001`
  (WoW: Forever), `20506`, `38002`, `50504` (classic branches), `120007` and
  `120100` (retail 12.0/12.1). The same Lua runs on all of them.
- The code spends much of its size on telling the clients apart: about 1,470
  `pcall`s, about 2,560 `type(...)` checks, and fallbacks such as
  `C_Container.GetContainerNumSlots` or else `GetContainerNumSlots`.
  **This is the main thing Selu can remove**: manifests per target, `has(...)`
  and the strictest-target rule replace the guessing, and the compiler says
  where a call is missing on a target.
- SPEC-V1 has only `retail` and `forever` in v1 and classic-era/classic
  later. Porting the whole addon needs the classic flavours too (see "Open
  for the owner" below).
- `docs/WOW_LUA_API.md` in the toolkit records verified client facts (for
  example "Forever needs interface `16001`"). It is the first source for
  manifest overlays.

## Lua patterns the language must cover

| Pattern | Count | Selu feature |
|---|---|---|
| `function(...)` literals (closures, callbacks, scripts) | ~1,630 | arrow functions, function expressions |
| `#t` length | ~1,420 | `xs.length` |
| `ipairs` / `pairs` | ~1,130 / ~290 | `for...of` over arrays and maps |
| `pcall` (mostly feature guarding, some real error handling) | ~1,470 | `has(...)`, `try`/`catch`, multiple returns |
| string `..` | ~940 | templates, `+` on strings |
| `...` varargs | ~340 | rest parameters, multiple returns |
| numeric `for` | ~260 | counted `for` |
| `_G[name]` (Blizzard frames by name) | ~130 | typed named frames from the manifest; `unknown` for dynamic names |
| `setmetatable` | ~60 | `class` |
| weak tables (`__mode`) | ~55 | needs a decision: a `WeakMap`-style type or an annotation |
| module tables `RA.X = {}`, `function M.F()` | everywhere | modules, `export` |
| string methods `match`, `gsub`, `lower`, `format`, `find`, `sub`, `gmatch` | ~350 | the string library (SPEC 2.3); Lua patterns, not regex |
| `math.max/floor/min/abs/huge/ceil` | ~540 | `Math` |
| `table.insert/concat/sort/remove` | ~290 | array methods |
| `wipe` | ~85 | `xs.clear()` / `map.clear()` |

No `coroutine` and no `goto` are used.

## WoW features

- **Frames:** `CreateFrame` ~190 times, of types `Frame` 156, `Button` 24,
  `GameTooltip` 3, `ScrollFrame` 2, `EditBox` 2, `Slider` 1; templates
  `BackdropTemplate`, `GameTooltipTemplate`, `UIPanelButtonTemplate`.
- **Widget methods**, most used first: `SetPoint` 658, `Hide` 358,
  `SetText` 205, `SetScript` 202, `Show` 188, `SetSize` 182,
  `ClearAllPoints` 166, `CreateTexture` 156, `SetWidth` 127, `SetHeight` 113,
  `RegisterEvent` 97, `IsShown` 95, `GetWidth` 92, `GetHeight` 80,
  `SetShown` 73, `SetAllPoints` 62, `HookScript` 49, `SetFrameLevel` 45,
  `SetEnabled` 41, `SetAlpha` 36, `GetText` 32, `EnableMouse` 30,
  `SetTexture` 28, `SetColorTexture` 21, `SetAttribute` 17, `AddLine` 17,
  then animations (`CreateAnimationGroup`, `SetDuration`, `SetFromAlpha`),
  dragging (`StartMoving`, `RegisterForDrag`), scroll frames, edit boxes.
  About 260 distinct methods in total. The manifest needs widget types with
  their methods; the typed checker needs them to check `frame.X(...)`.
- **Scripts:** `OnEvent` 41, `OnEnter`/`OnLeave` 32, `OnClick` 24, `OnShow`
  19, `OnUpdate` 18, `OnHide` 16, drag and mouse scripts, edit box scripts.
- **Events:** 47 distinct. Most used: `PLAYER_REGEN_ENABLED` 12 (work queued
  until combat ends), `ADDON_LOADED` 11, `PLAYER_ENTERING_WORLD` 11,
  `PLAYER_LOGIN` 5, `PLAYER_REGEN_DISABLED` 4, then bags, merchant,
  nameplates, target, quests, action bars, group roster.
- **Hooks:** `hooksecurefunc` on 27 Blizzard functions and mixin methods
  (`ContainerFrameItemButton_Update`, `QuestInfo_Display`,
  `CompactUnitFrame_UpdateHealthColor`...), `HookScript` 49 times. The skin
  modules restyle Blizzard frames found by name.
- **Namespaces:** 32 `C_*` namespaces, 127 distinct functions. Most used:
  `C_Timer` (`After` 116, `NewTicker` 20), `C_Map` 60, `C_QuestLog` 59,
  `C_Item` 42, `C_Spell` 35, `C_CVar` 24, `C_Container` 19, `C_GossipInfo`,
  `C_SuperTrack`, `C_UnitAuras`, `C_Secrets` (retail 12 secret values),
  `C_ChatInfo` (addon messages), `C_AddOns`.
- **Global functions:** about 300 distinct, mostly `Unit*` (`UnitGUID`,
  `UnitName`, `UnitClass`, `UnitLevel`, `UnitExists`...), `Get*`
  (`GetTime`, `GetSpellInfo`, `GetRealmName`, `GetCVar`, `GetActionInfo`...),
  `InCombatLockdown` 59, `PlaySound`, `SetOverrideBindingClick`, merchant,
  mail, trainer and talent functions.
- **Globals that are not functions:** `GameTooltip` 155, `UIParent` 74,
  `Enum` 64, `SOUNDKIT` 11, `SlashCmdList`, font objects
  (`GameFontNormal`...), and many named Blizzard frames (`MerchantFrame`,
  `QuestFrameAcceptButton`, `WorldMapFrame`...).
- **Saved data:** two SavedVariables (`RennerToolkitDB`, `RennersAddonsDB`)
  with recursive defaults and per-character keys, plus a mirror workaround
  for a Forever beta bug. Data schemas carry versions (`questDB=1`).
- **Localisation:** `RA.RegisterStrings("enUS"/"deDE", { key = "..." })`
  with `%d`/`%s` formats; the addon ships English and German.
- **Combat:** `InCombatLockdown` checks and `PLAYER_REGEN_ENABLED` queues,
  secure attributes, override bindings.
- **Addon messages** between group members (`C_ChatInfo.SendAddonMessage`).
- A second addon (`RennersAddons_QuestData`) holds large generated data
  tables.

## What this means for the order of work

1. **Language core (SPEC step 2) stays first,** but in this order, because it
   is what almost every file uses: functions and arrow functions (closures),
   objects and module tables, arrays with `for...of`, `.length`, counted
   loops, multiple returns and rest parameters, strings with templates and
   the string library, `Math`, then `class`, `try`/`catch`, modules with
   `.toc` order, `saved`, `command`, `hook`, `L`.
2. **Typed Blizzard objects need real manifests.** A typed checker against
   the 5-function fixture can't check `frame.SetPoint(...)`. Recommendation:
   start the manifest generator (SPEC step 6) right after the language core,
   before the GUI framework, at least for widget types and their methods,
   the events above and the ~430 functions above, for `retail` and
   `forever`.
3. **`has(...)` and per-target checking** are worth more to this addon than
   any other feature; they come with the manifests.
4. **The simulator** must model the widget methods and scripts listed above,
   `C_Timer`, combat lockdown and `hooksecurefunc` (SPEC step 3 already
   covers these).
5. **Proof (SPEC step 8):** a good first module to port is a small,
   self-contained one such as `XPBarETA.lua` (700 lines) or `JunkSell.lua`
   (450 lines): events, a frame, saved settings, a slash command, strings in
   two languages, and client differences.

## Open for the owner

- **Classic flavours:** the toolkit ships for Classic Era and the classic
  branches as well. Porting all of it needs `classic-era` and `classic`
  targets, which SPEC-V1 puts after v1. Porting a retail/Forever feature
  first works without them.

## Decided here

- **Weak tables** (about 55 uses): `WeakMap<K, V>` (weak keys, as in
  TypeScript) compiles to a table with `__mode = "k"`. Weak values, if
  needed, get their own type later. Built together with `Map`.
