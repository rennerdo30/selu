# Keybindings

```ts
keybinding("MyAddon_TOGGLE_PANEL", L`Toggle the panel`, () => {
  print("Panel toggled");
}, L`My addon`);
```

Arguments are a literal name, string label, callback callable without arguments,
optional string header, and optional literal `runOnUp` boolean (default false).
This follows `command` and uses positional options to keep the initial surface
small. A header is supported; a separate category option is deferred. With
runOnUp true the client invokes the action on both press and release; the callback
currently receives no key-state argument. Use the default for toggle actions.

Use ASCII letters, digits and underscores, starting with a letter or underscore.
The build requires the exact addon name followed by `_` and a nonempty action,
and rejects duplicate names across reachable modules. Choose an addon name that
is itself an ASCII identifier. Diagnostics SELU0660-0666 cover arity, identifier,
addon prefix, labels, callback, runOnUp and duplicate names. Local declarations
shadow this built-in just as they shadow `command`.

The build writes `Bindings.xml` beside the addon Lua in every target folder, or
once in shared layout. It never adds this file to the TOC: the client discovers
that special filename in the addon directory. Each XML entry calls
`MyAddon_TOGGLE_PANEL_Binding()`, a global holding the callback. Lua also sets
`BINDING_NAME_MyAddon_TOGGLE_PANEL` and, when supplied,
`BINDING_HEADER_MyAddon_TOGGLE_PANEL`. The XML header uses that binding identifier;
its text lives in the header global. Labels and headers accept existing `L`
templates and participate in locale key collection and the existing runtime
translation table. They are evaluated at registration time.

Declare bindings during module initialization, unconditionally, so every XML
entry has a registered callback before input begins. Binding callbacks close over
ordinary module locals. These names are public client entry points: do not reuse
them for your own globals. Bindings do not assign default keys or bypass secure
client restrictions.

The simulator does not model keyboard binding dispatch. Its TOC loader continues
to load the Lua while ignoring the adjacent unlisted Bindings.xml. Tests can
invoke the generated global directly; a future simulator input action can use
that same entry point. Native client input acceptance remains an owner check.
