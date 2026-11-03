# Classes

Classes declare nominal instance types and class-object values. Types are visible
throughout their block, including classes wrapped in export. Value reads retain
use-before-declaration checks. A module and declaration location distinguish nested
classes with the same name; derived instances fit base annotations.

A class has two separate tables. The class table holds static members; its
plainly named methods table holds instance methods and has __index pointing to
itself. Instances use methods as their metatable. Derived class tables fall
back to the base class table; derived method tables fall back to base.methods.
Thus a static f and an instance f coexist without changing either convention.
The name methods describes the table's role in generated Lua; it is reserved on the static side
alongside new and the construction helper names. Instance members named methods
remain independent.

```lua
local __selu_setmetatable = setmetatable
local Tracker = {}
Tracker.methods = {}
Tracker.methods.__index = Tracker.methods
function Tracker.methods:record(amount)
  self.total = self.total + amount
end
```

Class and base names are used directly unless a binding in the class code can
shadow them. Only those cases get reserved __selu_class_N or __selu_base_N
aliases. Mutable base aliases are captured because extends fixes the base value
when the class is declared. Base/helper planning follows lexical scopes and uses
the checked class identity of aliases; declarations inside an unrelated function
do not force aliases or select a different base. Self-references inside a class do not force a forward
local: local Tracker = {} suffices. Genuine earlier uses still get a forward
local, preserving closures that construct a class declared later.

A standalone class or local leaf subclass constructs directly in new, with
named parameters when known. Bases with subclasses in the file retain __init
so derived constructors initialize the same instance. Exported classes retain
that entry too, since their subclasses can live in other modules. All classes
still allocate exactly one instance table per new; method calls allocate none.
Explicit rest parameters retain the allocation described in functions.md.

Field initializers normally appear directly in new or __init. Only an actual
lexical conflict with constructor parameters or forward locals requires a shared
__fields helper. Free-name analysis reuses the module walker's lexical scopes;
an arrow's own parameters do not create a false conflict. Receiver hygiene is
shared across the inlined fields and constructor. Simple compound assignments
on this or stable names, with a pure right operand, emit direct field updates.
Other updates keep the existing evaluation-order protections.

Instance initializers are checked as delayed code, while static initializers
are eager. Fields need an annotation or initializer; checked initializer types
and method/default-parameter types refine their signatures. Uninitialized typed
fields remain the constructor author's responsibility: no definite-assignment
analysis is added.

Initialization follows standard JavaScript class field order. Base fields run
before parameter defaults and the body (xdA in the order regression). Derived
defaults run first, super runs the base initialization, then derived fields run
immediately after super returns, followed by the rest of the body (daAbB).
These strings were verified using the installed Node runtime. Whole constructor
bodies are lowered together before inserting the field call, preserving forward
locals even when a closure passed to super captures a later declaration.
Static fields execute in source order after methods are installed.

Only declared instance methods receive an implicit Lua receiver and use colon
calls. Function-valued fields use dot calls, including arrows capturing this.
Method markers survive calls through base-typed variables. this denotes the
lexical instance in fields, methods, constructors and nested arrows. Function
expressions and function declarations clear that context. Static this/super are
errors. Reads of this during derived defaults or evaluated super arguments are
errors until super returns; creating an arrow that reads this later is allowed.
Nested class declarations start their own validation context.

Any use or binding of the source name self in a member (including defaults,
outer free names and nested classes) selects the protected __selu_self receiver.
Other methods keep readable self and colon declarations. This preserves ordinary
WoW callback parameters named self without reserving that source identifier.

Overrides must preserve the base member's type, including fields and statics.
Instance fields and declared methods also retain their calling convention.
Constructor diagnostics show the expected signature; override diagnostics name
the expected member type. Parameter diagnostics underline only the name.
The runtime names new, methods, __init, __fields, __proto and __index are reserved, as are
duplicate members. Class-specific diagnostics use SELU0420-0433.

A class instance may satisfy a structural object type when every requested
member is a data field (no function-typed members). Object literals cannot
satisfy class types: they have no class metatable or methods. Diagnostics suggest
new A(...) in declarations, assignments, arguments and returns. This one-way
compatibility intentionally differs from TypeScript's structural class typing.
Field-shape metadata is refreshed for class-valued expressions; non-class
expressions pass through that hook without copying or allocating.

Module integration carries class signatures through an additive classes registry
on ModuleExport. The registry includes constructors, instance/static members,
method calling conventions and ancestor identities, including transitive imports.
Class identities include the source module and declaration location; unrelated
classes named A in different files remain distinct. Imported names are declared
by D1's helper before any block signatures are built, while ordinary class value
reads retain use-before-declaration checks. Methods and constructors establish
parameter/default/body scopes in D1's live-import walker, so local parameters
shadowing mutable imports remain plain local reads.

The class prelude stays one call per lowering entry. The project module path
places it immediately after local __selu_addon, __selu_ns = ..., before source
imports or declarations. WoW runtime setup follows the namespace fallback.
D4's builtin-call dispatch remains ahead of data/class/generic calls, and class
methods can register commands, log and return localized labels. Construction of
values that are not known class objects retains SELU0390.

The classes example has a full-string Lua comparison for both class definitions,
as well as the real addon build/load/output test. Shadowed class/base aliases,
standalone constructors, module identity and import scopes have regressions.
