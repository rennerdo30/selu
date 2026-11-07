# Secret values and combat lockdown

**Coverage is limited to sourced manifest flags.** Familiar protected globals
such as `CastSpellByName`, `TargetUnit` and `UseAction` are not flagged in the
pinned documentation, so Selu does not defer or check them. Classic manifests
flag zero protected globals. Waiting can help a combat restriction; it does not
grant secure execution or hardware-event permission. This feature is not a
complete protection or taint checker.

## Opaque secret values

A secret-capable result is `Secret<T>`, not a missing value. It can be stored in
locals, fields, arrays and maps, returned, and passed to Selu parameters written
`Secret<T>`. Normal nullability remains independent: a possibly absent string
may be `Secret<string?>`; dense collections still prohibit absent elements.
A declared secret result with an untyped inner type retains that untyped inner
type inside Secret<T>; its known secrecy cannot be erased by missing type data.
Names-only APIs without result declarations remain untyped.

```selu
const name: Secret<string> = UnitName("player");
const label = CreateFrame("Frame").CreateFontString();
label.SetText(name);                    // passes the original value through
print("Hello " + (name ?? "Player")); // explicitly reads an accessible value
const visible = readable(name);        // string?
if (visible != null) { print(visible); }
```

No guard is emitted at the API call. `value ?? fallback` tests accessibility and
uses the fallback for an inaccessible or absent value. `readable(value)` returns
`T?`, with null for an inaccessible value. Both preserve false and evaluate the
value once; fallbacks remain lazy. A comparison, condition, arithmetic, string
operation, or non-secret parameter cannot consume an opaque secret (SELU0601),
with help showing `?? fallback` and `readable(...)`. A plain value can be stored
in `Secret<T>`, but the annotation keeps it opaque until explicitly read.

API pass-through uses `SecretArguments = "AllowedWhenTainted"` or
`"AllowedWhenUntainted"`. The qualifiers remain engine permissions; Selu does
not infer taint or secure execution. Missing/NotAllowed policies do not allow an
opaque argument. Immutable aliases retain permissions; mutable API function
slots conservatively require explicit reads because another function may be
stored there. Selu parameters explicitly written Secret<T> retain their contract.

`UnitName` yields two Secret<string> results on retail/Forever and ordinary
strings on single-target classic builds. `UnitHealth` is secret-capable on retail
and Forever too. Function SecretReturns/SecretWhen.../SecretIn... policies apply
per result; explicit SecretValue/ConditionalSecret positions refine the policy,
and NeverSecret excludes a slot. SecretReturnsForAspect marks getter results too
(for example FontString.GetText and Frame.GetID). Nilable and MayReturnNothing
remain independent. ConditionalSecretContents describes table contents and is
not recursively sanitized.

Every requested target is checked. Secret<T> collapses to T when only clients
without secret capabilities are selected (the four current classic flavours).
Several targets retain the strictest secrecy/protection policy regardless of
order, including getter aspects. Argument permission must hold on every selected
secret-capable client that declares that API; clients without secrets do not
need a secret-argument flag. `has(...)` proves availability, not readability.
Conditional function values union restriction policies and intersect argument
permissions. Generic aliases retain the identity of their secret element type.

## Combat lockdown

Global functions with IsProtectedFunction=true defer while InCombatLockdown is
true. For widget methods, this flag describes an operation restricted **on a
protected receiver**. It does not make every ordinary addon frame protected.

A frame created without a template, or with a literal comma-separated list
containing no Secure* template, carries plain provenance. Immutable values and
plain child-region factories retain it. Their method calls stay direct, with no
combat or IsProtected check. Unknown templates, Secure* templates, Blizzard
frames, mutable storage and mixed plain/unknown joins cannot retain that proof.
Their flagged methods evaluate the receiver once and use the equivalent of:

```lua
if InCombatLockdown() then
  local protected = receiver:IsProtected()
  if (canaccessvalue and not canaccessvalue(protected)) or protected then
    return defer(receiver.Method, receiver, ...)
  end
end
return receiver:Method(...)
```

Thus an unknown receiver that is plain still runs during combat; protected
receivers wait. IsProtected itself has an ObjectSecurity secret aspect, so the
internal read point defaults to deferral when that boolean is inaccessible.
IsProtected includes inherited protection, unlike its second
explicit-protection return. SELU0600 rejects a used result only when deferral is
possible. `const x = CreateFrame("Frame").ClearAttribute("x");` does not get it.
All execution contexts may run during combat, including load-on-demand setup,
ADDON_LOADED, PLAYER_LOGIN, events, commands, hooks, timers and click handlers.

Mutable function slots (locals, object fields and collection elements) keep a
conservative may-be-protected policy instead of trusting their initial value.
When the signature can hold a protected function, calls defer during combat.
This deliberately allows unnecessary deferral, and does not depend on function
identity remaining stable after hooks. Unspecified function references can also
hold widget operations, including on classic clients with zero flagged globals.
Immutable known global calls use their policy directly. Used potentially deferred
results remain errors. Unmarked globals and untyped API operations remain unchecked.

One FIFO and one PLAYER_REGEN_ENABLED registration live in the shared addon
namespace `__selu_ns` (the addon table supplied as the second chunk argument).
The first using module initializes them. Function, receiver, arguments and arity
are captured at enqueue time, preserving nil arguments. Outside combat the paths
allocate no per-call table or closure. Drain checks combat before each item and
pauses if combat resumes. Each item runs under pcall; errors go to geterrorhandler
under protection and do not strand subsequent items. Re-deferred work appends
behind existing pending items. Nothing persists across reloads.

`[build] debug = true` logs each enqueue once without values. Package K can
relocate the self-contained emit_restriction_runtime function later; queue sharing
already works across modules now.

## Source evidence and gaps

Only git show, git ls-tree and git grep were used in the read-only
E:/Development/wow-ui-source clone. These are source findings, not live acceptance.
All paths below are relative to that clone; refs are the manifest source commits.

| Flavour | Ref |
|---|---|
| retail | 18c6d677b605b312f3d813d498404631fb941977 |
| forever | ca8368f2f2380cbe696854f7f2646387c278a61b |
| classic-era | 4192ac60067fc55566182291bc67c85dd9b6794d |
| classic-anniversary | 68e0a2ec559d9761dfc4e7f864c3944f9d758a49 |
| classic-titan | 7014c309a25338905c41dc3799a93f9719ebb533 |
| classic | e5d9b3a22c3f0f6f41ebac0e454f4969594261cf |

At all six refs, Interface/AddOns/Blizzard_SharedXML/Dump.lua:106,113 tests
canaccessvalue before inspecting values. That is Selu's read-point predicate;
a target without an available predicate receives no guard. Classic-only builds
have no secret types and need no guards. Forever Dump.lua:312 also uses
issecretvalue for formatting.

At the retail ref, under Interface/AddOns/Blizzard_APIDocumentationGenerated:
- UnitDocumentation.lua records SecretReturns, conditional policies and
  NeverSecret result positions (UnitName, UnitHealth, UnitClass, UnitCastingInfo).
- SimpleFontStringAPIDocumentation.lua:664-671 marks SetText's SecretArguments
  AllowedWhenTainted, and SecretArgumentsAddAspect Text. Its GetText, and
  SimpleFrameAPIDocumentation.lua's GetID, use SecretReturnsForAspect.
- SimpleScriptRegionAPIDocumentation.lua:516-527 declares IsProtected and its
  isProtected/isProtectedExplicitly results. Its own result uses the ObjectSecurity
  aspect; the generated receiver check reads it through canaccessvalue and defaults
  to deferral if inaccessible.
- RestrictedActionsDocumentation.lua:15-20 describes permission for protected
  functions **on the supplied object**, supporting receiver-dependent checks.
- SimpleFrameAPIDocumentation.lua:20 and SimpleRegionAPIDocumentation.lua carry
  IsProtectedFunction. The same flag is present at SimpleFrameAPIDocumentation.lua:20
  on Forever and :41 at every classic ref above.

At the retail ref, Interface/AddOns/Blizzard_RestrictedAddOnEnvironment/
SecureHandlers.lua:268 uses InCombatLockdown together with frame:IsProtected().
RestrictedInfrastructure.lua:111 and :602 deliberately select the second,
explicit-protection result. Selu uses the first result for unknown receivers.
HasRestrictions is broader access metadata; SecureOnly is an execution
**environment**, seen in CombatLogSecureDocumentation.lua and
PingManagerSecureDocumentation.lua. Neither is mapped to waiting for combat.
SecureOnly rejection is not implemented here.

No general hardware-event flag was found in the six committed manifests' global
restrictions, raw signature attributes or widget method attributes. Also searched:
docs/manifests.md and retail pinned InputDocumentation.lua (gamepad prose at
257,284,297,310). RequiresLimitedInput/MouseFocusValidForLimitedInput do not
establish a general click rule. Hardware-context rejection is therefore omitted.
No manifest was edited. See the report for exact JSON search paths.

C_CombatText.GetCurrentEventInfo is flagged secret but has no result declarations
in manifests/retail/12.1.5.70391.json and manifests/forever/1.60.1.70532.json.
Both corresponding refs agree in Interface/AddOns/Blizzard_APIDocumentationGenerated/
CombatTextDocumentation.lua:21-27: SecretReturns=true, empty Returns. No shape or
guard is invented for it.

The simulator exposes combat transitions, IsProtected state, secret sentinels,
canaccessvalue/issecretvalue and normal error capture. SetText/GetText retain a
secret sentinel. This models selected behavior, not the client's full taint,
secure-action, inherited-protection or rendering system. Package F is not needed.

## Reasoned reads

`readNumber(() => value)`, `readBool(() => value)` and
`readString(() => value)` invoke a zero-argument getter under protection.
They return `{ok: true, value: number | boolean | string}` (the specific
reader fixes the value type) or `{ok: false, reason: "none" | "secret" | "unreadable"}`.
Use `if (result.ok == true)` to narrow the discriminated union. The getter
may return Secret<T>; no opaque payload is exposed in the failure arm.

These follow Toolkit Core.lua SafeNumber/SafeBool: getter failure is
unreadable; absent numbers are none; absent booleans are unreadable. Numbers
must have number type; booleans use Lua truth conversion (including truthy
zero and empty strings). A protected issecretvalue probe precedes all payload comparisons, including the nil check, and the protected
arithmetic/truth proof. A failed proof is secret. The string variant requires
string type and proves concatenation; absence is unreadable. False is preserved.
This read point exposes a safe reason, never a secret value to branch on.

## Taint safety

All supported WoW products, including Classic and PTR/beta channels, have
secure UI. Selu checks addon code on every target. These checks detect risky
boundaries; they do not prove that an addon is taint free in combat.

SELU0604 warns about `hook` on foreign objects or global functions, and
`hookScript` on foreign objects for `OnShow`, `OnHide`, `OnEnable`, and
`OnDisable`. Such callbacks run synchronously inside Blizzard's call and can
taint secure UI. Poll `IsShown` from an owned ticker or frame instead.

```selu
hookScript(UIParent, "OnShow", () => {}); // SELU0604
hook(UIParent, "Show", () => {});         // SELU0604
hook("GetTime", () => {});               // SELU0604
const owned = CreateFrame("Frame");
hookScript(owned, "OnShow", () => {});    // owned receiver
```

Ownership is a conservative source-origin proof, following the lowering
purity analysis: a unique binding initialized by `CreateFrame`, a visible
Selu constructor, or an object/array literal is owned. Aliases retain proof;
a reassigned binding loses proof. Binding and class-field identities come
from the checker, so unrelated names in another scope do not invalidate proof. A frame created
with an owned parent is owned. Class fields retain proof when every visible non-null initializer or store
produces an owned value. Foreign stores invalidate that proof. Fields of
owned object literals can retain their initializer's proof.

Only manifest game types can be foreign: widgets, widget-like API types,
manifest API tables and manifest namespace receivers. A game value has one of
three origins:

- **owned**: proven created by this addon (see above);
- **foreign**: proven to come from the game. That is a manifest global no
  Selu binding shadows, a game API result that is not a factory (for example
  `GetMouseFocus`, or `GetParent` on a foreign value), a field, index or
  method result of a foreign value, or a binding or class field whose every
  store is foreign;
- **unknown**: anything else, such as a frame returned by the addon's own
  factory function, a parameter or an import.

By default SELU0604 and SELU0605 report only **foreign** values, so frames an
addon builds in its own helpers do not warn. `[build] strict_taint = true`
also reports **unknown** values and makes both checks errors; use it to audit
code where frames are passed in from elsewhere. The string form
`hook("GlobalFunction", ...)` always targets the game. Selu class instances (including
`this`), interfaces, source object types, arrays, maps, sets and functions
are never foreign. A union is a game boundary only if an arm is a manifest
game type; collection element types and object fields do not make the
containing collection or object foreign. Selu collection callbacks and
Selu-defined methods therefore do not trigger SELU0605.

Hook checks apply only to the checker-resolved `hook` and `hookScript`
built-ins. A local declaration shadows the built-in throughout its scope.

SELU0605 warns when a function value is passed to a method on a foreign
receiver or assigned to a foreign field (including indexed fields):

```selu
foreignView.SetExtentCalculator(() => 24); // SELU0605
foreignObject.callback = () => {};         // SELU0605
```

The small safe registry list lives in `selu-check/src/taint.rs`: `C_Timer.*`,
`CallbackRegistryMixin.RegisterCallback`, `EventRegistry.RegisterCallback`, and
`EventRegistry.RegisterFrameEventAndCallback`. CallbackRegistry's
`TriggerEvent` uses `secureexecuterange` (client `CallbackRegistry.lua`,
lines 200–204, owner evidence SL-34). An arbitrary method named
`RegisterCallback` is not enough to prove that receiver uses this registry.

SELU0606 rejects calls in the shipped deny table in `selu-check/src/taint.rs`.
The table records owner-observed SL-34 incidents and alternatives: the three
bank table getters should use `C_Bank.FetchNumPurchasedBankTabs`; bank purchase
and talent/spellbook opening need secure click proxies; `ToggleWorldMap` and
`OpenWorldMap` should use `C_Map.OpenWorldMap(mapID)`. The incidents were on
Forever; these APIs are also rejected on other targets where they exist.
The checker does not introduce missing manifest declarations.

Project rules augment that table:

```toml
[lint]
deny = [{ api = "Example.Unsafe", reason = "Our documented incident" }]

[build]
strict_taint = true
```

`strict_taint` also reports values of unknown origin and promotes SELU0604
and SELU0605 to errors. Denied calls
are always errors. API names and reasons in project rules must be nonempty.

Allow one expression with a mandatory nonempty literal written reason:

```selu
allowTaint("Reviewed secure caller at this site", ToggleSpellBook());
```

For a field assignment, wrap its function value, for example
`foreignObject.callback = allowTaint("Reviewed registry", () => {});`.

The wrapper is erased during lowering and preserves the expression's value.
It suppresses the wrapped occurrence only; nested calls retain their checks.
An empty or missing reason is SELU0607. It does not suppress type checking or
secret-value errors. Secret values used as table keys, for reads or writes,
are rejected with SELU0601 just like other opaque secret reads.
