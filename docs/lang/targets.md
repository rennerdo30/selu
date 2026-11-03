# Targets and capability checks

Every requested client must accept an unguarded API use. Availability errors
(SELU0500) list the clients that lack it and suggest `has(...)`. Builds still run
the checker for each target, so each target's signatures and return policies apply.

`has(X)` is a boolean intrinsic accepting one manifest function, namespace, global,
or event. X must be known on at least one requested target. It does not evaluate X.
A local `has` is an ordinary function; a local namespace cannot be queried as a
manifest capability. Invalid queries produce SELU0501.

A true branch of `has(X)`, the else branch of `!has(X)`, and code following
`if (!has(X)) return;` allow X. Namespace guards select clients with that namespace; individual members must
still exist on those clients. Facts follow
block scopes and short circuit conditions. They remain valid in closures because
client availability does not change during a session. Other code must guard its
own uses. Guarding availability does not waive argument or return type checks.

The default `[build] layout = "per-target"` writes one addon folder per target
when several are selected (the existing `dist/Name-flavour/` convention). Queries
become booleans, and directly guarded if branches are removed, retaining a `do`
block for lexical scope. A single target retains `dist/Name/`.

`[build] layout = "shared"` writes `dist/Name/` and one TOC whose Interface line
lists the selected manifest numbers in project order, separated by commas. Unknown
build keys and layout values are errors. Dotted queries become short circuit nil
checks at every component. Event queries call `C_EventUtils.IsEventValid` (present
in both committed client manifests), so guarded event handlers register only valid
events. The event dispatch prelude recognizes nested handlers too.

`--target` overrides the selected targets for either layout. No flavour names or
interface numbers are hardcoded. See `examples/multi-target` for a shared addon.
