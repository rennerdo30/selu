# Frame templates

CreateFrame with a literal fourth argument retains its widget type and adds
members from every comma-separated template (whitespace is ignored):

```selu
const panel = CreateFrame("Frame", null, UIParent, "BackdropTemplate");
panel.SetSize(260, 100);
const button = CreateFrame("Button", null, panel, "UIPanelButtonTemplate");
button.SetText("Sell junk");
```

Template inheritance and mixin inheritance are followed transitively with cycle
protection. M3 fields and methods are names only and cannot authorize a checked member access without a signature. Existing
widget signatures still apply, including strict_api's usual signature checks.
Unknown template literals warn (SELU0580) even in strict mode, since addon XML
and incomplete manifests are valid sources of templates. Dynamic template
strings retain the plain widget type without a warning.

A member known through a template on one selected target but absent on another
is an error naming the missing target (SELU0581). This availability rule applies
in both layouts and both strict modes. Widget members must be declared: unknown
reads, writes and calls report SELU0582 in both modes. An annotation, parameter,
return type or conditional cannot grant additional members.

Named XML globals retain their template members. A global-only child has no
implied parent field: use MerchantFrameTab1 directly, rather than
MerchantFrame.Tab1. A suffix access matching a recorded global is rejected with
SELU0582 and a help line naming that global. GameTooltipTexture1 follows the
same rule. Store addon state in typed Selu classes or objects instead of
attaching undeclared frame fields.

## Child frames by key

A child that the game's XML attaches to its parent under a key (`parentKey`)
is a typed, read-only field of the parent:

```selu
const inset = MerchantFrame.Inset;          // Frame, with InsetFrameTemplate
inset.Bg.SetAlpha(0);                        // Texture from the template
MerchantFrame.PortraitContainer.portrait.SetAlpha(0);
```

The manifests record, per extraction 15, the keyed children of every named
frame, every template and every keyed child below them (frames, textures,
font strings, lines, mask textures, animation groups and animations). A frame
has its own keys and those of its templates; a template's own key wins over
an inherited one and a later template over an earlier one. The child keeps
its widget type, its templates and its own keyed children, so chains work.
A named child of a template (`$parentInset`) is also a global per instance
(`MerchantFrameInset`) and has the same keyed children there.

Rules:

- The field is read-only (SELU0355). Change the child through its methods.
- A key recorded on only some selected targets is SELU0581, naming the
  targets without it. Guard with `has(...)` or use keys every target has.
- Only XML is read. A key that Blizzard's Lua assigns at runtime (for
  example the pieces of a `NineSlice`, or `frame.buttonPool`) is not
  recorded and stays SELU0582.
- An unkeyed anonymous child cannot be reached, so nothing below it is
  recorded.
- The child exists when its parent exists. For a frame of a load-on-demand
  addon, check the parent global first.

### Lists of children

Children that the XML collects in a list (`parentArray`) are a typed,
read-only array on the parent, recorded per extraction 16:

```selu
for (const tab of AuctionHouseFrameAuctionsFrame.Tabs) { tab.SetAlpha(1); }
```

The element type is the widget type every listed child shares. Frames of
different types are `Frame`; a list that mixes frames and regions is not
recorded. Elements carry the templates all listed children share. A key of
the same name wins over a list.

### Lua fields and methods of Blizzard mixins

Blizzard's Lua also assigns values on its frames (`self.isSelected = ...`)
and gives them methods through mixins. Neither is in the XML or in the API
documentation. Where an addon needs one, a reviewed overlay entry types it,
with the source line as evidence:

- `[[fields]]` types a value a mixin or template assigns. It is read-only.
  A build whose source lacks that mixin has no such field.
- `[[functions]]` named `<Mixin>.<Method>` types a mixin method.

The overlays currently type, on every family: the scroll list
(`ForEachFrame`, `GetFrames`, `RegisterCallback` for the frame events), the
tab system (`tabs`, `isSelected`, `GetTextYOffset`), `NineSlicePanelMixin`'s
centre and border colours, `PanelTemplates_GetSelectedTab`, and the
eight-coordinate form of `SetTexCoord`; on Forever also the profession rank
bar (`GetMaskWidth`, `overrideMaskRightOffset`). A mixin member without such
an entry stays as before: a name the manifests know but do not type.

### A template name is a type

A function can ask for "a frame with this template":

```selu
function paintRows(box: WowScrollBoxList, paint: (row: Frame) => void) {
    box.ForEachFrame((row) => { paint(row); });
}
paintRows(AddonList.ScrollBox, (row) => { row.SetAlpha(1); });
```

The type is the template's widget carrying that template, so the template's
keyed children and reviewed members are available on the parameter. A frame
fits only when it carries the template itself or through the templates it
inherits; a plain `Frame`, or a frame widened to `Frame`, does not. A widget,
class or type alias of the same name keeps precedence.

Template and mixin lists apply in order: later definitions override earlier
ones. An untyped override blocks access rather than falling back to an earlier
typed definition. Documented widget
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

So are the game data's documented structures and mixin values, so a helper
can take one as a parameter: `function id(recipe: TradeSkillRecipeInfo?)`,
`function red(color: colorRGBA)`. Widgets and templates of the same name
keep precedence.

The templated-frames example runs offline using declared Texture methods for
its background and UIPanelButtonTemplate for the button. SetBackdrop and
SetBackdropColor have source signatures in the shipped manifests; unannotated
parameter and result types follow the untyped game value policy in api.md. The simulator's Lua fixtures still model BackdropTemplate
SetBackdrop/GetBackdrop and SetBackdropColor/GetBackdropColor as stored data (getters return no colour without a backdrop, clearing removes
colour, and GetBackdrop returns a deep copy);
UIPanelButtonTemplate uses the existing button text and geometry model. XML
scripts, border textures, rendering and arbitrary mixins are not simulated.
Actual client appearance remains an owner check.

## Numbered Blizzard frames

`frameFamily("MerchantItem", index)` returns `Frame?`;
`frameFamily("MailItem", index, "Subject")` returns `FontString?`.
Check for null before calling methods. Blizzard may not have loaded the UI,
created a particular row, or populated an observed gap yet.

The prefix and optional suffix must be string literals present in every target
manifest. The index is a number using Blizzard's numbering (usually starting at
1), without Selu array index adjustment. A literal outside the observed bounds
is an error. Dynamic indices are nullable and are not clamped.

The compiler emits `_G["MerchantItem" .. index .. "NameFrame"]` directly.
This intrinsic is the only access it grants: `_G` remains forbidden in Selu.
A local declaration named `frameFamily` shadows the intrinsic normally.

`frameFamilies` stores one record per prefix/suffix with `widget`, `minIndex`
and `maxIndex`. Manifest generation splits the first decimal run in a typed
named widget global, rejects multi-digit leading zeroes and mixed widget types, and requires
at least two distinct indices. Each suffix has its own range; missing interior
indices remain safe because results are nullable. Existing template and XML
child expansion supplies widget types without guessed signatures.

Diagnostics: SELU0640 unknown family, 0641 unknown suffix, 0642 nonliteral
prefix/suffix, 0643 nonnumeric index, 0644 family absent on another target,
0645 literal outside bounds, 0646 arity, 0647 inconsistent target widget types,
0648 a forbidden `_G` binding.

Ranges describe source evidence, not guaranteed runtime existence. In retail's
pinned source, MerchantFrame.xml:127 defines MerchantItem1 (and later rows);
MerchantFrame.lua:1 sets MERCHANT_ITEMS_PER_PAGE to 10. The manifest scans all
source variants, whose XML instances extend through MerchantItem12; its observed
range is therefore 1..12. These are XML families. FloatingChatFrame.xml:698 and :729
define ChatFrame1 and ChatFrame2. FloatingChatFrame.lua:710 and :723 create
temporary ChatFrame tabs and ScrollingMessageFrames using maxTempIndex; its
runtime-dependent upper bound is not inferred. Those temporary indices remain
a coverage gap. No arbitrary string-based frame lookup is provided.

The generator also reads numeric Lua `for` loops around direct `CreateFrame`
calls with a literal widget and `"Prefix" .. loopIndex` name. Literal bounds
and scalar constants with retained numeric values produce source-evidence
families; `sources` records creation file and line. Counts from engine globals,
computed expressions, temporary frame counters, pools, and runtime suffix
expressions remain gaps. For example, CalendarDayButton's
CALENDAR_MAX_DAYS_PER_MONTH is not defined by the pinned source, so the
calendar loop at Blizzard_Calendar/Mainline/Blizzard_Calendar.lua:1079 is not
assigned an invented range. QuestTitleButton uses a frame pool at
Blizzard_UIPanels_Game/Mainline/QuestFrame.lua:303 and supplies no numbered
creation evidence. Retail runtime evidence does include
AchievementFrameSummaryAchievement, created in Mainline/Blizzard_AchievementUI.lua:2391.

The generator also follows frames a mixin method creates for each frame that
uses the mixin (`mixin_loops.rs`): a loop `for i = 1, self.<count>` whose
`CreateFrame` name is `"Literal"..i` or `self:GetName().."Infix"..i`, also
through a local assigned in an `if self == Frame` chain. The count and the
template are the XML `KeyValue`s of each named frame that inherits the mixin.
This types the action buttons (`ActionButton`, `MultiBarBottomLeftButton`,
`MultiBar5Button`, `StanceButton`, `PetActionButton`, ...) with their real
counts per product. A family proven this way keeps its prefix even when the
prefix contains a number. Shapes outside this pattern are skipped, never
guessed.

Family values retain their family identity through inferred aliases and null
narrowing. A named child global is not a proven field: after narrowing a
`frameFamily("MailItem", index)` value, `.Subject` reports SELU0582. Use
`frameFamily("MailItem", index, "Subject")` for the typed FontString lookup.
A single zero is valid (`frameFamily("CharacterBag", 0, "Slot")`); spellings
such as `01` do not define family members. Runtime creation evidence retains
all distinct widget types; conflicting types reject the whole family. Numeric
loop discovery resolves the index binding, excludes writes to it, and excludes
creation calls in nested functions whose execution need not match the loop.

Simulator sessions accept the already selected manifest in
`SessionConfig.manifest`, so runners with custom manifest locations can use the
same checked data. Otherwise Session resolves its target from the nearest
`manifests` directory; addon loading can infer a single project target from
`selu.toml` (or the flavour suffix for a split multi-target addon). Families are
installed before addon evaluation and retained across reload/replay, including
manually registered families. Unrelated missing APIs keep their existing
callable fallback and log-once behavior.

Compiler builds save the exact checked manifest in
`dist/.selu/<addon>/manifest.json`, outside the installed addon. Studio receives
that snapshot from `selu debug`; direct Session addon loads and reloads read it
as well. `selu test --manifest-dir ...` builds and checks tests against that same
selection and passes it into Session. Compiler and Session discovery share the
precedence: explicit directory, `SELU_MANIFEST_DIR`, project ancestors, then cwd
ancestors. Rebuilds preserve an explicit Studio manifest-directory override.

## Shared targets and multiple addons in the simulator

A compiled output's `manifest.json` contains one checked snapshot per canonical
target. Single-target output retains the legacy single-manifest form; shared
output bundles its snapshots in a `manifests` array. Session and Studio select
using the requested flavour/version, including partial-version resolution.
Multiple snapshots require an explicit target; an unavailable or mismatched
target is an error, including when loading a legacy single-target snapshot.

Session keeps checked metadata for every loaded addon, including dependencies
and load-on-demand addons. Compatible snapshots have the same canonical target
and identical widget type and observed range for every family/suffix they share;
disjoint families merge. Conflicting target identities or family definitions
are errors before the incoming addon's code runs. Loading another addon cannot
remove previously loaded families. Manual simulator registrations remain separate.

Reload reads and validates the whole loaded set before retiring the current VM.
Missing, incompatible or wrong-target snapshots fail without replacing that VM.
The new VM receives the complete validated registry before any addon executes,
and uses that same checked set throughout loading. Rebuilds publish snapshots
only after all target compilations succeed.

Pool and view factories carry generated checked object types. For example:

```selu
const pool = CreateFramePool("Frame", UIParent);
for (const [frame, active] of pool.EnumerateActive()) {
  frame.Hide();
}
const [frame, fresh] = pool.Acquire();
if (frame != null) { pool.Release(frame); }
```

Acquire may return null at capacity. Texture and font-string pool allocators
retain their respective base widget types. ObjectPool's arbitrary element data
and source results without a determined type use the existing API untyped rule.
ScrollBox views and DataProviders also retain their generated method surfaces.
Call an enumerator directly in `for ... of` so its Lua iterator triple survives;
saving just its first result and iterating it is rejected. NumLines and ClearLines
are supplied by pin-cited engine overlays; NumLines returns a number.
