# Saved data

`saved settings = { ... };` declares account-wide data. Use
`saved perCharacter progress = { ... };` for character-specific data. Defaults
must be literal serializable objects/arrays, primitives or empty typed Maps.
Functions are rejected recursively, including inside collection types. Map and
set keys must be primitive numbers, strings or booleans; table keys and weak
maps cannot be persisted. Names obey the usual Lua keyword and reserved
`__selu` prefix checks. The root object binding is immutable; mutate its fields.

The default global name is `<AddonName>_<savedName>`, using `[addon].name` from
`selu.toml` at compile time. For example, addon `SavedSettings` and declaration
`settings` use `SavedSettings_settings`. Module paths are deliberately absent:
moving a declaration between modules keeps its data. Saved names and effective
global names must be unique across the addon; duplicate diagnostics underline
both declarations. Exported saved values remain live bindings.

## Porting an existing addon

Adopt its existing SavedVariables global explicitly:

```selu
saved global MyAddonDB version 2 = { count: 0 };
migrate MyAddonDB 1 -> 2 (old) {
    old.count = old.x;
    old.x = null;
}
on PLAYER_LOGIN() {
    print(MyAddonDB.count);
}
```

The `.toc` lists exactly `MyAddonDB`. A player's legacy
`MyAddonDB = { x = 99 }` has no Selu metadata, so it counts as version 1;
the migration preserves 99 as `count`. Fresh data (the global is nil) starts
at the declaration's current version and receives defaults without historical
migrations. This unversioned-data rule applies to default globals too.
Use `saved global perCharacter ExistingCharacterDB = { ... };` to adopt a
SavedVariablesPerCharacter global. Keep the same account/character scope as the
old addon. Do not rename a published global or change its storage scope without
a separately designed data transfer; builds reject such history changes.

## Loading and migrations

Declarations and migrations are file-scoped; write each migration after its
saved declaration. Keep exactly one adjacent migration for every version step
from 1 to the current positive integer version. A migration receives an untyped
historical object in its written parameter. Each attempt receives a deep copy
of the last good data; success commits its version and object together. Failure
logs a warning, stops that declaration's chain and preserves the last good
version. The next login attempts the failed step again. External side effects
of a migration cannot be rolled back; confine changes to its parameter.

Root `__selu_version` is reserved. Version metadata must be a positive integer.
Invalid metadata or a non-table root is left untouched with a warning, and
other saved declarations continue loading. Newer-version data is also left
untouched with a warning and receives neither migrations nor defaults.
Warnings use D4's per-addon log runtime at `warn` level when available; otherwise
they use captured `print`. Disabled warning levels follow D4's normal rules.

After migrations, missing object fields are filled recursively using `nil`
checks, preserving false and all existing values. Missing arrays/maps and other
tables are deep-copied. Existing arrays and maps are not traversed to fill
element fields. Thus adding an object field with a default is compatible;
changing an array/map element shape, including adding a field, requires a
version bump and migration. Removing or retyping any existing field also needs
that bump. Every successful build writes deterministic `selu.saved.toml`;
commit it. Failed builds preserve the prior history.

Read saved data at PLAYER_LOGIN or later. Direct top-level reads/writes,
including imported bindings and member roots, are SELU0460. Functions may refer
to saved values, but callers must wait for loading. An arbitrary ADDON_LOADED
handler may run before initialization; PLAYER_LOGIN is the earliest guaranteed
event for addon code.

## Runtime and simulator

Each file has exactly one reserved loader line and one saved runtime prologue
when needed. Files share `__selu_ns.__selu_saved`, one event frame and one set
of helpers. Only the globals used by those helpers are captured. Every generated
local/parameter has a `__selu` prefix; each declaration adds only its user
binding at file scope. Loaders and migrations, the event script and the reserved
namespace entry are released after the addon's ADDON_LOADED. There is no idle
updater. D1/D4 reserved boolean namespace keys remain unchanged.

`lower_with_addon` supplies the compile-time addon name for standalone callers;
`lower` defaults to `Addon`. Project lowering passes the configured name through
module bindings. The loader's runtime addon name is used only to filter events.

`load_addon` fires ADDON_LOADED after its files. `logout_saved(names)` fires
PLAYER_LOGOUT and captures plain Lua data into a VM-independent SavedSnapshot;
`login_saved` restores it into a fresh VM before loading files. Cycles and
nonserializable values fail explicitly. For multiple characters, SavedSessions
holds one account snapshot and a map of character snapshots. Pass a character
identity to `Sim::login_character` and `logout_character`, plus account and
character global lists on logout. Account values survive a character switch;
character values are restored only for that identity. Overlapping scope lists
are rejected.

Class instance field initializers and methods may read saved values because they
execute later; static field initializers are checked as eager top-level reads.
Class types are rejected recursively in saved shapes. Class-typed values also
cannot be stored through saved-root field/index assignments or collection
push/insert/set/add calls, including imported saved roots. Copy their data into
a plain object instead, for example `{ x: p.x, y: p.y }`. Class metatables and
methods cannot round-trip through SavedVariables.
