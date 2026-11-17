# The standard library: `@selu/std`

`@selu/std` is the standard library that ships with Selu (SL-74, owner-approved
2026-11-17). It holds the small helpers addons kept writing for themselves:
number checks, bit flags, byte-level text encodings, the game clock,
durations, timers, warn-once logging and protected reads of the player's
state. Every function is typed and works on every product
(`wow`, `wowf`, `wow_classic`, `wow_classic_era`, `wow_anniversary`,
`wow_classic_titan`).

```selu
import { now, after, warnOnceTo, inCombatLockdown } from "@selu/std";
```

Like every package, only the entry point is importable
(docs/lang/packages.md). The package keeps no saved data and creates no
globals.

Built into the language, not part of this package: string methods (`upper`,
`lower`, `trim`, `slice`, `includes`, `startsWith`, `endsWith`, `split`),
array methods (`push`, `pop`, `shift`, `insert`, `removeAt`, `indexOf`,
`includes`, `slice`, `join`, `sort`, `map`, `filter`, `find`, `some`,
`every`, `reduce`, `forEach`), `Map`, `Set` and `WeakMap`, `Math`
(`floor`, `ceil`, `round`, `abs`, `clamp`, `min`, `max`, `random`) and
`format`.

## Numbers

| Function | Result |
|---|---|
| `finite(value: number?): number?` | The value, or `null` for `null`, NaN and the infinities |
| `wholeNumber(value: number?): number?` | The value when it is a whole number of zero or more (a count, an ID), else `null` |
| `clampOr(value: number?, low, high, fallback): number` | The value limited to `[low, high]`; `fallback` for `null` or NaN |
| `inUnitRange(value: number?): boolean` | True for a value in `[0, 1]`, the range of map coordinates |
| `roundTo(value: number, decimals = 0): number` | Rounded half up to `decimals` places |
| `hasBit(mask: number, bit: number): boolean` | True when the flag `bit` (a power of two) is set in `mask`; no bit library needed |

## Text

All text functions work on bytes, as Lua strings do.

| Function | Result |
|---|---|
| `hexEncode(text): string` | Two upper-case hex digits per byte |
| `hexDecode(digits): string` | The bytes back (either case); a non-hex pair gives byte 0, a trailing odd digit is ignored |
| `adler32(text): string` | Adler-32 checksum as eight lower-case hex digits |
| `truncateBytes(text, maxBytes): string` | At most `maxBytes` bytes; a UTF-8 character the cut would split is dropped whole |
| `plainText(text): string` | UI escape bars (`\|`) and control bytes turned into spaces, safe to show text another player sent |

## Time

| Function | Result |
|---|---|
| `now(): number` | The client clock (`GetTime`) in seconds, or 0 while it cannot be read |
| `durationParts(seconds): DurationParts` | `{ days, hours, minutes, seconds }`, whole numbers; negative counts as 0. Addons format the parts with their own localised strings |
| `after(seconds, callback): TimerHandle` | Runs `callback` once after `seconds` |
| `every(seconds, callback): TimerHandle` | Runs `callback` every `seconds` until cancelled |

`TimerHandle` is `{ cancel: () => void }`; `cancel` may be called more than
once.

## Logging

`warnOnceTo(log: (message) => void): (key, message) => void` gives a function
that passes a message to `log` the first time its key is seen and drops later
messages with that key. Each call has its own keys, so two modules never
silence each other:

```selu
const warnOnce = warnOnceTo((message) => { logMessage("WARN", message); });
warnOnce("map", "map: no position on this map");
```

## The player's state

Each read is protected: a client that refuses or hides a value gives the
stated fallback, never a Lua error.

| Function | Result |
|---|---|
| `inCombatLockdown(assume = false): boolean` | True during combat lockdown. `assume` is returned when the client gives no readable answer: pass `true` where a wrong "no" would touch a protected frame, `false` where it only skips work |
| `playerInCombat(): boolean` | True in combat lockdown or while the player affects combat |
| `playerClassToken(): string?` | The class token (`"WARRIOR"`, `"MAGE"`, ...) |
| `playerLevel(): number?` | The player's level |
| `shiftKeyDown(): boolean` | True while a Shift key is held |

## Tests

`crates/selu-cli/tests/std_package.rs` type-checks the whole API on every
product and runs its behaviour tests under the simulator
(`tests/fixtures/std-package/`).

## Adding to it

A helper belongs here when addons repeat it, it is typed, and it works the
same on every product. Each addition comes with a behaviour test in the
fixture and a line in this page.
