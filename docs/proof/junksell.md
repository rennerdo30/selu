# JunkSell proof at 5aaa837

Source: `E:/Development/RennersAddons/Modules/JunkSell.lua`, Core.lua helpers
ResolveContainerQuestStatus, ResolveItemID, IsSellLocked and WouldAutoSell,
commit `df793af16fa1da0b9707e112ad483352e191620f` (MIT, Copyright 2026 Renner).
Started from `5aaa837cebcf7e25c3bb328993d52151d6cf3b4c`, branch `proof/junksell`.
The source checkout was read only. Its TOC and docs/WOW_LUA_API.md were read.

This is a buildable exploratory port, **not a drop-in replacement or safety-equivalent
release candidate**. Acceptance 1 is partial; 2-4 have artifacts and offline evidence.
The source's runtime defenses remain necessary: manifests describe availability and
signatures, not whether a call throws or returns secret/unreadable values.

Run from the repository root:

```powershell
cargo test --manifest-path proof/junksell/tests/Cargo.toml
# Deliberate broken-port proof (expected failure):
$env:JUNKSELL_BREAK = '1'
cargo test --manifest-path proof/junksell/tests/Cargo.toml merchant_visit
Remove-Item Env:JUNKSELL_BREAK
# Show diagnostic reproductions:
cargo test --manifest-path proof/junksell/tests/Cargo.toml gap_reproductions -- --nocapture
```

The isolated Rust test crate is under proof/ to respect ownership. Root cargo test
does not discover it. Its merchant test compiles the proof source in-process
using the compiler crates and the retail manifest, writing ignored dist output.
It needs no prior CLI build and starts no child processes.
JUNKSELL_BREAK changes only an in-memory copy of that Lua, replacing the count
increment with zero. The observed assertion failure included
`Sold 0 junk item(s) for 20c.`. Restored tests pass. The test stubs merchant,
container and item functions with Sim::eval; it checks junk 101 sold,
quest 102, locked 103 and quality-2 item 104 skipped, repair using own funds,
and two newly sold locked duplicates bought back from index 2 then 1.

**Status after round 5 (2026-11-04):** gaps 2 (`type`/`interface`) and 3
(`typeof`) are closed by package L; gap 5 (`MerchantFrame`) is closed by M3;
gap 6 (`NUM_TOTAL_EQUIPPED_BAG_SLOTS`) is closed on retail and forever, while
the classic branches only reference it (guard with `has(...)` there). Round 6
closed gap 1 (`try`/`catch`, package E), gap 4 (string patterns, package S)
and gap 7 (simulator timers, package Q); gap 8 (runtime scaffolding) is open.
`tests/src/lib.rs`
(`gap_reproductions`) asserts each gap's current status, so this list can't
drift from the compiler. The port itself still uses its original
workarounds.

Numbered gaps encountered (4 language, 0 checker false errors, 2 manifests,
1 simulator, 1 readability; 8 total):

1. **Closed: structured error handling.** Minimal reproduction:
   `try { print(1); } catch (e) { print(2); }` produces SELU0101 at `try`'s
   following space, suggesting a semicolon. Expected SPEC-V1 2.5 structured
   exception handling, or an unsupported-feature diagnostic. Workaround:
   `const [ok, value] = pcall(GetMoney);` checks on all six flavours. The port
   uses pcall wrappers; this does not remove the need for runtime validation.
   No parser changes were made.
2. **Language: named types not parsed at this base.**
   `type Item = { id: number }; const x: Item = {id: 1};` gives SELU0101 after
   `type`. Expected a reusable structural name. Workaround: inline object types
   (queue entries). Package L owns the eventual implementation.
3. **Language: typeof not parsed.**
   `function safe(x) { return typeof x == "number"; }` gives SELU0101 before x.
   Expected narrowing for values returned by pcall and older names-only APIs.
   Lua `type(x)` can be called, but does not supply the promised TypeScript
   narrowing. The proof primarily relies on API types and pcall, with less
   thorough validation than Core.SafeNumber/SafeBool. L may address this.
4. **Language/library: string pattern match - closed by package S.**
   ``const x = "item:123".match(luaPattern`item:(%d+)`);`` compiles on all six
   flavours. Plain-string `.match("item:(%d+)")` remains an error and suggests
   `luaPattern`. The port retains its C_Item.GetItemInfoInstant workaround;
   it has not been rewritten. See `docs/lang/patterns.md`.
5. **Manifest: MerchantFrame unresolved.** `const x = MerchantFrame;` gives
   SELU0310 on every real flavour. Expected the source-defined native frame
   so MerchantFrame.IsShown can be revalidated before a sale. Locations
   searched: all six committed manifests through load_manifest_dir plus
   checker resolution; source Modules/JunkSell.lua, Core.lua, TOC and
   docs/WOW_LUA_API.md. This is a missing manifest declaration, not a claim
   the game lacks the frame. Workaround: event-based open state only, weaker
   than the source. M3 may supply this data.
6. **Manifest: equipped-bag count unresolved.**
   `const y = NUM_TOTAL_EQUIPPED_BAG_SLOTS;` gives SELU0310 on every real flavour.
   Expected a client-specific upper bound. Same manifest/checker locations
   searched as 5, plus LastEquippedBag in the source module. Workaround used:
   source's final fallback of four equipped bags. Additional equipped bags
   are not scanned. No fabricated declaration was injected into manifests.
7. **Simulator: merchant/container/timer behavior needs explicit models.**
   Minimal Selu: `C_Timer.After(0.2, () => { print("sold"); });`.
   Closed by package Q: Sim::advance_time runs built-in simulator timers at
   exact positive deadlines, with a 60 Hz OnUpdate grid and next-frame zero
   delays. The merchant test advances simulated time without a timer stub.
   Merchant/container APIs still use explicit fixtures; no client validity
   or wall-clock pacing is claimed.
8. **Readability: considerable lowering scaffolding survives.** Minimal form:
   `saved settings = {enabled: true}; on PLAYER_LOGIN() { print(settings.enabled); }`.
   Expected compact readable addon setup. Observed generated saved-loader,
   dispatcher and optional-value temporaries expand the real port to 439 lines.
   Needed shared runtime code is useful, but dominates a short module and makes
   hand inspection harder. Workaround: retain generated reference.lua for review;
   no compiler output edits. This is a qualitative finding, not a semantic bug.

The five most consequential findings for the whole Toolkit are 1 (error handling),
3 (untrusted/secret-value validation), 2 (maintainable named shapes), 5 (native frame
access), and 6 (client data constants). Gap 4 also affects cache-independent identity.
No checker false error is claimed: nullable quality, item-ID and vendor-price
errors encountered in the first build were legitimate and fixed by explicit
null handling. Interpolating an untyped pcall error produced SELU0443; the proof
uses a literal warning. A future fuller port can call tostring explicitly.

Behavioral differences and unfinished coverage:

- Settings preserve enabled/maxQuality/maxValue/announce/autoRepair/useGuildRepair
  defaults, but use a separate JunkSellProof_settings global. Locks live in its
  numeric Map, not Core's bags.sellLocked; this does not adopt RennerToolkitDB or
  migrate its history. There is no settings UI; tests can edit the saved global.
- English and German messages are present with the source wording, but copper
  text replaces coin textures. Locale selection is explicit GetLocale branching,
  not RA.RegisterStrings or a shared translated-key registry. Other locales use
  English. CPU tracking and RA diagnostic logging infrastructure are not ported.
- Queue rereads eligibility and item ID before acting, with 0.2-second spacing.
  It does not compare hyperlink when IDs are unavailable. Closing/disabling a
  session cancels actions, but skips the original partial-sale close summary.
  Generation invalidation cancels older sessions. queue.indexOf is an avoidable
  O(n squared) scheduling cost; no performance claim is made.
- Quest flags, positive quest IDs and class 12 protect items; pcall makes unreadable
  quest reads fail closed. The proof does not reproduce all SafeBool/SafeNumber
  secret-value validation. Initial item enumeration and queue reads are unguarded.
- Only modern C_Container/C_Item paths are implemented. The original legacy
  global container fallback and namespaced optional buyback alternatives remain
  unported, despite compilation on all six current manifests. This is an
  implementation limitation, not evidence those APIs cannot be expressed.
- Repair exceptions are caught, but a failed guild-capability query aborts repair
  rather than falling back to own funds. Output uses copper instead of textures.
- Buyback uses identity-count differences and descending indices, but has no
  transient-failure baseline state, failed-call retry bookkeeping, immediate
  asynchronous-success baseline reconciliation, warning deduplication, or pending
  timer coalescing. Buyback API calls are unguarded. The source's safety net is
  materially stronger. Normal synchronous duplicate recovery alone is tested.
- Guild repair, legacy fallbacks, timers after close, inventory moves, settings
  changes mid-sale, secret values, API exceptions, asynchronous buyback, both
  locales and persistent save round trips need further behavioral coverage.

Comparison (physical lines, UTF-8 file bytes; includes comments and runtime):

| Artifact | Lines | Bytes |
| --- | ---: | ---: |
| Original Modules/JunkSell.lua (CRLF) | 447 | 17008 |
| Selu proof main.selu (LF) | 146 | 6229 |
| Generated main.lua (LF) | 439 | 16717 |

Core helper lines are excluded from the original count; this comparison is not a
claim of equivalent completeness or a code-size win. The Selu is shorter partly
because the behavior listed above is unfinished. Generated Lua retains descriptive
local function names and straightforward event bodies, but shared runtime setup
and snapshots reduce that advantage. Original module text has 14 pcall calls and
10 type(...) checks (comments excluded by matching call spellings); Core adds more.
The proof uses three pcall boundaries and eight has(...) sites. has replaces
availability checks only: it does not replace pcall, data validation, or permission
checks. Shared layout lowers has into runtime capability checks. All six builds
succeeded with zero compiler warnings: retail 12.1.5.70391, forever 1.60.1.70532,
classic-era 1.15.9.70576, classic-anniversary 2.5.6.70577,
classic-titan 3.80.2.70522, classic 5.5.4.70631. TOC interfaces:
120105, 16001, 11509, 20506, 38002, 50504.

No WoW client was launched. Native merchant, taint/protection, server throttling,
actual locale rendering and live client acceptance did not run. No pushes,
publication, compiler edits, manifest edits, source-addon writes or subagents.
Final workspace checks and commit are in the report under E:/AgentData/reports/.
