# Typed game API

Manifest entries with signatures are checked; names-only entries remain untyped.
Explicit overlay signatures are usable even when their provenance is source
rather than generated documentation. Diagnostics SELU0480/0481 report arity and
parameter type mismatches and name the target. SELU0482/0483 report unknown widget
methods/types with spelling suggestions; SELU0484 reports excess event parameters
and SELU0486 rejects an incompatible written payload annotation.
Aliases and imported function values retain the multiple-results convention in
their function type. Selu tuple functions retain SELU0377 and require destructuring.

An API call in a value context selects its first result; destructuring reads all
positions without allocating a tuple table. A call contributes exactly one value
in arguments, array literals and scalar returns, including the last position.
The lowering parenthesizes those calls. Names-only calls also receive Lua result
adjustment in scalar contexts. Function identity is never inferred from spelling.
Empty results read as null; Nilable makes an individual result optional, and
MayReturnNothing makes every position optional. False remains false.

Primitive mapping: string/cstring/WOWGUID and unit tokens are strings; bool is
boolean; documented numeric aliases such as uiUnit, WOWMONEY, fileID, Seconds
and BigUInteger are numbers. luaIndex is a 1-based number; no conversion is performed.
Structures become object shapes, callbacks become functions, and table InnerType
metadata describes arrays. Enum.X.Y has type number; enum parameters accept numbers.
Widget types are nominal, carrying their transitive inherited identities.
CreateFrame with a literal type selects that widget; a dynamic type returns Frame.
Factories use their documented return widget. Inherited methods keep Lua colon
calls. Typed global entries select widget types without hard-coded global names.

The optional type on manifest global entries is additive: old fixture entries
remain untyped. Retail and Forever overlays type UIParent and GameTooltip and
connect GameTooltip to FrameAPITooltip and Frame. These entries have source
evidence, not live verification. Regeneration uses the original pinned commits.

The simulator models region parent identity, size, stored anchor points and text.
Texture and font-string factories return distinct child objects. This is offline
behavioral proof; actual client rendering and live API policy acceptance remain
with the owner.

## Scalar unions and opaque engine values

One metadata table in api.rs supplies the accepted scalar members for named
scalar unions. Arguments may be any member. Returned union values fit a target
only when every possible member fits that target. Only an engine type with an actual documentation-table definition and no Selu
counterpart remains nominal. A referenced name without a definition is untyped,
accepts anything, and interoperates with other types in both directions. A false
error is worse than a missing check (owner decision, 2026-11-04). This applies to
ItemInfo, ClubId, ClubStreamId and other undefined names; their use in a signature
is not itself a type definition. Documented structures, enums, widgets, callbacks,
primitive aliases and the sourced scalar-union table keep their existing types.

| API type | Members | Source |
|---|---|---|
| SpellIdentifier | number, string | Pinned generated SpellDocumentation.lua, GetSpellInfo argument documentation (ID, name or link) |
| uiAddon | number, string | Pinned AddOnsDocumentation.lua argument tables and Blizzard addon name/index calling convention |

String-like aliases include FramePoint, DrawLayer, unit token variants,
textureAtlas, textureKit and stringView; numeric aliases include uiUnit, luaIndex,
fileID, Seconds and SingleColorValue. These map to scalar types before opaque
fallback. Generated tables retain structure identity before any fallback.
Documented ScriptObject method groups named XAPI also resolve factory results X.
The report accompanying this change lists every remaining opaque name and its
occurrence count for each committed manifest.

Multiple-result functions cannot be assigned to single-result function types,
including callback parameters and collection callbacks (SELU0485). Wrap the call,
for example `(unit) => UnitName(unit)`, to expose exactly one result. A plain
unannotated alias keeps its original result convention. This compatibility rule
is shared with collection callback checking. API origin metadata contains names
and target solely for diagnostics; it compares equal regardless of content and
has no role in assignability, joins or lowering.

LuaFunctionReference is an unspecified callback signature: any function value is
accepted, parameter context is untyped, and its arity/result are unrestricted.
Non-functions are rejected; Nilable still permits null to clear SetScript.
Follow-up outside T1: a data table keyed by script name could give OnShow `(self)`
and OnEvent `(self, event, ...)` their own contextual signatures.

## Incomplete client metadata: advisory API checking

A false error blocks porting real addons; a missing check does not. API signatures
and widget method groups omit templates, mixins, overloads and optional parameters.
SELU0480/0481 therefore warn by default. `[build] strict_api = true` in selu.toml
promotes these signature mismatches to errors. Selu functions, classes and typed
values retain strict checking. Missing API availability and multiple-result
contracts retain their existing rules.

Widget values are open: addon fields can always be written, and unknown members
are untyped. Only a member within edit distance 1 or 2 of an inherited known
method warns (SELU0482). Unknown CreateFrame literals warn (SELU0483) and produce
open Frame, including in strict mode. Number arguments satisfy API string
parameters because the client converts them; return types and Selu string rules
are unchanged.

The generated manifests carry explicit sourced method overloads. Overlays add
canonical frame names and complete region/texture/animation ancestry from the
pinned client's method groups and UI call sites. SeluAliasOf records equivalent
widget identities, including SimpleFrame/Frame. CreateFrame specialisation and
overload signatures live in identity-neutral semantic function information,
separate from diagnostic origin, and survive aliases/imports.

Single-value parentheses are printed only where Lua could expand a final call:
last arguments, final positional table fields and final return operands. Operators,
keyed fields, earlier list operands and single-target stores already truncate.
The simulator retains CreateFrame's parent and exposes it through GetParent.
