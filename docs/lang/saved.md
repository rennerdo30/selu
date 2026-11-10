# Saved data

`saved settings = { ... };` declares account-wide data. Use
`saved perCharacter progress = { ... };` for character-specific data. Defaults
must be literal serializable objects/arrays, primitives or empty typed Maps.
Functions are rejected recursively, including inside collection types. Map and
set keys must be primitive numbers, strings or booleans; table keys and weak
maps cannot be persisted. Names obey the usual Lua keyword and reserved
`__selu` prefix checks. The source binding is an immutable typed load result. Mutate its data only inside a `whenReady` callback, or after checking `data != null`.

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
migrate MyAddonDB 1 -> 2 (old: { x?: number; count?: number }) {
    old.count = old.x ?? 0;
    old.x = null;
}
on PLAYER_LOGIN() {
    MyAddonDB.whenReady((data) => { print(data.count); });
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

With the default TOC load order, WoW executes all addon files, restores that
addon's SavedVariables, then fires `ADDON_LOADED` with the addon name. See
[Warcraft Wiki: Saved variables, The loading process](https://warcraft.wiki.gg/wiki/Saved_variables#The_loading_process)
and [ADDON_LOADED](https://warcraft.wiki.gg/wiki/ADDON_LOADED).
The [AddOn loading process](https://warcraft.wiki.gg/wiki/AddOn_loading_process)
also documents the optional `LoadSavedVariablesFirst` override; Selu does not
require that override. Generated loaders read saved globals only on their own
addon's event and ignore events naming other addons. For all declarations,
the source result stays `loading` until then: no saved-global assignment or
ready callback runs during chunk execution.

Declarations and migrations are file-scoped; write each migration after its
saved declaration. Keep exactly one adjacent migration for every version step
from 1 to the current positive integer version. A migration requires a finite,
closed historical schema on its parameter. Each attempt receives a deep copy.
Mapped adoption commits validated steps and retains the last successful version
on failure. Ordinary loading commits the complete attempt only on success;
failure retains the original restored graph and version. Both retry next login. External side effects
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
`login_saved` executes the files in a fresh VM, restores the snapshot, then
fires ADDON_LOADED. Synthetic graph tests use `load_addon_with_restore` at
the same boundary; pre-seeding registered saved globals before chunk loading
is rejected. `selu test` scenario fixtures and Session reloads follow that
order too. Cycles and
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

## Mapped-version adoption

For a pre-existing database with its own version field, use the explicit
adoption contract (this does not import Lua code):

```selu
saved global MyAddonDB version 2 using dbVersion legacy OldAddonDB = {
    count: 0, enabled: true
};
migrate MyAddonDB 1 -> 2 (old: { count: number; enabled: boolean }) {
    old.count += 1;
}
on PLAYER_LOGIN() {
    MyAddonDB.whenReady((data) => { print(data.count); });
}
```

Both exact global names enter the TOC in the declared scope. `legacy` is
optional. `using` selects the sole persisted version key: this mode never adds
Selu version metadata. The mapped key is compiler-owned and must be omitted
from defaults; it cannot use the reserved retry-context namespace. The source binding is a load result with `status: string`,
`data: <declared or defaults-inferred shape>?`, and `whenReady(callback)`. Status is `loading`,
`ready`, `collision`, `newer`, `corrupt`, or `failed`. Data is available only
for ready/collision results. Ready callbacks run after publication and defaults;
callbacks registered after success run immediately. Refusals discard pending
callbacks. Callbacks must own all persistent writers, including log attachment
and logout writers. Reading data requires a null check. A collision is usable
current data plus retained, distinct legacy data requiring explicit reconciliation.

A present current root takes precedence, including a corrupt one (it is never
overwritten by a legacy value). Otherwise the legacy table is selected by
identity. One exception: an empty current table holds nothing, so a filled
legacy table is selected instead and the current name then points to it.
Empty or absent data starts at the current version. Populated data
without the mapped key starts at 1. A version stored as text that reads as a
number (`"12"`) counts as that number and is written back as a number on
success. Non-table roots, other malformed versions and
future versions are preserved without defaults, migration or publication.

At the current version, a string, number or boolean in a field whose default
is a table (a record, array or map) is replaced by that default, with one
`repaired` warning that names the field path. No code could use such a value,
and refusing the whole save for it would disable the addon. Fields whose
schema accepts the scalar (`SavedRepair`, unions) are kept. A wrong scalar in
a scalar field is still refused. The same rule applies to ordinary saved
declarations.

Every migration in this mode requires an explicit finite serializable record
type. Optional fields distinguish absent values from malformed present values.
Records, primitive values, optional fields, dense arrays and maps with primitive
keys are supported. Unsupported or recursive readers are rejected (SELU0901).
Declared fields are the only fields application code may access; extra fields
remain opaque and survive serialization, including numeric versus string keys.
Existing array/map elements are validated without receiving defaults. Dense
arrays reject holes, while numeric maps preserve sparse keys.

Historical container repairs are explicit: `SavedRepair<T>` is the closed union
of a declared table shape `T`, `string`, `number`, and `boolean`. Add `?` for
absence. `T` must itself be a finite, closed serializable record, array or map;
functions, open types and recursive shapes are rejected. A `typeof` check on a
local input must select the table arm before table operations are allowed.
For example, toolkit migration 5 repairs non-table cooldown containers:

```selu
type Characters = Map<string, { ownedValue?: string; restoreValue?: string }>;
type Cooldown = { characters?: SavedRepair<Characters> };
// Historical reader for 4 -> 5:
// old: { cooldownText?: SavedRepair<Cooldown> }
const container = old.cooldownText;
if (typeof container != "table") { old.cooldownText = {}; }
const settings = old.cooldownText;
if (typeof settings == "table") {
    const characters = settings.characters;
    if (typeof characters != "table") {
        settings.characters = new Map<string, { ownedValue?: string; restoreValue?: string }>();
    }
}
```

Only declared fields admit these scalar alternatives. A table with malformed
character records still fails validation: migration 5 repairs the containers,
not those records. Historical input and next-version output validation remain
mandatory, and the final current schema is strictly validated before publication.
This is safer than validating only the final schema, which would hand unchecked
values to typed migration parameters and permit invalid intermediate commits.
The reader, including its repair union, is recorded in `selu.saved.toml`.

The optional second migration parameter is a typed snapshot of the root at
entry to this login's migration chain. Each step gets its own copy of this
snapshot, validated against that parameter's closed schema. It supports repairs
that need data removed by an earlier step. Failed steps leave the last successful
version in the original selected root. Each successful step commits after output
validation, retaining existing table identities and aliases. If an attempt stops after a successful step, its original snapshot is retained
under the reserved `__selu_saved_adoption_context` key (`format = 1`, `original`
contains the initial serialized root; `removals` lists deferred input paths). This is retry context, not another
version counter. A later Selu login reuses it while continuing from the mapped
last-success version. The key is removed on success. Malformed context is
refused without overwriting it. Source declarations cannot claim this key.

Defaults run only after all steps and validation succeed. Successful publication
keeps the selected root identity and retires an absent/aliased legacy name.
Distinct legacy roots are always retained. `selu.saved.toml` records the mapped
key, alias and historical readers; changing them is rejected (SELU0902).
SELU0900 covers invalid adoption options and alias collisions.

Deployment requires working native SavedVariables hydration. This contract cannot
detect a client that incorrectly omits existing globals; installers must refuse
known broken clients. Simulator checks do not establish native client hydration
or authorize replacement of player installations.

Future context readers protect their declared root fields and complete input
subtrees, including opaque extension fields. An earlier step's deletion within
such an input is deferred. The input stays intact in the last-success save until
the last context reader commits; deferred removals and its version advance occur
in that same transaction. This general rule lets an older loader retry its own
migration using intact legacy inputs after a Selu failure. It is deliberately
stronger retention than an in-place deletion order. Ordinary Lua master ignores
the namespaced retry key and preserves it as opaque data; only a subsequent
successful Selu load can remove that compiler-owned key.

A deferred input path cannot be repurposed before retirement: a conflicting
write at its consuming step fails without committing or erasing the new value
into defaults. Reuse that path in a later version after the reader has committed.

Default filling is also transactional: it runs on a copy that retains graph
provenance. The complete result is then strictly validated through every declared
field, including aliased records and maps, before committing. Incompatible aliases
are refused without changing the last-good graph or publishing writers.

Adoption refusals, migration failures and distinct legacy roots emit one warning
through the existing warning runtime. It names the database, phase or migration
step and reason; exceptions retain their error text. Validation warnings include
the field path and expected/actual kind. Warnings do not attach persistent writers
and respect the existing warning level settings.

Each validation pass memoises successful checks by table identity, schema identity
and missing-field mode. Active-path cycle detection remains separate. Shared
subgraphs are checked once per schema/mode instead of expanding every alias path;
different schemas still validate the same table independently. The strict pass
uses a fresh memo after default filling, so earlier successes cannot hide changes.

## Independent current schemas

A saved declaration may specify its current schema independently of defaults:

```selu
type Position = { p?: string; rp?: string; x?: number; y?: number };
type Settings = { fallbackPos: Position; value: string | number };
saved global DB using dbVersion: Settings = { fallbackPos: {}, value: 0 };
```

The annotation may also follow the saved name before version/adoption options.
Defaults must remain literal data and are checked against the schema; a factory
call is not a literal default. Optional fields can be omitted. Only keys present
in defaults are filled: `{p: "BOTTOM", x: 0}` retains absent `rp` and `y`.
Adoption validates the entire annotated schema before publication, including
exact union alternatives and literal values, without coercion. Schema history
records the annotation rather than the smaller default object. The existing
candidate-copy, alias validation, memoisation and transactional commit paths
remain in use.

## Ordinary declarations now fail closed

Ordinary declarations (including `saved perCharacter` and `saved global` without
`using`) expose the same `status`, optional `data`, and `whenReady` API as mapped
adoption. This changes the old direct-field API: replace `settings.enabled` with
access inside `settings.whenReady((data) => { ... })`, or null-check
`settings.data`. Register persistent writers inside the ready callback.

```selu
saved settings = { enabled: true };
function initialize() {
  settings.whenReady((data) => {
    on PLAYER_LOGIN() { print(data.enabled); }
  });
}
initialize();
```

The ordinary loader uses the reserved `__selu_version` key. It reads the restored
root only on its own addon's `ADDON_LOADED`. Missing globals start fresh at the
current version; populated unversioned roots start at version 1. Validation uses
the explicit schema, or the complete defaults-inferred schema when no annotation
is written. Union alternatives and literals are exact; no coercion occurs.
Missing defaulted keys are filled on a copy and the full result is validated
before publication, with the same table/schema/mode memoisation and independent
active-path cycle detection as mapped adoption.

Ordinary migration parameters now require explicit finite closed historical
schemas too. This removes the former unchecked historical input. Typed context
readers are allowed. Unlike the old ordinary loader's partial commit on migration
failure, the complete ordinary load attempt is transactional. Invalid metadata,
malformed data, failed migrations and future versions leave the client-restored
global graph, identity, aliases and version untouched. The result reports
`corrupt`, `failed`, or `newer`, with absent data; ready callbacks and their writers
are suppressed. A database-specific warning explains the refusal. Logout writes
the original retained value. A successful attempt commits while preserving
existing table identities, then publishes `ready` and invokes callbacks.

Mapped adoption retains its existing last-success/retry-context migration
contract. Ordinary refusal retries from the original untouched graph next login.

Saved Map keys may be finite unions of string, number and boolean types (including
literal alternatives). Validation checks each alternative exactly: `17`, `"17"`
and `true` remain distinct keys through iteration, migration and serialization.
Null, optional, table and open key types are rejected, including in historical
and `SavedRepair` schemas.

An empty record type `{}` describes an opaque serializable table. Its contents
must be finite serializable data with primitive keys; functions, cycles and
non-finite numbers are refused. Typed code may retain or move that table, but
cannot read arbitrary members or index it. It provides no dynamic lookup or
untyped access to game values. Additional fields of declared records remain
opaque as described above.

Annotated defaults receive their schema context immediately, including nested
empty arrays and optional fields. Only written defaults are filled; omitted
optional fields stay absent.
