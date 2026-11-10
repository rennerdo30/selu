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
targets = ["wow", "wowf", "wow_classic_era", "wow_anniversary", "wow_classic_titan", "wow_classic"]
[build]
layout = "shared"
```

Their current manifest interfaces produce:
`## Interface: 120105, 16001, 11509, 20506, 38002, 50504`.
These numbers come from the resolved manifests; none are compiler constants.

`--target` overrides the selected targets for either layout. No flavour names or
interface numbers are hardcoded. See `examples/multi-target` for a shared addon.

`[build] strict_api = true` promotes game API signature diagnostics to errors;
otherwise they are warnings. Availability failures remain errors in both modes.
Guarded fallback declarations retain API signatures, referenced structures and
widgets, multiple-result semantics, and factory specialization through aliases.

## Addon metadata

Optional `[addon]` fields become TOC lines that the AddOns list shows and the
addon can read with `C_AddOns.GetAddOnMetadata(addonName(), field)`:

```toml
[addon]
name = "MyAddon"
title = "My Addon"          # ## Title (default: the name)
notes = "What it does."     # ## Notes (default: "Built with Selu")
version = "1.2.0"           # ## Version
author = "Someone"          # ## Author
icon = "Interface\\AddOns\\MyAddon\\Media\\Logo"   # ## IconTexture
```

A value with a line break is SELU0506: it would end the field and start
another TOC line.

## Media and other addon files

`selu build` writes a complete addon, so the addon's media are copied into it.
When the code names a path inside the addon, such as
`"Interface\\AddOns\\MyAddon\\Media\\Logo"`, the build copies the project's
`Media` folder (matched without case, like the game) into every built addon
folder. A referenced folder the project lacks is warning SELU0504. Files or
folders kept elsewhere are listed explicitly; each keeps its own name:

```toml
[build]
assets = ["../Media", "Sounds"]
```

A missing listed asset, or one named like a file the build writes (the TOC,
`Bindings.xml` or a module folder), is error SELU0505. Unchanged files (same
size and time) are not copied again.

## Products and selu.lock

Use `targets = ["wow", "wow_classic", "wow_classic_era"]` or an exact pin such
as `"wow_classic@5.5.4.70631"`. Product version-line targets are rejected.
The transitional aliases retail, forever, classic, classic-era,
classic-anniversary and classic-titan remain accepted (including their old
version forms), with a warning naming the replacement product.

The first build creates `selu.lock` beside `selu.toml`:

```toml
version = 1

[targets]
wow = "wow@12.1.5.70783"
wow_classic = "wow_classic@5.5.4.70631"
```

Keys sort lexically. Exact project pins have their complete target as the key.
Builds retain these resolutions until `selu targets --update [product...]` is
run in the project directory. The command prints previous/new builds and added,
removed and changed functions, events, widgets, globals, named types/structures
(the tables section), predicates, enums, templates, data globals and frame families,
plus candidate source lines for removed or changed names and members. Candidate lines are textual references, not a claim
that a guarded use is invalid. Exact project pins stay exact. Major-version
changes require `--yes`, or Studio's confirmation dialog. Missing locked builds
produce an error with fetch-then-update recovery instructions.
Unknown selectors, including products absent from the project, are errors listing
valid project products and targets; a failed selection leaves the lock unchanged.
Studio Project Settings shows the lock and offers the same explicit update. It
preserves legacy selectors such as `retail@12.1` during ordinary edits and saves.
Explicitly choosing the corresponding product migrates a legacy selector to its
locked exact build; build the project first if no matching exact lock exists.
Run in Game keeps test products distinct, reads the install root's `.build.info`
and warns when its version differs from the locked version. The shared TOC
Interface line continues to come from the resolved manifests.
