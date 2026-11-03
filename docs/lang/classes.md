# Classes

Classes declare nominal instance types and class-object values. Types are visible
throughout their block, including classes wrapped in export; values keep normal
use-before-declaration checks. Class identity includes its declaration location,
so a nested class with the same name is a different type. Derived instances fit
base annotations. Built-in written types retain the existing resolver behavior.

Fields need an annotation or initializer. Literal fields have signatures during
predeclaration; other initializer types and method return types are refined when
the declaration is checked. Methods and constructors use the existing parameter,
contextual-arrow, default, rest and return checking. As elsewhere in Selu,
untyped host values remain unchecked. Uninitialized annotated fields are the
constructor author's responsibility; this package adds no definite-assignment
analysis.

Each class owns a table with __index pointing to itself. A derived class table
has a metatable falling back to its base. Shared __init functions initialize an
existing instance, letting super initialize the same table without allocating a
second object. new allocates exactly one instance table and forwards arguments
to __init without a rest table. Ordinary methods use colon declarations and
calls; static methods use dot declarations and calls. Explicit rest parameters
retain the allocation documented in functions.md.

Parameter defaults run before instance field initializers. Root initializers
run before the constructor body. Derived constructors must start with super;
their initializers run immediately after it. An implicit derived constructor
forwards all arguments, then initializes its own fields. Static fields run in
source order after methods have been installed, so they may call static methods.
Constructors may return without a value; they cannot replace the instance.

this means the lexical instance receiver in constructors, instance fields,
methods and nested arrows. Non-arrow function expressions clear that context.
Static this and super are errors. A member binding self uses a reserved
__selu_self receiver; other methods retain readable self and colon declarations.
The reserved receiver survives callback parameters and block locals named self.

super.method(args) calls the base function with the same receiver. The checker
requires compatible override signatures. Repeated super construction, duplicate
members and collisions with new, __init or __index are errors; those names belong
to the class runtime layout. Class and parameter names must obey Lua/compiler
name restrictions. Diagnostic codes SELU0420-0432 cover class-specific errors;
existing assignment, function-call and use-before-declaration diagnostics remain.

The module package owns export checking and lowering. D3's name prepass unwraps
export and its normal class dispatch is reusable by that package. Until D1 is
merged, export still reports SELU0390; runtime tests use a single file.
Construction of values that are not known class objects still reports SELU0390.

The derived declaration captures its base value in one reserved local. Both the
class metatable and super calls use that binding, so a method parameter/local
cannot redirect super and an aliased base has the same behavior as a direct one.
This local retains the extends value without allocating another table.
