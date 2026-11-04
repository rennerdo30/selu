# Frame templates

CreateFrame with a literal fourth argument retains its widget type and adds
members from every comma-separated template (whitespace is ignored):

```selu
const panel = CreateFrame("Frame", null, UIParent, "BackdropTemplate");
panel.SetBackdrop({ bgFile: "Interface/Tooltips/UI-Tooltip-Background" });
const button = CreateFrame("Button", null, panel, "UIPanelButtonTemplate");
button.SetText("Sell junk");
```

Template inheritance and mixin inheritance are followed transitively with cycle
protection. M3 fields and methods are names only and remain untyped. Existing
widget signatures still apply, including strict_api's usual signature checks.
Unknown template literals warn (SELU0580) even in strict mode, since addon XML
and incomplete manifests are valid sources of templates. Dynamic template
strings retain the plain widget type without a warning.

A member known through a template on one selected target but absent on another
is an error naming the missing target (SELU0581). This availability rule applies
in both layouts and both strict modes. Open widget fields remain open; an unknown
member is not evidence of a missing API. Merely naming an unknown template does
not turn open widget calls into errors.

Named XML globals retain their template members. Where M3 recorded a concrete
$parent child, its suffix is accessible as a field, for example
GameTooltip.TextLeft1 when GameTooltipTextLeft1 is recorded. The child keeps its
widget type and templates. Template identities survive ordinary aliases and
module exports, using internal ancestry metadata; no Lua wrapper is emitted.

The templated-frames example runs offline. The simulator models BackdropTemplate
SetBackdrop/GetBackdrop and SetBackdropColor/GetBackdropColor as stored data;
UIPanelButtonTemplate uses the existing button text and geometry model. XML
scripts, border textures, rendering and arbitrary mixins are not simulated.
Actual client appearance remains an owner check.
