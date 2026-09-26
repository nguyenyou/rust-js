# 0049. Traits carry dictionaries; ordinary values stay untagged

Status: Accepted. Extends [0047](0047-methods.md),
[0023](0023-strings-references-shared-state.md), and
[0039](0039-generic-bindings.md). Replaces 0047's rejection of trait methods
for the supported subset below.

## Context

`Circle { r: 2.0 }` is `{ r: 2 }`, and both an `i32` and an `f64` are JS
numbers. A payload cannot tell us which Rust trait implementation to call.
Generic functions also need implementations when there is no receiver:
`T::default()` cannot dispatch on a value that does not exist yet.

We lower THIR before monomorphization. rustc has already checked trait
obligations and can select concrete implementations. We want to use those
facts without duplicating every generic function or tagging ordinary data.

The public JS calling convention matters: JS users must be able to call
an exported generic function, including on an empty array. Module cycles
also matter: a hoisted function can run before its module's `const`s.

## Decision

**Concrete calls resolve statically. Generic functions receive dictionaries.
A trait object carries its payload and dictionary.**

```text
concrete type  -> rustc selects implementation -> direct function call
T: Shape       -> caller supplies TShape       -> TShape.area(value)
dyn Shape      -> value stores its impl        -> shape.impl.area(shape.value)
```

### Implementation location and names

An implementation belongs to the module containing its `impl`, even when
its type or trait is defined elsewhere. Nothing mutates the type's inherent
method object or a central trait registry.

Each implementation has a hoisted dictionary accessor: `circleShape()`,
`squareShape()`, `f64Shape()`, or `vecShape(TShape)`. Its name joins the
self type's name, with the first letter lowercased, and the trait's name.
Non-identifier punctuation in other self types is replaced by underscores.
Ambiguous generated dictionary names in one module are compilation errors;
put those implementations in separate modules. We do not silently number
public dictionaries according to discovery order.

Method bodies have private generated names such as `circleShape_area`.
They are exported when another module directly calls them. Dictionary
accessors are exported for public traits, or when another module uses them.
Dictionary method names follow the existing `camel_case` crate policy.
Original free-function names and inherent-method namespaces are unchanged.

```js
var $circleShape;

function circleShape_area(circle) {
  return 3.14 * circle.r * circle.r;
}

export function circleShape() {
  if ($circleShape === undefined) {
    $circleShape = {
      area: circleShape_area,
      name: circleShape_name
    };
  }
  return $circleShape;
}
```

`var` without an initializer is intentional. Its binding exists before
module evaluation, and reaching the declaration later cannot overwrite a
dictionary created by an early call through a cycle. This only solves
initialization of trait dictionaries; it does not change the initialization
semantics of existing inherent-method objects or arbitrary module values.

Generated helper globals are reserved against user bindings. A trait's
method keys and supertrait keys must be distinct after name conversion;
`__proto__` is rejected as a dictionary key.

### Concrete and generic calls

rustc's instance resolution selects a concrete method before lowering it.
`c.area()` on a known `Circle` becomes `circleShape_area(c)`.

A generic function keeps one body. Its ordinary arguments come first,
followed by dictionaries in predicate order, including inherited impl
bounds. Duplicate identical obligations are removed. The calling convention
comes from the signature, not which methods happen to be used in the body.

```js
export function total(shapes, TShape) {
  return shapes.map(s => TShape.area(s)).reduce((a, b) => a + b, -0);
}

export function fresh(TDefault, TShape) {
  return TShape.area(TDefault.default());
}
```

JS supplies the evidence explicitly:

```js
total([[1], [2]], squareShape());
fresh({ default: () => 3 }, f64Shape());
```

Unconstrained, representation-independent generics need no evidence.
Closures capture dictionaries like ordinary lexical variables. Taking a
generic function as a value binds its concrete dictionaries in an arrow
function, preserving the Rust function's original argument list.

`Copy` gets a compiler dictionary with a `copy(value)` operation. A generic
read of a copied aggregate must not accidentally alias its input. The
operation copies according to the concrete Rust representation; it does not
call a user `Clone`. Generic mutation of an aggregate conservatively marks
all instantiations of that aggregate as potentially needing copies.

Array and slice indexing used by generic functions checks bounds before
reading, and applies the appropriate Copy operation to the result.

### Generic implementations and supertraits

A generic impl is a dictionary factory. `vecShape(TShape)` binds the element
implementation into its methods. Factories cache dictionaries in lazy
WeakMaps, keyed by all dictionary arguments. The helper walks a WeakMap per
argument when there are several bounds. Builtin evidence objects constructed
at a call site need not have stable identity, so those calls may miss the
cache. Cache identity is an optimization, never a Rust type identity.

A known call to `Vec<Square>::area` directly calls the generic implementation
body with `squareShape()`; it does not construct a Vec dictionary first.

A supertrait is a named accessor on its subtrait dictionary:

```js
circleLabeled().Shape(); // the Circle: Shape dictionary
```

These accessors defer dependencies rather than constructing the whole
supertrait graph at module initialization. Generic code can obtain a
supertrait from an existing subtrait bound.

### Default methods

Default bodies are copied into each implementation's dictionary, with their
Self evidence specialized to that implementation. Calls inside a default
therefore honor overrides. Self is known there, so a call on it resolves
like any concrete call: Circle's copy of `label` calls `circleShape_name(self)`
directly, and a default it doesn't override goes through its accessor. The body's original Rust definition still
controls lexical name resolution: a private helper in the trait's module
remains a reference to that helper, exported internally if needed.
JavaScript bindings used by a copied default are imported into the
implementation's module as well.

This favors straightforward dispatch over deduplicating large defaults.
Sharing default bodies later is an internal optimization, not a JS ABI change.

### Trait objects

Read-only local trait objects use a named pair:

```js
const shape = { value: 3, impl: f64Shape() };
shape.impl.area(shape.value);
```

The same payload convention works for structs, tuples, enums, primitives,
and the supported erased Box/Rc/reference wrappers. Each conversion creates
a pair, not a collection of bound-method closures. Upcasts retain the payload
and obtain the supertrait dictionary; a conversion to the same trait is the
pair itself. Nontrivial receivers are evaluated once, in Rust argument
order, in a `const` before the call: `const receiver = make(c);`.

This does not make wrapper identity Rust pointer identity, and does not
provide equality, reference counts, `Any`, or downcasting. Mutable dyn
receivers are rejected: replacing a number or an entire struct requires a
writable storage location, which the current reference representation does
not supply generally.

### Initial supported boundary

This implementation supports local non-type-generic traits, handwritten
impls, defaults, supertraits, generic functions and impls over supported
representations, multiple bounds, receiverless methods, captured dictionaries,
function values, and read-only dyn calls and upcasts. Trait lifetime
parameters erase as other lifetimes do.

`Default` is also supported for handwritten local impls and the supported
primitive, String, Vec, and Option representations. This does not add
`#[derive(Default)]`. Existing supported derives keep their existing paths.

Associated types and constants, type-generic traits, generic trait methods,
const generics, and user implementations of other external/standard traits
remain errors. Generic `Option<T>` is rejected when T could itself be nullish;
this does not change the Option ABI. General writable references, runtime
type reification, user destructors, and arbitrary standard-library traits
are future work, not approximate implementations.

The running example's default label also requires f64 Display. The on-demand
formatter finds a shortest round-trip decimal using exact rational arithmetic
and emits Rust's decimal notation and special values. It favors correctness
and simplicity over formatting throughput. `f64::max` and `min`, including
function values, use helpers that ignore a single NaN operand as Rust does.
Floating-point sums start at negative zero, matching Rust's `Sum` identity.

## Why

- Existing payloads remain ordinary JS values, including values supplied by JS.
- Implementations on primitives and foreign types need no exceptional registry.
- Empty inputs and receiverless functions work because evidence is explicit.
- Generic code is emitted once; implementations can be passed and cached.
- Concrete calls avoid runtime selection.
- Lazy accessors cooperate with hoisted functions and cyclic ES modules.
- The compiler can reject missing representation machinery at its source span.

## Alternatives

- **Nest under the type (`Circle.Shape`).** Awkward ownership across modules,
  and primitives have no existing namespace to attach to.
- **Group under the trait (`Shape.Circle`).** Encourages eager registration and
  cross-module mutation rather than ordinary imports.
- **Monomorphize all functions.** Larger output and a less usable generic JS
  API. Selective specialization can be added behind this public convention.
- **Tag every payload.** Changes existing interop, requires boxing primitives,
  and still does not supply the type for receiverless generic operations.
- **Bound-method dyn objects.** Pleasant as a JS facade, but allocate closures
  per conversion and hide the payload needed for other operations.
- **Eager const dictionaries.** Shorter output, but unsafe for early calls in
  module cycles.

## Consequences and verification

Dictionaries and calls through them are visible in generated code. Exported
implementation accessors are non-component exports and can prevent a React
module from being a Fast Refresh boundary; reusable impls belong in separate
Rust modules. No extra generated companion modules are introduced.

The existing eager iterator policy in ADR 0036 is unchanged. This decision
does not claim to fix that policy's effect-order differences. A later lazy
iterator implementation is separate from trait dispatch.

Which modules a body uses is known only once it is lowered: a resolved trait
call or a copied default can reach a module its Rust does not name. So the
crate is lowered twice. The first pass reserves every module's import alias
and records the uses; the output is the second, reserving aliases only for
modules actually used, so an unused module never renames a local (a crate
where every module uses every other is lowered once).

Source spans are retained on method bodies. A copied default from another
source file currently has no mapping for those out-of-file spans: existing
maps associate each emitted module with its own Rust source file.

The public dictionary names, evidence argument order, and dyn pair shape are
ABI decisions. Cache strategy, default-body sharing, and internal
specialization can change independently. Dictionary objects supplied by JS
must satisfy the declared operations; mutating compiler dictionaries is not
part of the interop contract.

`examples/traits.rs` is the complete running example; its generated `demo()`
returns 13.14. `test/traits.test.ts` compares native Rust and generated JS for
concrete/generic/dyn dispatch, inherited defaults, generic Copy, multiple
bounds, receiverless calls, nested impl factories, closures, function values,
upcasts, and evaluation order. It also checks JS callers, lazy initialization
through module cycles, cross-module default resolution, f64 formatting, and
source-located rejection of unsupported cases.
