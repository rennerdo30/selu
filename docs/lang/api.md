# Typed game API

Manifest entries with signatures are checked; names-only entries remain untyped.
Explicit overlay signatures are usable even when their provenance is source
rather than generated documentation. Diagnostics SELU0480/0481 report arity and
parameter type mismatches and name the target. SELU0582/0483 report unknown widget
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
Reviewed literal script names select sourced callback signatures. Unreviewed
names retain the existing unspecified callback behavior.

## Incomplete client metadata: advisory API checking

A false error blocks porting real addons; a missing check does not. API signatures
and widget method groups omit templates, mixins, overloads and optional parameters.
SELU0480/0481 therefore warn by default. `[build] strict_api = true` in selu.toml
promotes these signature mismatches to errors. Selu functions, classes and typed
values retain strict checking. Missing API availability and multiple-result
contracts retain their existing rules.

Widget reads, writes and calls require declared members. Undeclared members
report SELU0582 even after annotations or other widening; source-only mixin
names without signatures do not grant an unchecked member. Keep addon state in
typed objects or classes and use frameFamily suffix lookups for child globals.
Unknown CreateFrame literals warn (SELU0483) and produce the declared Frame
type, including in strict mode. Number arguments satisfy API string
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

Native API callbacks may ignore trailing arguments supplied by the game. A
zero-parameter callback is valid for NewTicker. Every declared parameter is still
checked positionally against the supplied type, including rest parameters; a
callback requiring more arguments than the native signature guarantees remains
SELU0481. Result-convention checks still apply. This rule applies to callback
arguments of native APIs; ordinary Selu function assignment keeps its existing
contract.


## Opaque native callback payloads

A reviewed callback slot with no evidenced payload type is `opaque<slot>` in
checker diagnostics. This is a distinct internal type, not permissive Untyped
and not a Selu annotation spelling. Declared CallbackType parameters whose
types are missing or unresolved become opaque by default. `SeluOpaque` gives
reviewed slots stable identities for sourced round trips. The existing policy
for other incomplete API results and unspecified script signatures is unchanged.

An opaque value may be ignored or kept in an inferred local. It cannot satisfy
a concrete annotation or assignment, expose a field, or pass through an
untyped/unknown annotation (including inside a container), or launder its type
through an untyped call. These boundary
violations are errors even with advisory API checking. A runtime `typeof`
check proves the tested scalar type; `!= null` only removes possible absence,
not the requirement to prove a scalar or object shape.

```selu
StaticPopupDialogs.set("TEST", {
    OnShow: (self, data) => {
        if (typeof data == "number") {
            const count: number = data;
        }
        StaticPopup_Show("NEXT", null, null, data);
    }
});
```

The last call is a sourced round trip to the matching StaticPopup data slot.
`SeluOpaqueInput` records such native input capabilities independently of API
origin labels. Ordinary arguments still use that API's original parameter
signature; this does not make other values valid for SetAttribute, for example.
Extra unannotated callback parameters beyond a finite native signature infer
null, rather than creating unchecked values; explicit defaults retain their
ordinary inference. Different opaque slots cannot be substituted for each other. Immutable native
function aliases keep proven capabilities; mutable function slots and uncertain
function joins discard native input capabilities. Known script boundaries remain
opaque through those aliases, while their receiver, scalar and arity promises
are withheld. Use an immutable registration alias to retain precise context. Secret value policies remain independent.

OnClick's down argument is `boolean?`, including Blizzard handler invocations
that omit it. The reviewed motion and key payloads have no evidenced scalar
contract, so consumers must check them before using boolean/string operations.
Popup flags follow stored native values: numeric flags are numbers, and fields
with evidenced boolean and numeric values use their sourced scalar union.

### Repeating results are arrays

A function that returns one repeating value (the API data marks it with
`StrideIndex = 1` and it is the only result) returns an array in Selu. The
compiler collects the results: `frame.GetRegions()` becomes
`{ frame:GetRegions() }`.

```selu
for (const secret of frame.GetRegions()) {
    const region = readable(secret);
    if (region != null && readable(region.IsObjectType("Texture"))) {
        region.SetAlpha(0);
    }
}
```

`GetRegions()` gives `Region` values and `GetChildren()` gives `Frame`
values. Both are marked as possibly secret by the API data, so each element
needs `readable(...)` before use; narrow it with `IsObjectType` to reach
texture or font string methods. An empty result is an empty array.

### Checked native object and payload guards

A callback payload narrowed by `typeof value == "table"` permits field reads.
Each field is another opaque value: snapshot it and test its runtime type before
using it as a string, number or other checked value. Fields remain read-only;
the table test establishes neither a writable schema nor a native round-trip
capability for a field. This permits checking popup `data.itemGUID` first and
retaining the original GUID precedence before trying another source.

On a widget receiver, `readable(obj.IsObjectType("Frame"))` and
`readable(obj.GetObjectType()) == "Frame"` establish Frame on the matching branch when Frame
is a manifest-known subtype of the receiver. The identity methods retain their
documented ObjectType secret policy; readable is necessary on these clients.
The ordinary write and call
invalidation rules apply. Unknown names and structural objects establish no
widget identity. A readable parent therefore needs a non-null check followed
by a widget guard before using frame-only methods.

`GetBagID` belongs to the sourced `ContainerFrameItemButtonMixin`, reachable
through `ContainerFrameItemButtonTemplate`; a plain Button has no such contract.
Font globals and documented SimpleFont parameters share the documented font
interface. The parameter remains specific to that interface.

## Colour and vector values

The API documentation names colour and vector values (`colorRGB`, `colorRGBA`,
`vector2`, `vector3`) without defining them. They are the objects the client's
own factories make (`CreateColor`, `CreateVector2D`, `CreateVector3D`), so they
take that object's type from the manifest: pass `CreateColor(r, g, b, a)` or
`CreateVector2D(x, y)` where an API wants one, and read a returned value with
`const [r, g, b] = color.GetRGB();` or `const [x, y] = position.GetXY();`. The
overlays type these getters as numbers.

The overlays also type `GetNumClasses()`, `InCombatLockdown()` and, where the
client has user waypoints, the `UiMapPoint` value
(`{ uiMapID, position: CreateVector2D(x, y) }`).
