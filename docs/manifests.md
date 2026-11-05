# Generated client manifests

The source clone is owner-maintained. After the owner fetches it, run:

```powershell
cargo run -p selu-manifest-gen -- update
cargo run -p selu-manifest-gen -- list manifests/store
cargo run -p selu-manifest-gen -- dump manifests/store wow@12.1.5.70783
```

The generator resolves immutable commits with read-only git operations; it never
fetches, checks out or writes to the clone. Temporary archives are streamed and
removed on completion. `migrate` imports pre-store single-build manifests, retaining
the six original build identities under their product codes. Single-file generation
and dump remain available for offline extraction probes and JSON fixtures.

## Schema and compatibility

Existing function names, `params` (names), restrictions, evidence, event payloads
and globals remain readable by the current checker. New `signature` declarations
hold typed function arguments and multiple returns, or typed event payloads.
`tables`, `enums` and `widgets` hold declarations. Widgets hold methods and an
`inherits` list. Canonical widget aliases in overlays connect `Frame` to
`SimpleFrameAPI` and the ScriptRegion groups; `CreateFontString` returns
`SimpleFontString`, which connects to the documented FontString methods.
Native method signatures and policies come from generated documentation; source member signatures are described below. The checker consumes these signatures; sourced iterator metadata also guides generic-loop lowering.

Declarations preserve Blizzard `type` names. Their `attributes` preserve metadata
keys such as `Nilable`, `Default`, `InnerType`, `KeyType`, `EnumValue`,
`MayReturnNothing`, secret policies and restriction flags. False and zero defaults
are retained. Parameter, field and return order is significant. Top-level entries
are sorted; attributes use sorted maps. Enum members are represented losslessly
as a `values` map from member name to value; the declaration preserves the enum
type, bounds and cardinality. Default-valued legacy fields are omitted (the loader
defaults them), while false and zero API policies remain explicit in attributes. The readable schema is JSON; generated files use the compiled container below.
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

## Product identities

Projects use Blizzard TACT product codes, optionally followed by a complete
`major.minor.patch.build` pin. The updater's generated table below records the
latest build of every discovered, mapped source branch. The six original pins
were imported before updating; no other historical source builds are backfilled.
Overlay directory names remain provenance identifiers from that migration.

## T1 typed globals

Global entries may carry an optional `type` string. It is omitted for names-only
entries and defaults to no type when loading older manifests. Global overlays
accept that same optional key. Retail and Forever type UIParent as Frame and
GameTooltip as GameTooltip, with source-evidence widget aliases; no live probe
is implied. The checker mapping and multiple-result rules are in lang/api.md.

T1 reviewed widget overlays also support `[[methods]]` with `widget`, `entry`,
`evidence`, `note` and optional `live_probe`. They replace one named method on an
existing widget, preserving its other methods. `Declaration.overloads` is an
additive list of complete argument/return declarations; omitted lists default
empty. Every overlay keeps a pinned source citation. `SeluAliasOf` in widget
attributes records an equivalent canonical identity.

## M3 XML and Lua data

`quick-xml` 0.42.0 (crates.io newest stable when implemented) parses XML;
addon Lua is still parsed with full_moon and never executed. Widget tag identities
and intrinsic aliases are also read from the pinned UI.xsd schema. Named nonvirtual
widgets become globals with name, widget type, template list (`templates`), and
source evidence. `$parent` uses the nearest enclosing named widget through layout
containers; unresolved template-parent names are omitted. Virtual and intrinsic definitions and their descendants are not definition-time
globals. Concrete instances receive named children inherited through template
chains, including nested child template inheritance. Cycles stop per traversal
path. Named Font/FontFamily objects remain globals even when virtual,
because their definitions create reusable font objects. Their type is `Font`.

`dataGlobals` and `templates` use compact source metadata: `name`, `source` (index
into the shared `sourceFiles` array), `evidence`, optional `type`, and optional
`fields`, `methods`, `inherits`, `mixins`, `values`. Fields and methods are name
lists, with no invented signatures. Templates record direct mixin members plus
inheritance links; consumers must traverse inherited templates. XML and Lua variants
are scanned without claiming TOC loading or live availability. Existing overlays
still take precedence. No checker changes are included.

Literal global table assignments outside functions exclude lexical locals. Named
top-level table method declarations supply mixin member names. Tables exceeding
2,048 constructor fields are omitted entirely: large generated map, quest and locale
payloads are datasets rather than addon API metadata. Literal assignments in paths
containing QuestData or MapData (case insensitive) are also skipped. Localisation
tables retain field names but no values. Nested table contents and
positional array payloads are never projected. Only all-uppercase constant-table
names retain literal number/string values, and only when their compact value map
is at most 65,536 bytes. Other tables retain names only. This keeps SOUNDKIT-like
constants while excluding bulky payloads and prose. Booleans, expressions and
functions never become values; comments, XML and documentation text are excluded.
Shared provenance and string name lists avoid repeating file paths on every member.

## Top-level scalar constants

Global entries may carry an optional `value` (JSON number/string); omitted values
remain backward compatible. The Lua AST scan also records bare global scalar
assignments outside functions, excluding lexical locals and table fields. Literal
numbers (including hex and negative numbers) retain their values. String values
are retained only for nonempty ASCII identifier-like tokens of at most 64
characters (`A-Z`, `a-z`, digits, `_`, `-`, `.`, `:`). Markup, prose, spaces and
control characters are excluded. Paths containing localization/localisation,
locale or GlobalStrings retain no scalar or table values, including numeric
values. Names and types remain available.
References and arithmetic derive number/string types from other discovered scalars
and documented Constants fields; concatenation is string. These expressions carry
no invented evaluated value. Unknown call results and other unproven scalar types
are omitted. This is static source evidence, including conditional assignments and
all source variants, rather than a claim about runtime initialisation. The first
available scalar type and first literal value in deterministic file/source order
are retained; scalar globals do not receive repeated per-entry provenance. Unlike
large table datasets, scalar assignments are scanned in every addon Lua path.

## Reviewed XML and mixin metadata

Intrinsic XML definitions are templates, including their definition-time children.
The generator expands inherited `$parent` child names for concrete instances;
child patterns remain internal to generation and no XML markup is emitted.
Top-level static `CreateFromMixins(A, B, ...)` and `Mixin(Target, A, B, ...)`
definitions record `dataGlobals[].inherits` links in argument order. Template
`mixins` links to these entries and their transitive inheritance remains
traversable without duplicating members. Local/shadowed helpers and dependencies,
dynamic arguments and function-body definitions are excluded.

Generated `globals` may contain groups: `{names: [...], type?, evidence,
templates?}`. All names share that metadata and no value is stored on a group.
Scalar entries with values retain the individual object form. The manifest
loader expands both forms into the same GlobalEntry list; existing manifests
and fixtures remain readable. This lossless grouping keeps inherited child
names below the 40 MB package cap. XML EOF with open elements is an error.

## Release updates

After the owner fetches the source clone, run `cargo run -p selu-manifest-gen -- update`.
It discovers tracking branches without fetching or checking out, skips automation,
and scans each branch's history for its newest build subject. Unknown branches
are reported for a verified mapping; they are never guessed. Optional product or
branch arguments select a subset. Existing segments and build identities are immutable.
`selu-manifest-gen list manifests/store` lists stored builds;
`selu-manifest-gen dump manifests/store wow@12.1.5.70783` dumps one build.
The updater refreshes the stored product table below. It never publishes.

Source Lua member declarations now emit qualified function signatures, which
existing template/mixin resolution consumes. Parameter names and variadic tails
come from the AST; contiguous EmmyLua parameter/return annotations supply types.
Unannotated types follow the existing untyped game value policy in lang/api.md;
unannotated parameters permit omission because Lua bodies may supply defaults.
A missing return annotation remains untyped rather than claiming a void result.
This does not type the Lua Mixin/CreateFromMixins helpers for Selu consumption.
## Compiled containers and history store

Single-build snapshots and derived caches retain Z's `.selum` encoding:
`SELUMAN\0`, format 1 and schema 1 (u16 little endian), then zstd level 3
MessagePack named maps. Decoding is bounded to 256 MiB. JSON fixtures remain readable.

History segments in `manifests/store/*.selus` are plain Git files, with no LFS
requirement. The unpublished initial pack was rebuilt from the six W2 snapshots
on master `719f404` and fresh generation of each branch's latest build. It contains
no pre-W2 snapshot at the retained pins. Each stored extraction revision remains immutable; the game-build identity can
gain newer extraction revisions. The bootstrap replacement above was explicitly
authorized before publication. They begin with `SELUSEG\0`, format 2 (legacy) or 3 (revision-aware), schema 1 (u16 little endian),
a 32-byte SHA-256 checksum of the complete remaining segment, and a u32 compressed
catalog length. The zstd-compressed MessagePack catalog contains build delta indexes
and entry-block offsets; independent zstd entry blocks follow. The checksum covers
the catalog length, catalog, and every compressed block. Readers reject unsupported
versions, truncation, bad checksums, invalid ranges, duplicate IDs, missing entries,
and missing/cyclic bases. Decompression is bounded to 256 MiB per block.

Entries have dense u32 IDs across append-only segments. SHA-256 is used only to
deduplicate canonical values while writing, never stored per reference. The first
build lists integer IDs; each later build copies ranges from its closest available
base and inserts only changed integer IDs. Same-product bases have priority; shared
entry overlap chooses the closest candidate, with deterministic ties. Builds within
a batch can use earlier builds in that batch. Arrays retain separate copy runs for
distant changes. Scalars use sections prefixed with `=`. Provenance uses source-file
entry IDs internally and is restored to each manifest's sourceFiles indices, so a
new source file does not duplicate unchanged records. The writer hashes existing
entries once per non-empty update, then appends one new segment containing only new
entries and new build indexes. Existing files are never rewritten. Repeated identical extraction updates add
zero bytes. Game builds can gain new immutable extraction revisions, described below.

Writers serialize with an exclusive OS lock on `manifests/store.lock`, next to the
store. The lock file is gitignored and keeps a stable identity; its handle releases
the lock automatically, including after a process exits. Writers wait at most 30
seconds, then return a retry error. After acquiring the lock, a writer reopens the
current store before assigning entry IDs. It writes and syncs a temporary file,
validates the combined store and pending reconstructions, and only then atomically
renames the file into place. Rejected candidates are removed while still temporary.
Generation may run outside the lock; publication always uses the refreshed store.

Production entry blocks contain at most 4,096 entries. Larger blocks were measured
and saved some space, but increased one-build cold load time. Opening the store reads
and verifies complete compressed segments and decodes their small catalogs; it does
not reconstruct manifests or decompress entry tables. A load resolves only the
requested build's delta-base index chain and decompresses only referenced blocks.
Those blocks and indexes are shared across requested builds within the load.

The store is authoritative. Optional derived `.selum` caches live under
`target/selu-manifest-cache/v4/<store-content-hash>/` beside the manifests workspace,
never inside project dist or AddOns. Each requested build has a file named by its
build-ID hash and a CRC32 sidecar. Every load checks the store's segment checksums,
the cache CRC32, and decoded build identity. Changed store content hashes select a
new cache directory. Missing or corrupt cache files rebuild only that requested
build; an unwritable cache still permits uncached loading. Cache snapshots use the
same versioned Z MessagePack/zstd container, with direct typed serialization.
No all-build cache population runs on first use.

## Extraction revisions and compiler improvements

A game build and an extraction revision are separate identities. `selu.lock` still
pins only `product@major.minor.patch.build`. Loading that game build selects its
newest stored extraction, so a compiler extraction fix can improve an existing pin.
No project target, output directory, TOC or lock key contains an extraction ID.

The generator's `EXTRACTION_VERSION` is re-exported from
`selu_manifest::store::EXTRACTION_VERSION`. It is **3 for the corrected W3 pool overlay** (2 is its retained initial revision; 1 is the W2 baseline). Bump this
constant whenever extraction output rules change, including sourced signatures or
overlay changes. The workspace test `extraction_sources_and_overlays_require_version_bump`
enforces this with a frozen SHA-256 contract per version in
`crates/selu-manifest-gen/tests/extraction_version.rs`. It discovers extractor Rust
modules and all overlays, and covers generation orchestration, manifest schema,
normalization and encoding inputs. Paths and contents are hashed in sorted order
with LF normalization. When changing these inputs, bump `EXTRACTION_VERSION` and
append the new version/digest to `CONTRACTS`; preserve previous entries. CLI-only
import/report changes do not affect the extraction contract. This is intentionally
conservative about changes within covered source files.

Extraction IDs are `v<version>-<sha256>`, where the hash covers the
named-MessagePack Manifest content in schema-1 serialization order. That order is
part of the hash protocol. The game build and content, not compression settings,
determine the ID. A version bump with unchanged content still creates a revision;
repeating the same version/content writes zero bytes.

Format-3 catalogs key each index by `(game build, extraction ID)` and record its
extractor version, content hash and monotonically allocated per-build ordinal.
Entries remain dense integers shared across all revisions. New revisions copy from
the closest stored index and insert only changed IDs, using the same compressed
append-only segments, integrity checks, exclusive writer lock and pre-publication
validation. A revision is immutable; an older ID always reconstructs its original
content. Selection uses highest numeric extractor version, then append order within
that version. Replaying an existing extraction or using an older extractor cannot
roll back the latest selection. Multiple different revisions for one game build in
a single append transaction are rejected as ambiguous.

The existing format-2 W2 pack stays byte-for-byte intact. Its game-build indexes are
read as extraction version 1, with content-derived audit IDs computed on demand.
New segments use format 3; the reader supports both. Ordinary loads do not reconstruct
all old builds to derive revision IDs. A new segment changes the store fingerprint,
which invalidates derived caches even when game-build pins did not change.

`selu-manifest-gen update [product...]` compares the stored extractor version with
the current constant. A bump regenerates unchanged branch builds and retained older
stored builds for those products, from their recorded source commits. It adds their
new revision deltas, with API diffs, without adding historical game builds. A normal
unchanged update with the same constant is a zero-byte no-op.

Standalone `.selum` and JSON files do not prove which extractor produced them.
`migrate` therefore treats them as unversioned. Under the exclusive writer lock,
an import matching an existing build's selected content is a zero-byte no-op;
different content is rejected with the game-build ID and a direction to run
`update`. Rejection publishes none of that import batch. A previously unseen build
can be bootstrapped as reserved revision **v0** (unknown extractor), which always
needs real re-extraction by `update` before it counts as current. Version 0 is
reserved for this import path; trusted generator appends require a positive version.
An import cannot create a newer revision of an existing build, even when its content
matches a historical revision. Only re-extraction or validated extractor provenance
can establish a current revision; `migrate` provides no provenance override.

Audit commands (run from the compiler workspace, or pass an explicit store):

```text
selu-manifest-gen revisions wow@12.1.5.70391
selu-manifest-gen revisions path/to/store wow@12.1.5.70391
selu-manifest-gen dump path/to/store wow@12.1.5.70391 --revision v1-<hash>
selu-manifest-gen extraction-report path/to/project path/to/store
```

`dump` without `--revision` reads the newest extraction. `revisions` lists all IDs and
marks the latest. `extraction-report` reads active game-build pins from `selu.lock`
(or exact project targets), reports their latest stored extraction transition, and
shows the complete API diff and candidate source uses using the same implementation
as lock updates. It does not modify the project or claim to remember which extractor
was last used locally. Newest extractions are already used automatically. For older
transitions, use the listed revision IDs with `dump --revision`. Exact game pins and
major-game-version confirmation behavior remain unchanged.

## Checked engine supplements and source factories (W)

Each flavour overlay adds the tooltip's zero-argument `NumLines` and `ClearLines`
with call-site and XSD citations at its existing pin. NumLines is numeric from
Blizzard's comparison. ClearLines has an untyped result because no return
contract is evidenced. Supplementary signatures are source evidence, not live
client probes. Review overlay citations when intentionally advancing pins.

The generator's optional fifth generation argument writes an engine-gap inventory
JSON to a report path, for example `E:/AgentData/reports/selu-W-retail-engine.json`.
It lists absent widget/method pairs for direct typed globals and lexical literal
CreateFrame receivers, after native inheritance, templates and overlays. Methods
with named or anonymous Lua implementations are listed separately as source or
ambiguous pairs, rather than claimed as missing engine APIs. Dynamic receivers,
fields, `self`, unresolved aliases and branch loading remain explicit limits.
Unresolved receiver calls are counted separately. This inventory is not a claim
that all dynamic widget types have been recovered.

A bounded source analysis follows lexical closures, local mixins, public aliases,
Mixin/CreateFromMixins and CreateAndInitFromMixin construction, and the pinned
ProxyUtil helpers' explicit exported method lists. It creates nominal checked
object signatures for proven factory results, including ScrollBox views and
DataProviders. Private helpers stay private. Analysis never executes Blizzard
Lua and does not expose Lua construction helpers to Selu. Unresolved operations,
recursive limits and dynamic return branches supply no factory proof.
Parameters and results without sufficient evidence retain the existing untyped
game-value policy. W2 follows uniquely indexed helper bodies within fixed bounds
and invalidates contracts when opaque calls or callbacks can reach the objects.
The conservative analysis still withholds pool contracts. W3 applies reviewed
source overlays in `pools.rs` through the same overlay validator. The pool source
must match a reviewed content digest; a changed implementation requires review.
Each pool widget retains pinned file/line citations in `SeluPoolEvidence`.
Acquire is nullable at capacity; object-pool elements remain untyped. Literal
frame types specialize the returned pool and its iterator/release contract.
Collections enumerate objects only. GetPool is nullable; unsecured GetOrCreatePool
also returns the new flag, while secure proxy forwarding retains only the pool.
Older Classic sources lack DoesObjectBelongToPool. Secure collection proxies list
that name without a defining implementation, so it is withheld there as well.
Factory objects and mixins are source metadata, not a TOC-loading guarantee.

`SeluIterator` in return attributes records source-proven Lua iterator yields.
Direct pairs/ipairs results and consistent forwarding signatures preserve this
contract. Pool proxy enumerators keep their source allocator element type where
proven. Selu lowers `for (const [first, second] of pool.EnumerateActive())` directly
to a Lua generic for, preserving iterator/state/control results. Iterators are a
distinct internal type, not arrays, maps or scalar unions. Call the enumerator in
the loop expression: saving one result would lose Lua's state/control values.
The same rule covers ScrollBox EnumerateFrames calls. No array allocation or
iterator wrapper is generated.
<!-- selu-store-builds:start -->
## Stored product builds

| Product | Branch | Full build | Interface | Commit |
|---|---|---|---|---|
| `wow_anniversary` | `classic_anniversary` | 2.5.6.70577 | 20506 | `68e0a2ec559d9761dfc4e7f864c3944f9d758a49` |
| `wow_beta` | `beta` | 12.0.1.66220 | 120001 | `078d0cf7512bfc9c83c576ae83d14ef6a173ea3b` |
| `wow_classic_beta` | `classic_beta` | 5.5.0.62071 | 50500 | `997d2ecc801c502c2c71ba467c07c84e5b5fa12d` |
| `wow_classic_era_ptr` | `classic_era_ptr` | 2.5.6.70006 | 20506 | `31702bfe83af187a4344eb6934d64ba1904238ad` |
| `wow_classic_era` | `classic_era` | 1.15.9.70576 | 11509 | `4192ac60067fc55566182291bc67c85dd9b6794d` |
| `wow_classic_ptr` | `classic_ptr` | 5.5.4.67849 | 50504 | `ea6f4ee4ce0d457c02f479787293b702c062f067` |
| `wow_classic_titan` | `classic_titan` | 3.80.2.70522 | 38002 | `7014c309a25338905c41dc3799a93f9719ebb533` |
| `wow_classic` | `classic` | 5.5.4.70631 | 50504 | `e5d9b3a22c3f0f6f41ebac0e454f4969594261cf` |
| `wow` | `live` | 12.1.5.70783 | 120105 | `297d7aaaa736f346b3e036f38ee088eaf7f8c07f` |
| `wowf` | `forever` | 1.60.1.70790 | 16001 | `eac30af597e5aaa43a60f6675ef5fdcea100375f` |
| `wowt` | `ptr` | 12.1.7.70668 | 120107 | `9fc0ba94381153262856ed0120a2675f1a11c30a` |
| `wowxptr` | `ptr2` | 12.1.5.70340 | 120105 | `12380f8cdc541b7f0963f74cbe0874c21a857fbb` |
<!-- selu-store-builds:end -->
