# Secret values and combat lockdown

Secret results become optional: `const name = UnitName("player") ?? "";`.
An `if (name != null)` check works too. An API still returns its first result in
scalar use and all positions by destructuring; guards do not allocate a tuple.
Untyped results remain untyped. False is preserved; inaccessible values become nil
before Selu tests, compares or computes with them.

`UnitName` returns two optional strings on retail and Forever, whose manifests
mark SecretWhenUnitNameIdentityRestricted. Single-target builds of the four classic flavours retain two
ordinary strings. `UnitHealth` is optional on retail and Forever too. Builds check
every selected target: a secret result on any target requires handling it.
`has(UnitName)` only proves availability, never accessibility of its results.
Restriction policies are merged before signature checking and lowering, so shared
output and per-target output retain the strictest policy independently of target
order. This merge changes only internal restriction attributes, never manifests
on disk or unrelated API signature metadata.

A function-level SecretReturns, SecretWhen... or SecretIn... policy applies to
all results unless individual SecretValue/ConditionalSecret positions specify the
policy more precisely. NeverSecret always excludes that position from guarding.
MayReturnNothing and Nilable remain independent reasons for optionality.
For example UnitClass marks its className conditionally secret; its other results
are public, although MayReturnNothing still makes them optional. UnitCastingInfo
explicitly marks isTradeskill, castBarID and delayTimeMs NeverSecret.
Conditional function values retain the union of both runtime policies, even
though API identity metadata does not participate in type assignability.
ConditionalSecretContents describes contents of a returned table; this release
does not recursively sanitize tables, structures or secret keys.

Protected calls used as statements run immediately outside combat and queue while
InCombatLockdown() is true. Every context is considered combat possible: file-load
setup (addons may load on demand), ADDON_LOADED, PLAYER_LOGIN, other events,
commands, hooks, timers, click handlers and ordinary functions. There is no
interprocedural proof of a safe setup context. A used protected result, including
a condition, destructuring, argument or alias call, is SELU0600; call as a
statement and observe state after combat instead.

The FIFO captures the function, receiver and argument values at the original call,
including nil arguments. One frame registers PLAYER_REGEN_ENABLED per emitted
runtime. Outside combat the helper allocates no table or closure. Drain checks
combat before every item; renewed combat retains the remaining items. A call
queued again is appended as a new call after existing pending calls. Deferred
work keeps its original arguments, rather than re-reading changed variables.
A queued error propagates through the normal event error boundary; remaining
items stay pending until the next regeneration event. There is no persistence
across reloads. Package K can relocate `emit_restriction_runtime` once per addon;
until then a module using deferral owns its runtime and FIFO.

`[build] debug = true` prints `Selu: deferred protected call` once each time a call
is enqueued, without printing its arguments. Release builds leave that branch
false. The `selu debug` command is unrelated and remains a stub.

## Evidence and deliberate gaps

All evidence below is source evidence, not live client verification. The clone
E:/Development/wow-ui-source was inspected only with git show and git grep.
The refs are the exact commits in the committed manifest source objects:

| Flavour | Ref |
|---|---|
| retail | 18c6d677b605b312f3d813d498404631fb941977 |
| forever | ca8368f2f2380cbe696854f7f2646387c278a61b |
| classic-era | 4192ac60067fc55566182291bc67c85dd9b6794d |
| classic-anniversary | 68e0a2ec559d9761dfc4e7f864c3944f9d758a49 |
| classic-titan | 7014c309a25338905c41dc3799a93f9719ebb533 |
| classic | e5d9b3a22c3f0f6f41ebac0e454f4969594261cf |

At every ref, Interface/AddOns/Blizzard_SharedXML/Dump.lua:106 uses
`canaccessvalue(val)` before inspecting a value; line 113 branches on inaccessible
values. This is the runtime accessibility predicate used here. At the Forever ref,
that file:312 also uses issecretvalue to format a secret differently. Accessibility
is the relevant test: the compiler must avoid inspecting inaccessible values.
The runtime feature-checks canaccessvalue before using it. A manifest without
an available canaccessvalue entry receives no emitted secret guard. Classic manifests
currently mark no secret function results, so their builds emit no secret guards.

At the retail ref, Interface/AddOns/Blizzard_APIDocumentationGenerated/
UnitDocumentation.lua records SecretReturns, SecretWhen... and result-specific
ConditionalSecret/NeverSecret fields (UnitHealth, UnitName, UnitClass and
UnitCastingInfo). The same raw fields are preserved in the manifests.

The combat mapping is **IsProtectedFunction = true** in a signature. At the
retail ref, SimpleFrameAPIDocumentation.lua (AddRoleset, Hide, Show and others),
SimpleRegionAPIDocumentation.lua and TooltipComparisonDocumentation.lua provide
the explicit protected-function flag. SimpleFrameAPIDocumentation.lua has that
flag at line 20 on retail/Forever and line 41 at each classic ref above.
RestrictedActionsDocumentation.lua:15-20
explains permissions to call protected functions on objects. SecureTemplates.xml:16
under Interface/AddOns/Blizzard_FrameXML checks InCombatLockdown before a secure
click action. The flag is applied conservatively even to an ordinary unprotected
frame, accepting unnecessary deferral in exchange for predictable behavior.

HasRestrictions is not a synonym: it occurs alongside RequiresFriendList,
RequiresClubsInitialized, secret restrictions and other access predicates.
SecureOnly is an execution environment, seen in CombatLogSecureDocumentation.lua
and PingManagerSecureDocumentation.lua at the retail ref. Waiting for combat to
end cannot make addon code secure, so neither flag is mapped to deferral.
Secure-environment rejection is not implemented in this package.

No explicit hardwareClick/requiresHardwareEvent policy was found in the functions'
restrictions or signature attributes of any of the six manifests. Searches also
covered their widget method attributes, docs/manifests.md and InputDocumentation.lua
in the retail pinned source. InputDocumentation.lua:257,284,297,310 describes
one-call gamepad hardware input in prose; RequiresLimitedInput and
MouseFocusValidForLimitedInput are not enough to infer a general click policy.
Hardware-context errors are therefore deliberately omitted pending sourced
manifest metadata. No manifest was changed.

The simulator exposes set_combat(bool), firing transition events only when the
state changes, InCombatLockdown, canaccessvalue, issecretvalue and opaque secret
sentinels through set_secret_global/secret_value. It models accessibility for
these sentinels, not the client's taint engine or all forbidden secret operations.
