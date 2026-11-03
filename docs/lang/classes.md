# Classes

Classes declare nominal instance types and class-object values. Types are visible
throughout their block, including classes wrapped in export. Value reads retain
use-before-declaration checks. A declaration location distinguishes nested
classes with the same name; derived instances fit base annotations.

A class has two separate tables. The class table holds static fields, static
methods and construction helpers. Its __proto table holds instance methods and
has __index pointing to itself. Instances use __proto as their metatable.
Derived class tables fall back to the base class table; derived prototypes fall
back to the base prototype. Thus a static f and an instance f can coexist, and
an instance override does not change an inherited static member.

```lua
local __selu_setmetatable = setmetatable
local Tracker = {}
local __selu_class_6 = Tracker
__selu_class_6.__proto = {}
__selu_class_6.__proto.__index = __selu_class_6.__proto
function Tracker.__proto:record(amount)
  self.total = self.total + amount
end
```

Compiler dependencies are captured before source statements and only in files
containing classes. Reserved class/base aliases keep constructor parameters,
locals and subsequent assignments from changing generated runtime references.
One instance table is allocated per new; method calls allocate no tables or
closures. Explicit rest parameters retain the allocation in functions.md.
Construction uses named parameters when known and varargs for inherited or rest
forwarding. The __init helper initializes an existing instance, including super
construction without a second instance allocation.

Each class has a shared __fields helper in the declaration's lexical scope.
Constructor parameters and body locals cannot capture field initializer names.
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
The runtime names new, __init, __fields, __proto and __index are reserved, as are
duplicate members. Class-specific diagnostics use SELU0420-0433.

A class instance may satisfy a structural object type when every requested
member is a data field (no function-typed members). Object literals cannot
satisfy class types: they have no class metatable or methods. Diagnostics suggest
new A(...) in declarations, assignments, arguments and returns. This one-way
compatibility intentionally differs from TypeScript's structural class typing.
Field-shape metadata is refreshed for class-valued expressions; non-class
expressions pass through that hook without copying or allocating.

D1 owns export checking/lowering. On this branch's 8da0740 base, export still
reports SELU0390; the class prepass already unwraps export. Runtime module
integration follows the parent's later rebase. Construction of values that are
not known class objects retains SELU0390.
