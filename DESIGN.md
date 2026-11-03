# Selu — design

Selu is a language that compiles to World of Warcraft addon Lua. The compiler
takes the target game version as an argument and checks the program against
the API surface of that exact client.

- **Name:** Selu (Selene, the moon, + Lua). CLI `selu`, source files `.selu`.
- **Core rule:** the default output is readable Lua that a human can debug and
  that is allowed on every addon platform.

## 1. Why

WoW addon Lua fails late. When you call an API the client lacks, misspell an
event, do arithmetic on a secret value, or touch a protected frame in combat,
the error only shows up at runtime, often in front of a player. Each client
flavour (retail, Classic Era, Classic progression, Forever) has a different API
set, so the same code can work on one and break on another. Selu moves these
failures to compile time.

## 2. Targets

```
selu build --target retail
selu build --target classic-era
selu build --target forever@1.60.1.69913
```

- A target is a **manifest**: the native functions with their signatures, the
  events and their payloads, FrameXML globals, templates and mixins, CVars,
  `.toc` fields and the interface number, plus protected/secure and secret-value
  flags.
- Manifests are generated from the client's own sources (Blizzard UI source and
  the generated API documentation), not written by hand. Each entry carries an
  evidence level, so "the documentation says so" and "verified in the game" stay
  distinguishable.
- Calling a symbol that the target lacks is a compile error. Optional APIs need
  an explicit guarded form, and the compiler emits the existence check.
- `--target` accepts several targets. One source tree can then build for each
  flavour, with branches the compiler can resolve at compile time and an error
  when an API is missing on any requested target.

## 3. What the type system guards

Ordered by how many real bugs each one would have prevented:

1. **Missing APIs per target** (see section 2).
2. **Secret values.** Values the client may mark secret (unit health, aura data,
   some cast booleans) have a `Secret<T>` type. Arithmetic, comparisons and
   truth tests on them are compile errors until the program goes through an
   explicit safe read, which the compiler lowers to `pcall`.
3. **Combat lockdown.** Calls and frame operations marked protected can only run
   in a context the compiler can prove is out of combat, or must go through a
   generated retry that runs after `PLAYER_REGEN_ENABLED`.
4. **Events.** Event names come from the manifest, and handler parameters get
   the payload types.
5. **SavedVariables schema.** Persisted shapes are declared with a version. When
   a shape changes, the build fails unless a migration exists for it.
6. **Localisation.** User-facing strings are keys; a missing locale entry is a
   build warning (an error when configured).

## 4. Output

- One `.lua` file per `.selu` module plus a generated `.toc`. Local names and
  structure follow the source closely.
- Source maps: a runtime error at `Foo.lua:120` maps back to `Foo.selu:48`. A
  small optional helper can rewrite stack traces in error handlers.
- No runtime library is required. Helpers are emitted only when used.

## 5. Optimizer

| Flag | Does |
|---|---|
| `-O0` | Direct translation, for debugging the compiler |
| `-O1` (default) | Constant folding, dead-code removal, dropping target branches that can't run on the chosen target |
| `-O2` | Caching hot globals in locals, inlining small functions, merging event dispatch, removing table allocations in handlers that run often when they are provably unnecessary |
| `--strip-comments` | Smaller output that stays readable |

Every optimization must keep the output readable and keep the source maps
correct.

## 6. Experimental minifier

```
selu build -O2 --experimental-minify
```

- Off by default, never enabled by a profile or config file silently.
- Every build that uses it prints: *"Minified output may violate Blizzard's
  AddOn policy and the CurseForge/Wago rules. You are responsible for where you
  publish it."*
- It strips whitespace, shortens local and upvalue names, pools repeated
  constants and can merge modules into one chunk.
- Globals, frame names, SavedVariables keys, event names and anything the
  manifest marks as externally referenced keep their exact spelling.
- Source maps still work.
- **Out of scope, permanently:** string encryption, anti-tamper, packed or
  self-decoding loaders, and anything else designed to stop a reader from
  understanding the code.

## 7. Non-goals

- Not a general-purpose Lua replacement outside WoW.
- No runtime, VM or bytecode: the client only loads Lua source.
- No way around Blizzard's protections. The compiler models secure execution
  and refuses code that would violate it; it never emits code meant to evade
  it.

## 8. Open questions

- Syntax: a Lua superset (existing Lua compiles unchanged, types are optional)
  or a distinct, stricter syntax. A superset makes migration easier; a distinct
  syntax makes the guarantees stronger.
- Implementation language for the compiler (Rust is the working assumption,
  which is why the `selu` crate name matters).
- Manifest pipeline: which sources each flavour's manifest is generated from,
  and how it is updated per client build.
- Editor support: a language server is likely needed early, because most of the
  value is errors shown while typing.
- Licence.
