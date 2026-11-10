# The UI toolkit: `@selu/ui`

`@selu/ui` is the base UI toolkit that ships with Selu (SL-42). It is the
design system of Renner's Toolkit, as typed classes and builder functions:
windows, buttons, check boxes, sliders, segmented controls, edit and number
fields, scroll areas, lists with columns, section bars, sidebars, popup and
choice menus, tooltips, glyph buttons, item slots and empty states, with a
dark and a light theme, colour tokens, spacing and typography.

```selu
import { ConfigureUi, CreateButton, Window } from "@selu/ui";
```

Everything is imported from `@selu/ui`; the package's internal files are not
importable (docs/lang/packages.md).

## Connecting it to an addon

The kit keeps no saved data and creates no globals of its own. The addon
tells it, once and early, where its log, strings and settings live:

```selu
import { ConfigureUi, RefreshKitLocale, SetTheme } from "@selu/ui";

ConfigureUi({
  log: (level, message) => { /* the addon's log */ },
  translate: (key) => key,          // caller keys -> text (labels, tooltips)
  locale: () => GetLocale(),        // the kit's own sentences follow this
  namePrefix: "MyAddon",            // for the few frames with a global name
  saveTheme: (name) => { /* keep the chosen theme */ },
  loadWindowPosition: (key) => null,
  saveWindowPosition: (key, position) => {}
});
```

- Every field is optional. Without a host the kit logs nothing, shows caller
  keys as given, uses the client locale and keeps theme and window positions
  for the session only.
- Restore a saved theme with `SetTheme(name, false)` once the addon's saved
  data is ready (`false`: don't save it again).
- Call `RefreshKitLocale()` when the addon's locale changes; labels built
  from keys update in place.
- The kit's font objects are named `SeluUIFont_<role>`; frames created later
  use `namePrefix`.

Renner's Toolkit's port shows a complete host: `selu/src/core/ui-host.selu`
on its `selu-port` branch.

## Products

The package lists `wowf` for now: the kit has been checked against Forever.
Other products are added as the kit type-checks against their manifests
(docs/lang/packages.md, SELU0408).

## Not yet

- Components in the SPEC-V1 section 4 style (declarative, with a layout pass)
  are a later compiler feature; the classes stay as the base.
- Package art (the toolkit's own textures) and a neutral default theme next
  to the toolkit's dark and light ("Gilded") themes.
