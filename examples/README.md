# Examples

Each folder is a complete small addon. From the repository root, build one with
`cargo run -p selu-cli -- build examples/<name>` (or `selu build examples/<name>`
when the CLI is installed). Copy `examples/<name>/dist/<AddonName>/` into the
matching client's `Interface/AddOns` folder, enable it, and log in.

For an offline preview, run `scripts\studio.bat examples\<name>` and fire
`PLAYER_LOGIN`. Running `scripts\studio.bat` with no argument opens `frames`.
For timers, advance simulator time by 0.2 seconds after login. The fixture
manifests and simulator cover a subset of the client; offline success does not
verify client rendering. Sample loot, cooldowns and quest rewards are explicitly
illustrative rather than live scans.

| Example | What it teaches | How to try it after building or opening Studio |
|---|---|---|
| [`hello`](hello/src/main.selu) | Events, default parameters and a login greeting | Log in |
| [`frames`](frames/src/main.selu) | Typed frames, regions, anchors and OnShow hooks | Log in to show the welcome panel; Studio default |
| [`classes`](classes/src/main.selu) | Inheritance, super, static counters and lexical this | Log in for sample reward totals |
| [`combat-safe`](combat-safe/src/main.selu) | Secure templates and deferred protected receiver calls | Log in; a protected hide can wait for combat to end |
| [`cooldowns`](cooldowns/src/main.selu) | Tuple results, closures, filtering and sorting | Log in for a sample cooldown report |
| [`errors`](errors/src/main.selu) | Typed Error, fallback returns and finally cleanup | Log in for a deliberate recoverable sample failure |
| [`item-links`](item-links/src/main.selu) | Lua pattern captures and lazy word iteration | Log in to inspect a sample trade message |
| [`loot-summary`](loot-summary/src/main.selu) | Arrays, Map, transforms and copper arithmetic | Log in for a sample haul |
| [`modules`](modules/src/main.selu) | Named imports, live exports and module ordering | Log in for a shared sample quest tally |
| [`multi-target`](multi-target/src/main.selu) | Shared retail/Forever layout and capability guards | Log in on either configured target |
| [`named-types`](named-types/src/main.selu) | Structural interfaces, aliases and unknown narrowing | Log in for sample route rewards |
| [`saved-settings`](saved-settings/src/main.selu) | Recursive defaults, migration and per-character state | Log in; reload to increase the character tally |
| [`slash-and-hooks`](slash-and-hooks/src/main.selu) | Slash aliases, additive hooks, logging and localisation | Log in, then type /status or /st |
| [`source-maps`](source-maps/src/main.selu) | Nested callbacks and source-map positions | Log in for a reminder; inspect dist/.selu sidecars |
| [`templated-frames`](templated-frames/src/main.selu) | Backdrop and button template members | Log in, then click Got it |
| [`timers`](timers/src/main.selu) | Delayed callbacks and shared pending state | Log in; wait 0.2s for the reminder |

`cargo test -p selu-cli --test examples` compiles every example with the real
compiler, loads Lua in `.toc` order, fires `PLAYER_LOGIN`, and compares prints
with `expected.txt`. It also compares generated Lua with `expected.lua` (and
module fixtures) and checks source-map coverage. It does not advance time or
click buttons, so the timers fixture contains only the immediate queued message.
Source-map regressions also run in `cargo test -p selu-cli --test sourcemap`.
