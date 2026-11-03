# Saved data

`saved name = { ... };` declares addon-wide data; `saved perCharacter name`
uses per-character storage. Versions are positive integers, defaulting to 1.
Declarations and migrations belong at file scope. Exported saved objects are
live bindings: importers read the object installed by the loader, not an early
copy. The root binding cannot be reassigned; mutate its fields instead.

The Lua global is `Selu_<addon hex>_<module path hex>_<name hex>`, with UTF-8
bytes encoded as lowercase hex. Addon names are the loader's installed folder
name, which must match the build's addon name. Components cannot collide through
punctuation or case folding; different addon names and module paths stay distinct.
`__selu_version` at the object's root is reserved metadata. Migration parameter
names and saved names also obey the Lua keyword and `__selu` prefix restrictions.

The private addon namespace's numeric slot 2 owns the saved runtime: copy/fill
helpers, initialization callbacks and one ADDON_LOADED frame. Files share that
runtime. Each file captures only the Lua globals used by its generated saved
runtime before user declarations; user locals cannot shadow those dependencies.
The initialization callback ignores other addons' ADDON_LOADED events and
unregisters itself after its own. There is no idle updater.

Read saved data in PLAYER_LOGIN or a later event. Top-level direct reads and
writes (including imported values and member roots) are SELU0460. Functions may
refer to saved values, but callers must wait until loading completes. An arbitrary
ADDON_LOADED handler may run before initialization because event-frame order is
not guaranteed; PLAYER_LOGIN is the first guaranteed event for addon code.

Defaults are literal serializable objects/arrays, primitives and empty typed
Maps. Each missing value is deeply copied; recursive filling tests `nil`, never
truthiness. Existing values, including false, remain intact. Functions, aliases,
nonempty constructor calls and computed defaults are rejected. Arrays and Maps
are ordinary persisted Lua tables; missing nested entries receive independent
copies. The simulator snapshots plain values into Rust-owned data and restores
fresh tables into a fresh VM; cycles and unsupported values fail explicitly.

A migration is written after its saved declaration:

```selu
saved settings version 2 = { enabled: true, thresholdPercent: 35 };
migrate settings 1 -> 2 (old) {
    old.thresholdPercent = 35;
}
```

Keep exactly one migration for every adjacent version from 1 to the current
version. Migration bodies use normal Selu statements, with `old` an untyped
historical object. Each attempt receives a deep copy of the last committed
object. On success the new version and object replace the global together.
Failure logs a warning through print, stops that object's chain and retains the
last successful object/version, so the next login retries. Defaults run only
after the chain succeeds. Newer-version data is published unchanged with a
warning and receives neither migrations nor defaults. Migration authors should
only change `old`; external side effects cannot be rolled back.

Every successful project build writes deterministic `selu.saved.toml`. Commit
it. Field paths use length-prefixed segments to distinguish punctuation in keys.
Adding object fields with defaults is compatible. Removing/retyping fields needs
a higher version and a complete migration chain. Version rollback, declaration
removal and malformed history fail without replacing history. Keep historical
migrations in source. Generated Lua and schema examples are in the D2 handoff
report and `examples/saved-settings/`.

Simulator helpers: `load_addon` now fires ADDON_LOADED after the last file;
`logout_saved(names)` fires PLAYER_LOGOUT and captures only listed globals;
`Sim::login_saved(addon, files, snapshot)` restores them into a new VM before
loading files. `SavedSnapshot` exposes the persisted tree for exact assertions.
