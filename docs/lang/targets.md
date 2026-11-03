# Targets and capability checks

Every requested client must accept an unguarded API use. Availability errors
(SELU0500) list the clients that lack it and suggest `has(...)`. Builds still run
the checker for each target, so each target's signatures and return policies apply.

`has(X)` is a boolean intrinsic accepting one manifest function, namespace, global,
or event. X must be known on at least one requested target. It does not evaluate X.
A local `has` is an ordinary function; a local namespace cannot be queried as a
manifest capability. Invalid queries produce SELU0501.

A manifest namespace root is not a value: storing, passing, returning or exporting
`C_Foo` is SELU0502, with the fix to call `C_Foo.Bar(...)` directly. Namespace
roots remain valid as member receivers and in `has(C_Foo)`. Function references
are values and are availability-checked at the full member expression, including
when exported/imported. Enum containers and members participate in capability
checks too; enum members have numeric types. Fallback typing declarations are
separate from availability: an explicitly unavailable function may borrow a
usable declaration for checking a guarded branch without becoming available.

A true branch of `has(X)`, the else branch of `!has(X)`, and code following
`if (!has(X)) return;` allow X. Namespace guards select clients with that namespace; individual members must
still exist on those clients. Facts follow
block scopes and short circuit conditions. They remain valid in closures because
client availability does not change during a session. Other code must guard its
own uses. Guarding availability does not waive argument or return type checks.

The default `[build] layout = "per-target"` writes one addon folder per target
when several are selected (the existing `dist/Name-flavour/` convention). Queries
become booleans, and directly guarded if branches are removed, retaining a `do`
block for lexical scope. Compound capability conditions respect short circuit
evaluation; side effects before a capability query still execute. Every `else if`
arm is folded too. A single target retains `dist/Name/`.

`[build] layout = "shared"` writes `dist/Name/` and one TOC whose Interface line
lists every selected manifest's interface number in project order, separated by
commas (including the space after each comma). A shared build requires every
target to declare the same Lua dialect set; mixed dialects are rejected with
SELU0503. Per-target builds can use different dialects. Unknown
build keys and layout values are errors. Dotted queries become short circuit nil
checks at every component. Shared event queries use a private probe frame and
`pcall(frame.RegisterEvent, frame, event)`: failure means the client lacks the
event. A successful probe unregisters the event immediately. The boolean result
controls handler registration, so unknown events never reach the dispatch frame.
The frame and captured `pcall` are local to each generated module, created only
when shared event queries are present. This has no dependency on
`C_EventUtils.IsEventValid`. The event dispatch prelude recognizes nested handlers.

The six current flavours can share a build with:

```toml
[addon]
name = "AllClients"
targets = ["retail", "forever", "classic-era", "classic-anniversary", "classic-titan", "classic"]
[build]
layout = "shared"
```

Their current manifest interfaces produce:
`## Interface: 120105, 16001, 11509, 20506, 38002, 50504`.
These numbers come from the resolved manifests; none are compiler constants.

`--target` overrides the selected targets for either layout. No flavour names or
interface numbers are hardcoded. See `examples/multi-target` for a shared addon.
