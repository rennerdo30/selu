# XPBarETA pilot

Source: `E:/Development/RennersAddons/Modules/XPBarETA.lua`, toolkit commit
`df793af16fa1da0b9707e112ad483352e191620f`, MIT, Copyright 2026 Renner.
Selu base: `bba1ace`; branch `proof/pilot-port`. The source checkout was accessed
only with git show, git grep and git ls-tree. No game, installation or push.

This is a buildable exploratory port, not a drop-in replacement. Its purpose is
to expose full-port costs. XPBarETA is 699 physical lines, has no dependency on
Skin/Framework, and provides a real rolling XP estimator with persistence,
events, timers and frame interaction. UI, HUD, Options and CPU integration are
optional in its source; those dependencies are deliberately not ported wholesale.
The original has no slash command: `/xpeta` toggles the proof and `/xpeta reset`
exercises the positioning reset otherwise reached through Options.

What ported cleanly: named sample shapes, a RateWindow class, numeric guards with
try/catch, positive-transition gain counts, 900-second pruning, 30-second minimum
span, 5-second unchanged-value coalescing, level/rollback resets, epoch-relative
history, per-character storage, English/German text, event handlers, a 15-second
ticker, additive drag hooks, readable geometry guards, frame size and visibility.
No mixin or weak table is needed by this module; WeakMap was not artificially
introduced. No plain Lua project inputs, declarations or dynamic game lookups.
The original Lua excerpt in tests/original.txt is exclusively a simulator oracle.

## Gap inventory

Severity concerns the full toolkit port. Counts below are matching **lines**, not
semantic occurrences, over Core.lua and Modules/**/*.lua at the source commit,
including comments. Reproduce with `git grep -n -E '<pattern>' <commit> -- Core.lua
'Modules/*.lua' 'Modules/**/*.lua'`. Counts overlap; they must not be summed into
an estimate of total work. Implementation omissions are distinguished from
compiler limitations. No compiler or manifest changes were made.

| # | Severity | Original location and pattern | Evidence and disposition | Toolkit prevalence |
|---|---|---|---|---|
| 1 | blocker | XPBarETA.lua:294-299, `_G[name]` | Closed typed world forbids generic global resolution. Use standalone host immediately; named-bar probing and its 2s/10-attempt/30s retry state machine omitted. F2 typed frame-family lookups are in progress; finite literal candidates may be expressible already. This is a migration blocker for the dynamic pattern, not a claim all these names are missing. | `_G\[` 126 lines / 39 files |
| 2 | blocker | XPBarETA.lua:267-289, :547-585, manager bars and `pairs(_G)` | Arbitrary frame shape inspection and global value scanning cannot be ported as written. Omitted, with standalone display. A typed API for known manager families needs sourced manifest data; global enumeration should be redesigned away. F2 may cover families but does not establish generic scan support. | `pairs\(_G\)` 1 / 1; `\.bars` 70 / 12 (broad proxy) |
| 3 | costly | XPBarETA.lua:96-144, RA.db.xpEta.rate and Core database | Proof uses isolated per-character history and settings. Existing RennerToolkitDB is neither read nor migrated. `saved global` exists, so adoption is not a missing language feature; typed validation, schema design and old character-key migration remain real port work. Corrupt saved arrays are not sanitized by this pilot. | `RA\.db` 188 / 40 |
| 4 | costly | XPBarETA.lua:433-542, UI card/theme and HUD lock | Standalone unthemed text; no card rail/backdrop, theme subscriptions, Options refresh or HUD lock. This is an unported shared Selu UI/HUD dependency, not a demonstrated parser limitation. Dragging remains enabled whenever text is visible. | `RA\.UI` 79 / 57; `RA\.HUD` 32 / 10 |
| 5 | costly | XPBarETA.lua:433-479, frame interaction | Simulator cannot prove drag movement/clamping or final rendered text. Exact observed notes below. Owner/native acceptance is needed after the shared systems port. | same UI proxy as 4 |
| 6 | costly | XPBarETA.lua:13-233, behavioral acceptance | S4 `selu test` is in progress; current crates/selu-test explicitly says execution is not implemented. Isolated Rust simulator tests supply acceptance now. | 23 NewTicker lines / 7 files indicate recurring timer tests, not a count of missing tests |
| 7 | minor | XPBarETA.lua:20, empty samples | Class field `samples: Sample[] = []` gives SELU0351 despite annotation. Rewritten through an explicitly typed local in emptySamples(). Saved empty-array assertion `[] as Sample[]` is not syntax; a default sentinel sample with level -1 is rejected on load then replaced on first record. This exposes a default typing ergonomic cost, not a valid TypeScript assertion silently accepted. | `table.remove` 20 / 12; array-use proxy only |
| 8 | minor | XPBarETA.lua:134, :161, :674, wipe(samples) | array.clear() gives SELU0390; replace with a fresh typed empty array. Allocation differs from wipe. No zero-allocation claim. | `wipe\(` 84 / 13 |
| 9 | minor | XPBarETA.lua:690, NewTicker callback | Manifest expects `(() => void) => void`; zero-parameter callback emits SELU0481. Accept and ignore that typed callback argument to build warning-free. The real WoW ticker object is not consumed; audit callback signature generation before callbacks use it. | `C_Timer.NewTicker` 23 / 7 |
| 10 | minor | XPBarETA.lua:5-11, RA.RegisterStrings | Existing L intrinsic supports localization, but no shared toolkit registration API was ported. Explicit GetLocale branch retains both source messages and English fallback. Shared translation ingestion remains integration work. | `RA\.RegisterStrings` 101 / 43 |
| 11 | minor | XPBarETA.lua:3, :669 and Core guards/logs | CPU wrapper and central diagnostics omitted; numeric arithmetic usability probe kept, but secret reason classification and type checks are reduced to typed API plus try/catch. readable() handles geometry. No actual secret-engine acceptance. | `RA\.CPU` 77 / 33; `RA\.Log` 341 / 54; `RA\.SafeNumber` 125 / 20 |
| 12 | minor | XPBarETA.lua:96, saved/event setup | Generated shared saved loader/dispatcher expands output substantially. Retain unedited output in ignored dist for inspection; no output-size or performance win claimed. | database proxy 188 / 40 |

Totals: **2 blocker, 4 costly, 6 minor**. Gap 7 and 8 have concrete diagnostics;
9 has a concrete warning. Items 3, 4, 10 and 11 are integration costs or pilot
omissions, not evidence the language cannot express the corresponding system.
No new feature package is proposed or started here.

## Simulator evidence and limits

Run `cargo test --manifest-path proof/xpeta/tests/Cargo.toml -- --nocapture`.
Three tests compile against all six non-fixture manifests and execute retail Lua:
1. Same XP trace through original lines 1-233 and the compiled Selu: blank warmup,
   positive gains, rapid gains, periodic unchanged XP, German text and expiry.
2. Eligibility, visibility, command toggle/reset and level-up history reset.
3. VM-independent saved round trip with session-clock reset, retained estimate,
   and rollback rejection. This is not legacy database migration acceptance.

The original oracle uses only stubbed Core registration/logging and game API
values. It does not execute the original display resolver. The deliberately
broken in-memory generated Lua (`XPETA_BREAK=1`, gains += 0) fails test 1 with
`differential ETA mismatch:  vs ~2m to level (~8 kills)`; normal tests pass.
Time advances through Sim::advance_time; no sleeps or game launches.

Observed `not simulated` notes:
- Frame:SetMovable
- Frame:SetClampedToScreen
- Frame:RegisterForDrag
- FontString:SetTextColor

An initial test probe of Frame:GetRegions returned nil (unsupported); naming
the proof FontString allows direct inspection without changing simulator code.
Width is the simulator's approximate glyph measurement. Real dragging, HUD lock,
clamping, theme/font appearance, protected execution, secret XP, malformed data,
character switching, offline level changes and bar attachment remain unverified.
The standalone panel appears immediately instead of waiting for bar resolution.
API exceptions inside readXP/readMaximum are caught; the source's broader
pcall boundary around ComputeETAText and time()/geometry calls is not reproduced.

## Recommendation order

Before a full port, settle the typed frame-family/manager access required by 1-2
(F2 already in progress), and redesign the one global scan without an escape
hatch. Then design database adoption and validation (3) and port the shared
UI/HUD contracts (4), before migrating consumers. S4 and simulator interaction
coverage (5-6) should be ready before scaling acceptance to dozens of modules.
Audit ticker callback metadata (9) before any module reads the callback argument.
The smaller array ergonomics, translation integration and diagnostic/readability
costs can be handled during migration. This pilot does not authorize new packages.

## Size and build evidence

Physical lines, including comments and shared runtime; these are unequal scopes:

| Artifact | Lines |
|---|---:|
| Original module (Core helpers excluded) | 699 |
| Selu module | 256 |
| Generated main.lua | 313 |
| Generated shared Selu.lua | 289 |

Generated counts describe the final shared build (classic); all six targets build
without warnings: retail 12.1.5.70391, forever 1.60.1.70532, classic-era
1.15.9.70576, classic-anniversary 2.5.6.70577, classic-titan 3.80.2.70522,
classic 5.5.4.70631. Shorter Selu partly reflects omitted integration.

Validation: root fmt check and strict workspace/all-target clippy passed.
JunkSell: 4 passed, 0 failed. XPETA: 3 passed, 0 failed.
Full workspace gate result and final head are recorded in the external report.

Package SB resolves gap 9's metadata mismatch: sourced extraction v5 gives the
callback a cancellable timer handle. The proof now uses a zero-argument callback
because it ignores that handle, removing the obsolete function-typed workaround.
This correction is offline compiler/simulator evidence, not a new native playtest.
