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

Named XML globals retain their template members. M3 records global child names,
but not parentKey relationships. A global-only child has no implied parent
field: use MerchantFrameTab1 directly, rather than MerchantFrame.Tab1. A suffix
access matching a recorded global is rejected with SELU0582 and a help line
naming that global. GameTooltipTexture1 follows the same rule. Other open addon
fields keep the usual T1 policy. Parent fields can be modeled when manifests
record actual parentKey relationships; this package does not invent them.

Template and mixin lists apply in order: later definitions override earlier
ones, including untyped definitions overriding typed members. Documented widget
methods keep precedence. Cycle detection is per traversal path so repeated
applications of an ancestor still participate in order. Template identities
survive aliases and module exports without a Lua wrapper.

Availability uses T2's scoped has(...) facts, including early-return guards,
in both layouts. Each unknown template entry has its own diagnostic span;
escaped string characters retain their original source offsets.

Manifest widget names are also valid written types, for example
`function style(f: Frame) { f.SetSize(240, 100); }`. A templated Frame fits
Frame, and derived widgets retain their existing nominal assignability.
Widget declarations, class names and type aliases retain their usual precedence.

The templated-frames example runs offline. The simulator models BackdropTemplate
SetBackdrop/GetBackdrop and SetBackdropColor/GetBackdropColor as stored data (getters return no colour without a backdrop, clearing removes
colour, and GetBackdrop returns a deep copy);
UIPanelButtonTemplate uses the existing button text and geometry model. XML
scripts, border textures, rendering and arbitrary mixins are not simulated.
Actual client appearance remains an owner check.
