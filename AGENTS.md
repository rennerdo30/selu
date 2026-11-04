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
