# Selu agent rules

Read `E:/Development/AGENTS.md` and `E:/Development/general-agent-rules.md`
first. These rules are specific to Selu and also apply to every brief written
for a delegated job.

## No Lua interop (owner, 2026-11-05)

Selu is a closed, typed world. Every global a Selu program uses is either
typed by the game's API data (`manifests/`) or defined by Selu code. "It
compiles" must mean "it was checked".

Never build, propose as a work package, or brief anything that pulls untyped
Lua into a Selu program. In particular:

- plain `.lua` files in a project (`lua = [...]` in `selu.toml`);
- `*.d.selu` or any other hand-written declaration files, and `declare global`
  or `declare module`;
- shipped declarations for Lua libraries (LibStub, CallbackHandler, Ace and
  similar);
- dynamic global access (`_G[...]`, a lookup by a name built at runtime) or any
  other escape hatch that returns `unknown` from the game;
- typing Lua mechanisms such as `setmetatable`, `getmetatable`, `Mixin` or
  `CreateFromMixins`.

SPEC-V1 section 5 is withdrawn. When porting Lua code (for example Renner's
Toolkit), rewrite it in Selu terms: use classes instead of mixins and
metatables, and, where Blizzard frames really are looked up by a built name,
add typed lookups generated from the manifests for those specific frame
families.

Still allowed: `export global`, which publishes a Selu value as a Lua global,
for example for keybindings, macros or other addons. It is Selu code defining
its own global, not Selu consuming Lua.

## Only build what the owner asked for (owner, 2026-11-05)

Don't start a work package, or widen one, unless the owner asked for it. If a
package would be your own idea (for example from a portability estimate or a
spec section), describe it and ask first. Never present it as already
decided.

## Every WoW product (owner, 2026-11-07)

Selu, the simulator and Studio support every World of Warcraft product
(`wow`, `wowf`, `wow_classic`, `wow_classic_era`, `wow_anniversary`,
`wow_classic_titan`, and their PTR and beta channels). Never assume one product.

- Product-specific values (TOC suffixes, game types, Blizzard addon lists,
  enums, constants, file IDs, folder names) come from one table per concern,
  covering every product, or from the session's own client and build at
  runtime.
- No Forever-only (or any single-product) defaults, fallbacks or lists in
  non-test code. A single addon may still target one product (the toolkit
  port targets `wowf`); the tools may not.
- No personal paths (for example `E:/Development/...`) as defaults in code.

## Blizzard content: never ship Blizzard's IP (owner, 2026-11-07)

Selu never ships Blizzard-owned intellectual property. Everything Blizzard
owns is loaded from the user's own game files at runtime, when it is needed.

- Blizzard's Lua, XML, art, fonts, sounds, text and game data are read at
  runtime from the user's own installation (or their local source
  checkout). Never commit, bundle, cache in the repository or redistribute
  them, or data exported or derived from them.
- Never ship decryption keys. Selu does not embed, commit, download or look
  up encryption keys. Decrypting protected client files is only possible
  with keys the user supplies themselves through an opt-in setting, at
  their own choice; by default Selu uses readable data only (for example
  text borrowed from another installed product).
- Keys are secrets: never print or log them, or put them in reports, errors
  or tests. Name only the key ID. Agents never obtain or use real keys;
  tests use self-made keys and self-made encrypted fixtures.
- Test fixtures are self-written. Screenshots with client art stay local.

## The simulator runs Blizzard's UI, never a copy of it (owner, 2026-11-07)

The simulated default UI comes from executing Blizzard's own Lua and XML and
rendering the result. Never write Rust or Lua that names a Blizzard frame,
template or button to show, hide, place, label or restyle it. Fix the
simulated engine instead (native API results, events, widget behaviour,
layout). Blizzard object names may appear in tests that check results.
