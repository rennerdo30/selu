# Other addons: the checked boundary

The interface to World of Warcraft is always fully typed. Other addons are
reached only through this boundary (owner, 2026-11-10; syntax approved
2026-11-13). Their values enter as `unknown` and must be narrowed with
`typeof` before use. Nothing of another addon is copied, cached or shipped.

## Reading another addon's global

```selu
const frame = addon("Lorewalker").global("LWDialogFrame");   // unknown
if (typeof frame == "table") { ... }
```

- Both names are literal strings (SELU0700); a name built at runtime would be
  a dynamic global lookup.
- The value is the global only while that addon is loaded (checked with the
  target's `C_AddOns.IsAddOnLoaded`, or `IsAddOnLoaded` where only that
  exists); otherwise `null`. The load check never raises.
- The addon becomes an optional dependency: the built TOC lists it under
  `## OptionalDeps`, so it loads first when installed and is never required.
- A name the game data knows (a function, global, frame, template, mixin or
  documented table) is refused (SELU0702), and so is a `Blizzard_` addon:
  game values stay typed. `addon(...)` alone, or any member other than
  `.global(...)`, is SELU0701.

## Fields of unknown values

A field of an `unknown` value is a protected read and is `unknown` again:
`frame.isShown.deep` never raises a Lua error, whether the value is a
string, a number or a table whose `__index` raises. Calls, arithmetic,
conditions and stores still need a `typeof` check first.

## Fields other addons put on frames

```selu
const bag = addonFields(button).bag;   // unknown
```

`addonFields(frame).field` reads the field raw (`rawget`), for values other
addons store on frames (Bagnon's bag buttons). It only accepts a frame
(SELU0704) and only as `.field`. A member the game data declares for that
frame (a method, a widget field, a template or mixin member, a keyed child)
or a reviewed Lua field of any Blizzard mixin is refused (SELU0705): read it
as the typed member it is.

## Declared addons

A project can declare the part of another addon's public Lua API it uses, in
`addons/<Name>.selu` beside `src/` (one file per addon, named after its
folder):

```selu
addon "QuestieDB" {
    type Quest = { name: string; level?: number };
    global LibQuestieDB: {
        RequireContract: (contract: number) => boolean;
        Quest: (id: number) => Quest?;
        Each: (visit: (id: number, name: string) => void) => void;
    };
}
```

```selu
import { LibQuestieDB } from addon "QuestieDB";

const lib = LibQuestieDB;            // the declared table, or null
if (lib != null && lib.RequireContract(4) == true) {
    const quest = lib.Quest(1234);   // checked; null when absent or wrong
    if (quest != null) { print(quest.name ?? "?"); }
}
```

- The file holds exactly one `addon "Name" { ... }` with `global Name: Type;`
  entries and `type` declarations (SELU0710). A global the game data knows,
  or a `Blizzard_` addon, is refused (SELU0702). Declarable types: numbers,
  strings, booleans, literals, arrays, `{ ... }` tables, functions, unions,
  optionals and `unknown` (SELU0711).
- An imported global is read live on every use: `null` while the addon is
  not loaded, otherwise the global checked against its declaration. The
  addon becomes an `## OptionalDeps` entry.
- Every value that crosses in is checked at runtime: the global, each field
  read, each call result, and each argument the other addon passes to a Selu
  callback. A value of the wrong shape becomes `null`, with one warning per
  place in the chat. So every crossing value is typed `T?`, and a callback's
  parameters are optional.
- Calls into the other addon are protected: an error there gives `null` and
  one warning, never a Lua error in Selu code.
- The other addon's tables and functions stay foreign values. They fit only
  their own declared types (or `unknown`), never a Selu type of the same
  shape, so every later read stays checked. Their fields are read-only
  (SELU0713); an undeclared field is an error (SELU0712).
- Functions are called with `.` (`lib.Quest(1)`); Lua methods called with
  `:` are not declarable yet.

## Selu addons using each other

A Selu addon publishes values for other Selu addons with one list:

```selu
type Quest = { id: number; title: string };
function quest(id: number): Quest? { ... }
export addon { quest as findQuest, version };
```

- Only constants, functions and imports are published (SELU0714), and only
  plain data and functions cross (SELU0711): numbers, strings, booleans,
  literals, arrays, `{ ... }` tables, functions, unions and optionals.
  Classes and game objects stay inside the addon. One `export addon` list
  per addon.
- `selu build` writes `dist/<Addon>.selu-api`: the published names and their
  types, written out in the declared-addon syntax with the API's
  fingerprint (`addon "Provider" fingerprint "..." { global findQuest: ...; }`).
  It holds types only, nothing of the addon's code.
- At runtime the addon puts its values and the fingerprint in the shared
  `SeluAPI` table under its name.

A consumer lists the file under `[addons]` in its `selu.toml` and imports
the names:

```toml
[addons]
Provider = "../Provider/dist/Provider.selu-api"
```

```selu
import { findQuest } from addon "Provider";
```

- Imports are read live, `null` while the provider is not loaded, and
  read-only; calls are protected, as for any declared addon.
- When the provider's fingerprint matches the one the consumer was built
  against, its values pass directly. A different or missing fingerprint (the
  provider changed its API) falls back to the per-value checks of a declared
  addon: a value of the wrong shape is `null` with one warning.
