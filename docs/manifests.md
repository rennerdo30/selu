# Generated client manifests

Regenerate from the owner-maintained source clone without fetching or changing it:

```powershell
cargo run -p selu-manifest-gen -- retail origin/live manifests/retail/12.1.5.70391.json E:/Development/wow-ui-source
cargo run -p selu-manifest-gen -- forever origin/forever manifests/forever/1.60.1.70532.json E:/Development/wow-ui-source
```

The ref may be any local ref or commit. Prefer remote-tracking refs: local branches
may be stale. The generator prints the resolved commit and uses that immutable
hash for every subsequent read. Version and build are parsed from the commit
subject `version (build)`. Only `git log`, `ls-tree` and `show` read the clone.
The output filename should be the printed build under the flavour directory.
The generator never fetches, checks out, or writes to the clone.

## Schema and compatibility

Existing function names, `params` (names), restrictions, evidence, event payloads
and globals remain readable by the current checker. New `signature` declarations
hold typed function arguments and multiple returns, or typed event payloads.
`tables`, `enums` and `widgets` hold declarations. Widgets hold methods and an
`inherits` list; missing inheritance in generated documentation is not inferred.
Overlays can supply known inheritance. No checker or lowering behavior changes.

Declarations preserve Blizzard `type` names. Their `attributes` preserve metadata
keys such as `Nilable`, `Default`, `InnerType`, `KeyType`, `EnumValue`,
`MayReturnNothing`, secret policies and restriction flags. False and zero defaults
are retained. Parameter, field and return order is significant. Top-level entries
are sorted; attributes use sorted maps. JSON is compact UTF-8 with one LF terminator.
`source` records the exact commit, version and interface provenance.

Future checker mapping: `bool` -> boolean; `string`/`cstring`/`WOWGUID` -> string;
`number`, `uiUnit`, `WOWMONEY` and numeric identifier aliases -> number, while
retaining their nominal names. `luaIndex` remains a distinct 1-based API index,
never a Selu array index. `table` plus `InnerType` describes a collection;
structures become object types, enumerations retain values, script objects become
widget types, and callback types keep their names. Unknown named types remain
named, never silently converted to number. `Nilable` adds nil; optional defaults
are independent of nilability. Multiple returns retain positions and nilability;
`MayReturnNothing` marks the separate no-return case. These mappings are recorded
for the next checker round, not wired into checking in M1.

## Overlays and provenance

`overlay.toml` beside the output is merged after generation. Supported entries:
`[interface]` with `value` and `note`; `[[functions]]` and `[[widgets]]` with
`entry` and `note`; `[[globals]]` with `name` and `note`. Entries replace matching
names. Notes must be nonempty, and unknown keys are errors. Overlay notes remain
in TOML; generated JSON contains API metadata only, except source provenance.
The Forever interface is 16001 from the owner's verified client handbook.

Lua runs without libraries or host globals, in a separate environment, with a
32 MiB allocation limit and a bounded instruction hook. Documentation registration
is captured, enums and constants resolve dependencies over repeated passes.
Engine-only enum/constant values unavailable in this source remain
`{"reference":"Constants.Name.Value"}` (including arithmetic expressions), never
guessed numeric values. Regeneration fails for other unresolved dependencies.
Only structural keys are projected; `Documentation`, comments and source are
excluded. Handwritten tests include a prose sentinel to prove exclusion.
The generated surface is evidence of documentation, not live client behavior or
proof that undocumented APIs are absent. Engine globals and missing base widget
facts are explicitly sourced in overlays. Existing handmade fixtures stay intact.
