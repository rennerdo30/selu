# Generated client manifests

Regenerate from the owner-maintained source clone without fetching or changing it:

```powershell
cargo run -p selu-manifest-gen -- retail 18c6d677b605b312f3d813d498404631fb941977 manifests/retail/12.1.5.70391.json E:/Development/wow-ui-source
cargo run -p selu-manifest-gen -- forever ca8368f2f2380cbe696854f7f2646387c278a61b manifests/forever/1.60.1.70532.json E:/Development/wow-ui-source
```

The ref may be any local ref or commit. For reproducibility use immutable hashes,
as above: even remote-tracking refs move, and local branches may be stale. The generator prints the resolved commit and uses that immutable
hash for every subsequent read. Version and build are parsed from the commit
subject `version (build)`. `git log` and `git archive` read the clone. Archives
are owned temporary files under `E:/AgentData/claude_temp/m1` and are removed
on completion; archive entries are streamed without extracting addon source.
The output filename should be the printed build under the flavour directory.
The generator never fetches, checks out, or writes to the clone.

## Schema and compatibility

Existing function names, `params` (names), restrictions, evidence, event payloads
and globals remain readable by the current checker. New `signature` declarations
hold typed function arguments and multiple returns, or typed event payloads.
`tables`, `enums` and `widgets` hold declarations. Widgets hold methods and an
`inherits` list. Canonical widget aliases in overlays connect `Frame` to
`SimpleFrameAPI` and the ScriptRegion groups; `CreateFontString` returns
`SimpleFontString`, which connects to the documented FontString methods.
All method signatures and policies come from generated documentation. No checker or lowering behavior changes.

Declarations preserve Blizzard `type` names. Their `attributes` preserve metadata
keys such as `Nilable`, `Default`, `InnerType`, `KeyType`, `EnumValue`,
`MayReturnNothing`, secret policies and restriction flags. False and zero defaults
are retained. Parameter, field and return order is significant. Top-level entries
are sorted; attributes use sorted maps. Enum members are represented losslessly
as a `values` map from member name to value; the declaration preserves the enum
type, bounds and cardinality. Default-valued legacy fields are omitted (the loader
defaults them), while false and zero API policies remain explicit in attributes. JSON is compact UTF-8 with one LF terminator.
Top-level `source` records the exact commit, version and interface provenance.
`predicates` retains named preconditions and failure modes; their names define
additional recognized policy attributes, including `TransliteratorAllowed`.
System `Environment` is inherited by exported declarations unless overridden.
Secure-only functions remain listed with `available:false` and restriction
`SecureOnly`. The unchanged checker rejects calls with SELU0301; detailed reason
metadata is in the manifest (the current diagnostic wording is generic).
`returnsSecret` means a return may be secret, including conditional secrecy;
false policies never make it true. `restrictions` lists active documented
preconditions/protection flags. Raw policies remain in the signature.


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
`[interface]` with `value`, `basis` and `note`; `[[functions]]` and `[[widgets]]`
with `entry` and `note`; `[[globals]]` with `name` and `note`. Every overlay entry
requires explicit `evidence` (`documented`, `source`, `verified`). Verified evidence
requires a nonempty `live_probe` record. Entries replace matching
names. Notes must be nonempty, and unknown structural keys are errors.
The declaration `attributes` map is deliberately open for client policy extensions;
overlay authors own its spelling and metadata-only contents. Overlay notes remain
in TOML; generated JSON contains API metadata only, except source provenance.
The Forever interface is 16001 from the owner's client handbook (`basis:source`).
Retail 120105 is convention-based (`interfaceBasis:convention`), not read from a
client TOC or confirmed by a live probe. Both interface records use source evidence.

Lua runs without libraries or host globals, in a separate environment, with a
32 MiB allocation limit and a bounded instruction hook. Documentation registration
is captured, enums and constants resolve dependencies over repeated passes.
Engine-only enum/constant values unavailable in this source remain
`{"reference":"Constants.Name.Value"}` (including arithmetic expressions), never
guessed numeric values. Regeneration fails for other unresolved dependencies.
Only structural keys are projected; `Documentation`, comments and source are
excluded. Handwritten tests include a prose sentinel to prove exclusion.
The generated surface is evidence of documentation, not live client behavior or
proof that undocumented APIs are absent. Undocumented globals come from an AST scan of all `Interface/AddOns/**/*.lua`.
The full_moon Lua 5.1 parser identifies global function declarations and anonymous
function assignments outside functions, excluding lexical locals and field/method
assignments. Direct bare-name calls without any global function definition in the
branch are collected separately. Parameters, local functions, block scopes, loop
variables and repeat conditions are accounted for; strings/comments are never
searched. Parse errors fail generation. A non-UTF8 source file uses a byte-preserving
Latin-1 mapping for parsing; only identifier names and signatures are retained.

Evidence levels are `documented`, `source`, and `verified`. Discovered entries
have `evidence:source`, which records static UI evidence without claiming a live probe.
A function's `source` contains `kind` (`defined` or `called`) and the first file,
relative to `Interface/AddOns/`. Defined functions have parameter names (so fixed
arity is their count, excluding a trailing `...`); call-only functions have no
inferred arity or types. Already-documented functions take precedence, including
unavailable secure entries. This scan covers all game variants in the branch and
does not establish TOC loading or live availability; source evidence does not
supply a documented signature or live probe. Native globals do not have to be
hand-listed. Overlays still supply the CreateFrame return type and canonical widget
connections. Enum and Constants are known namespace roots.
Existing handmade fixtures stay intact.
